#!/usr/bin/env python3
"""
Parse Metal GPU interval XML exported by xctrace and compute the mean
Vertex / Fragment / paired-by-frame durations for the Slug transparent
pass.

Usage:
    scripts/parse_gpu_intervals.py target/xctrace/text-renderer-slug-gpu-intervals.xml

Output (one key=value per line for easy grep):
    process_filter=text_renderer_gpu_bench
    label_filter=main_transparent_pass_3d:main_transparent_pass_3d
    vertex_samples=NNN                    # interval count
    fragment_samples=NNN                  # interval count
    frames_paired=NNN                     # frames with both V and F intervals
    vertex_per_interval_mean_ms=N.NNNN    # mean over vertex intervals
    fragment_per_interval_mean_ms=N.NNNN  # mean over fragment intervals
    vertex_per_frame_mean_ms=N.NNNN       # sum of vertex per frame, averaged across frames
    fragment_per_frame_mean_ms=N.NNNN     # sum of fragment per frame, averaged across frames
    vertex_plus_fragment_per_frame_mean_ms=N.NNNN

A frame can hold more than one fragment interval (e.g. the OIT
sub-pass and the main transparent pass each produce one). For
benchmark comparisons, the per_frame_mean values are the meaningful
unit — they sum vertex_per_frame + fragment_per_frame == V+F per
frame exactly. The per_interval columns can disagree because they
average a different denominator.

xctrace exports `metal-gpu-intervals` rows where every column is one
child element of the row, in fixed column order:

    1: <start-time>            (start time, ns)
    2: <duration>              (interval duration, ns)
    3: <gpu-channel-name>      ("Vertex" | "Fragment" | other)
    4: <gpu-frame-number>      (text = "17"; fmt = "Frame 17")
    5: <duration>              (start-latency — IGNORED; second `duration` element)
    6: <metal-nesting-level>
    7: <formatted-label>       (fmt attribute = "main_transparent_pass_3d:... ( text_renderer_gpu_bench (PID) ) 0x...")
    ...

The trick: xctrace de-duplicates repeated values with id/ref. The first
occurrence of a value is `<tag id="N" fmt="...">text</tag>`; later rows
reuse it as `<tag ref="N"/>` (empty element). The parser builds a
one-pass `id -> (text, fmt)` map, then on each row resolves every
child via that map before deciding whether to keep the row.

Required for a row to be counted:
  - resolved `<formatted-label>` `fmt` contains both PROCESS_FILTER and
    LABEL_FILTER
  - resolved `<gpu-channel-name>` text is exactly "Vertex" or "Fragment"
  - FIRST `<duration>` child of the row holds the interval duration in
    nanoseconds (later `<duration>` children are different columns
    like start-latency)
  - `<gpu-frame-number>` text is an integer (frame index) used for
    per-frame pairing of vertex + fragment durations

The parser exits non-zero with a clear error if the XML does not match.
Stdout is structured for easy ingestion in a follow-up table update.
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path

PROCESS_FILTER = "text_renderer_gpu_bench"
LABEL_FILTER = "main_transparent_pass_3d:main_transparent_pass_3d"


@dataclass
class Resolved:
    tag: str
    text: str
    fmt: str


@dataclass
class Interval:
    channel: str
    duration_ns: float
    frame: int | None


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1] if "}" in tag else tag


def build_id_table(root: ET.Element) -> dict[str, Resolved]:
    table: dict[str, Resolved] = {}
    for el in root.iter():
        el_id = el.attrib.get("id")
        if el_id is None:
            continue
        table[el_id] = Resolved(
            tag=local_name(el.tag),
            text=(el.text or "").strip(),
            fmt=el.attrib.get("fmt", ""),
        )
    return table


def resolve(el: ET.Element, id_table: dict[str, Resolved]) -> Resolved:
    ref = el.attrib.get("ref")
    if ref is not None and ref in id_table:
        return id_table[ref]
    return Resolved(
        tag=local_name(el.tag),
        text=(el.text or "").strip(),
        fmt=el.attrib.get("fmt", ""),
    )


def collect_intervals(xml_path: Path) -> list[Interval]:
    tree = ET.parse(xml_path)
    root = tree.getroot()
    id_table = build_id_table(root)

    rows = [el for el in root.iter() if local_name(el.tag) == "row"]
    intervals: list[Interval] = []
    for row in rows:
        formatted_label_fmt = ""
        channel = ""
        first_duration_text = ""
        frame_text = ""

        for child in row:
            res = resolve(child, id_table)
            tag = res.tag
            if tag == "formatted-label" and not formatted_label_fmt:
                formatted_label_fmt = res.fmt
            elif tag == "gpu-channel-name" and not channel:
                channel = res.text
            elif tag == "duration" and not first_duration_text:
                first_duration_text = res.text
            elif tag == "gpu-frame-number" and not frame_text:
                frame_text = res.text

        if PROCESS_FILTER not in formatted_label_fmt:
            continue
        if LABEL_FILTER not in formatted_label_fmt:
            continue
        if channel not in ("Vertex", "Fragment"):
            continue
        if not first_duration_text:
            continue
        try:
            duration_ns = float(first_duration_text)
        except ValueError:
            continue
        frame = int(frame_text) if frame_text and frame_text.lstrip("-").isdigit() else None
        intervals.append(Interval(channel=channel, duration_ns=duration_ns, frame=frame))
    return intervals


def mean_ms(values: list[float]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values) / 1_000_000.0


@dataclass
class PerFrameTotals:
    vertex_ns: list[float]
    fragment_ns: list[float]
    total_ns: list[float]


def per_frame_totals(intervals: list[Interval]) -> PerFrameTotals:
    if any(iv.frame is None for iv in intervals):
        vertex_list = [iv.duration_ns for iv in intervals if iv.channel == "Vertex"]
        fragment_list = [iv.duration_ns for iv in intervals if iv.channel == "Fragment"]
        paired = [(v, f) for v, f in zip(vertex_list, fragment_list)]
        return PerFrameTotals(
            vertex_ns=[v for v, _ in paired],
            fragment_ns=[f for _, f in paired],
            total_ns=[v + f for v, f in paired],
        )
    by_frame: dict[int, dict[str, float]] = {}
    for iv in intervals:
        assert iv.frame is not None
        bucket = by_frame.setdefault(iv.frame, {"Vertex": 0.0, "Fragment": 0.0})
        bucket[iv.channel] += iv.duration_ns
    paired_buckets = [
        bucket
        for bucket in by_frame.values()
        if bucket["Vertex"] > 0.0 and bucket["Fragment"] > 0.0
    ]
    return PerFrameTotals(
        vertex_ns=[b["Vertex"] for b in paired_buckets],
        fragment_ns=[b["Fragment"] for b in paired_buckets],
        total_ns=[b["Vertex"] + b["Fragment"] for b in paired_buckets],
    )


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: parse_gpu_intervals.py <gpu-intervals.xml>", file=sys.stderr)
        return 2
    xml_path = Path(sys.argv[1])
    if not xml_path.exists():
        print(f"file not found: {xml_path}", file=sys.stderr)
        return 2
    intervals = collect_intervals(xml_path)
    if not intervals:
        print(
            "no matching intervals found; verify process / label filters or the XML element names",
            file=sys.stderr,
        )
        return 3
    vertex_durations = [iv.duration_ns for iv in intervals if iv.channel == "Vertex"]
    fragment_durations = [iv.duration_ns for iv in intervals if iv.channel == "Fragment"]
    per_frame = per_frame_totals(intervals)
    print(f"process_filter={PROCESS_FILTER}")
    print(f"label_filter={LABEL_FILTER}")
    print(f"vertex_samples={len(vertex_durations)}")
    print(f"fragment_samples={len(fragment_durations)}")
    print(f"frames_paired={len(per_frame.total_ns)}")
    print(f"vertex_per_interval_mean_ms={mean_ms(vertex_durations):.4f}")
    print(f"fragment_per_interval_mean_ms={mean_ms(fragment_durations):.4f}")
    print(f"vertex_per_frame_mean_ms={mean_ms(per_frame.vertex_ns):.4f}")
    print(f"fragment_per_frame_mean_ms={mean_ms(per_frame.fragment_ns):.4f}")
    print(f"vertex_plus_fragment_per_frame_mean_ms={mean_ms(per_frame.total_ns):.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
