use std::cmp::Ordering;
use std::fmt::Write as _;
use std::fs::File;
use std::fs::create_dir_all;
use std::io::Error;
use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::prelude::*;
use bevy_kana::ToF64;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use crate::constants::BENCHMARK_CSV_HEADER;
use crate::constants::BENCHMARK_RESULTS_BANNER;
use crate::constants::BENCHMARK_RESULTS_FILE_PREFIX;
use crate::constants::BENCHMARK_RESULTS_FILE_SUFFIX;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_AVERAGE;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_FPS;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_FRAMES;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_MAX;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_MEDIAN;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_MIN;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_PERCENTILE_95;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_PERCENTILE_99;
use crate::constants::BENCHMARK_RESULTS_TABLE_HEADER_SCENARIO;
use crate::constants::DATE_COMMAND;
use crate::constants::DATE_COMMAND_ARG_FORMAT;
use crate::constants::DATE_COMMAND_ARG_REFERENCE_TIME;
use crate::constants::MEDIAN_PERCENTILE;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::constants::NINETY_FIFTH_PERCENTILE;
use crate::constants::NINETY_NINTH_PERCENTILE;
use crate::constants::RESULTS_DIRECTORY_NAME;

#[derive(Clone)]
pub(super) struct ScenarioResult {
    pub(super) name:          String,
    pub(super) frames:        u32,
    pub(super) average:       f64,
    pub(super) median:        f64,
    pub(super) percentile_95: f64,
    pub(super) percentile_99: f64,
    pub(super) min:           f64,
    pub(super) max:           f64,
}

impl ScenarioResult {
    pub(super) fn average_frames_per_second(&self) -> f64 {
        if self.average > 0.0 {
            MILLISECONDS_PER_SECOND / self.average
        } else {
            0.0
        }
    }
}

pub(super) fn compute_statistics(name: &str, frame_times: &mut [f64]) -> ScenarioResult {
    frame_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let len = frame_times.len();
    let sum: f64 = frame_times.iter().sum();
    let average = sum / len.to_f64();
    let median = percentile(frame_times, MEDIAN_PERCENTILE);
    let percentile_95 = percentile(frame_times, NINETY_FIFTH_PERCENTILE);
    let percentile_99 = percentile(frame_times, NINETY_NINTH_PERCENTILE);
    let min = frame_times.first().copied().unwrap_or(0.0);
    let max = frame_times.last().copied().unwrap_or(0.0);

    ScenarioResult {
        name: (*name).to_string(),
        frames: len.to_u32(),
        average,
        median,
        percentile_95,
        percentile_99,
        min,
        max,
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let len_f64 = (sorted.len() - 1).to_f64();
    let idx = (pct / 100.0 * len_f64).round().to_u32().to_usize();
    sorted[idx.min(sorted.len() - 1)]
}

pub(super) fn write_results(results: &[ScenarioResult]) {
    let mut table = String::new();
    let _ = writeln!(table, "{BENCHMARK_RESULTS_BANNER}");
    let _ = writeln!(
        table,
        "{BENCHMARK_RESULTS_TABLE_HEADER_SCENARIO:<18}| {BENCHMARK_RESULTS_TABLE_HEADER_FRAMES:>6} | {BENCHMARK_RESULTS_TABLE_HEADER_AVERAGE:>11} | {BENCHMARK_RESULTS_TABLE_HEADER_MEDIAN:>11} | {BENCHMARK_RESULTS_TABLE_HEADER_PERCENTILE_95:>11} | {BENCHMARK_RESULTS_TABLE_HEADER_PERCENTILE_99:>11} | {BENCHMARK_RESULTS_TABLE_HEADER_MIN:>11} | {BENCHMARK_RESULTS_TABLE_HEADER_MAX:>11} | {BENCHMARK_RESULTS_TABLE_HEADER_FPS:>6}"
    );
    let _ = writeln!(
        table,
        "{:-<18}|{:->8}|{:->13}|{:->13}|{:->13}|{:->13}|{:->13}|{:->13}|{:->8}",
        "", "", "", "", "", "", "", "", ""
    );

    for result in results {
        let _ = writeln!(
            table,
            "{name:<18}| {frames:>6} | {average:>11.2} | {median:>11.2} | {percentile_95:>11.2} | {percentile_99:>11.2} | {min:>11.2} | {max:>11.2} | {average_frames_per_second:>6.0}",
            name = result.name,
            frames = result.frames,
            average = result.average,
            median = result.median,
            percentile_95 = result.percentile_95,
            percentile_99 = result.percentile_99,
            min = result.min,
            max = result.max,
            average_frames_per_second = result.average_frames_per_second()
        );
    }

    info!("{table}");

    match write_csv(results) {
        Ok(path) => info!("CSV written to: {path}"),
        Err(error) => warn!("Failed to write CSV: {error}"),
    }
}

fn format_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let output = Command::new(DATE_COMMAND)
        .args([
            DATE_COMMAND_ARG_REFERENCE_TIME,
            &now.to_string(),
            DATE_COMMAND_ARG_FORMAT,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => format!("{now}"),
    }
}

fn write_csv(results: &[ScenarioResult]) -> Result<String, Error> {
    let results_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(RESULTS_DIRECTORY_NAME);
    create_dir_all(&results_dir)?;

    let timestamp = format_timestamp();
    let path = results_dir.join(format!(
        "{BENCHMARK_RESULTS_FILE_PREFIX}{timestamp}{BENCHMARK_RESULTS_FILE_SUFFIX}"
    ));
    let mut file = File::create(&path)?;
    writeln!(file, "{BENCHMARK_CSV_HEADER}")?;
    for result in results {
        writeln!(
            file,
            "{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.0}",
            result.name,
            result.frames,
            result.average,
            result.median,
            result.percentile_95,
            result.percentile_99,
            result.min,
            result.max,
            result.average_frames_per_second()
        )?;
    }
    Ok(path.display().to_string())
}
