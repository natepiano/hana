//! Generate macOS `say` speech fixtures for VAD/STT testing.
//!
//! Usage:
//! `cargo run -p hana_voice_sidecar --example voice_say_fixtures -- [--out-dir DIR] [--voice VOICE]
//! [phrase ...]`

use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

const DEFAULT_OUT_DIR: &str = "/tmp/hana_voice_sidecar_speech_fixtures";
const DEFAULT_PHRASES: &[&str] = &[
    "test",
    "testing",
    "reset",
    "okay",
    "rest",
    "make me neon",
    "make it glow",
];

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::from_env()?;
    fs::create_dir_all(&options.out_dir)?;

    for phrase in &options.phrases {
        let wav_path = output_path(&options.out_dir, phrase);
        generate_fixture(&options, phrase, &wav_path)?;
        println!("{} | {}", wav_path.display(), phrase);
    }

    Ok(())
}

#[derive(Debug)]
struct Options {
    out_dir: PathBuf,
    voice:   Option<String>,
    phrases: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlugBoundary {
    Separator,
    Word,
}

impl Options {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut out_dir = PathBuf::from(DEFAULT_OUT_DIR);
        let mut voice = None;
        let mut phrases = Vec::new();
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out-dir" => {
                    let Some(value) = args.next() else {
                        return Err("--out-dir requires a value".into());
                    };
                    out_dir = PathBuf::from(value);
                },
                "--voice" => {
                    let Some(value) = args.next() else {
                        return Err("--voice requires a value".into());
                    };
                    voice = Some(value);
                },
                _ => phrases.push(arg),
            }
        }
        if phrases.is_empty() {
            phrases.extend(DEFAULT_PHRASES.iter().map(ToString::to_string));
        }
        Ok(Self {
            out_dir,
            voice,
            phrases,
        })
    }
}

fn generate_fixture(
    options: &Options,
    phrase: &str,
    wav_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let aiff_path = wav_path.with_extension("aiff");
    let mut say = Command::new("say");
    if let Some(voice) = &options.voice {
        say.args(["-v", voice]);
    }
    let status = say.arg("-o").arg(&aiff_path).arg(phrase).status()?;
    if !status.success() {
        return Err(format!("say failed for phrase {phrase:?}").into());
    }

    let status = Command::new("afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@48000", "-c", "1"])
        .arg(&aiff_path)
        .arg(wav_path)
        .status()?;
    if !status.success() {
        return Err(format!("afconvert failed for {}", aiff_path.display()).into());
    }

    fs::remove_file(aiff_path)?;
    Ok(())
}

fn output_path(out_dir: &Path, phrase: &str) -> PathBuf {
    out_dir.join(format!("{}.wav", slugify(phrase)))
}

fn slugify(phrase: &str) -> String {
    let mut slug = String::new();
    let mut boundary = SlugBoundary::Separator;
    for character in phrase.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            boundary = SlugBoundary::Word;
        } else if boundary == SlugBoundary::Word {
            slug.push('_');
            boundary = SlugBoundary::Separator;
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    if slug.is_empty() {
        String::from("speech")
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugifies_phrase_for_file_name() {
        assert_eq!(slugify("Make me neon"), "make_me_neon");
        assert_eq!(slugify("OK!"), "ok");
    }
}
