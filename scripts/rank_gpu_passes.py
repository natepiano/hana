#!/usr/bin/env python3
"""
Rank GPU time per render-pass label from an xctrace metal-gpu-intervals XML.

Usage:
    rank_gpu_passes.py <gpu-intervals.xml> <process-name-filter>

Unlike parse_gpu_intervals.py (which filters to one pass label), this
aggregates EVERY pass label: per label and channel it sums interval
durations, then divides by the number of distinct GPU frames to get a
per-frame cost, and prints a table ranked by per-frame total.

Row layout and id/ref resolution follow parse_gpu_intervals.py: the
first occurrence of a value is `<tag id="N" fmt="...">text</tag>`;
later rows reuse it as `<tag ref="N"/>`.
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Resolved:
    tag: str
    text: str
    fmt: str


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


@dataclass
class LabelStats:
    total_ns_by_channel: dict[str, float]
    interval_count: int


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: rank_gpu_passes.py <gpu-intervals.xml> <process-filter>", file=sys.stderr)
        return 2
    xml_path = Path(sys.argv[1])
    process_filter = sys.argv[2]
    if not xml_path.exists():
        print(f"file not found: {xml_path}", file=sys.stderr)
        return 2

    tree = ET.parse(xml_path)
    root = tree.getroot()
    id_table = build_id_table(root)

    stats: dict[str, LabelStats] = {}
    frames: set[int] = set()
    skipped_process = 0
    rows = 0

    for row in (el for el in root.iter() if local_name(el.tag) == "row"):
        rows += 1
        label_fmt = ""
        channel = ""
        first_duration_text = ""
        frame_text = ""
        for child in row:
            res = resolve(child, id_table)
            tag = res.tag
            if tag == "formatted-label" and not label_fmt:
                label_fmt = res.fmt
            elif tag == "gpu-channel-name" and not channel:
                channel = res.text
            elif tag == "duration" and not first_duration_text:
                first_duration_text = res.text
            elif tag == "gpu-frame-number" and not frame_text:
                frame_text = res.text

        if process_filter not in label_fmt:
            skipped_process += 1
            continue
        if not first_duration_text:
            continue
        try:
            duration_ns = float(first_duration_text)
        except ValueError:
            continue
        if frame_text and frame_text.lstrip("-").isdigit():
            frames.add(int(frame_text))

        label = label_fmt.split(" ( ")[0].strip()
        entry = stats.setdefault(label, LabelStats(total_ns_by_channel={}, interval_count=0))
        entry.total_ns_by_channel[channel] = entry.total_ns_by_channel.get(channel, 0.0) + duration_ns
        entry.interval_count += 1

    if not stats:
        message = (
            f"no matching rows (rows={rows}, skipped_process={skipped_process});"
            f" verify the process filter or the XML element names"
        )
        print(message, file=sys.stderr)
        return 3

    n_frames = max(len(frames), 1)
    print(f"rows={rows} matched_labels={len(stats)} frames={n_frames} skipped_other_process={skipped_process}")
    print()
    header = f"{'per-frame ms':>12}  {'share':>6}  {'intervals':>9}  {'channels (ms/frame)':<40}  label"
    print(header)
    print("-" * len(header))

    ranked = sorted(
        stats.items(),
        key=lambda kv: sum(kv[1].total_ns_by_channel.values()),
        reverse=True,
    )
    grand_total_ns = sum(sum(s.total_ns_by_channel.values()) for _, s in ranked)
    for label, entry in ranked:
        total_ns = sum(entry.total_ns_by_channel.values())
        per_frame_ms = total_ns / n_frames / 1_000_000.0
        share = total_ns / grand_total_ns * 100.0
        channels = ", ".join(
            f"{ch}={ns / n_frames / 1_000_000.0:.3f}"
            for ch, ns in sorted(entry.total_ns_by_channel.items())
        )
        print(f"{per_frame_ms:>12.3f}  {share:>5.1f}%  {entry.interval_count:>9}  {channels:<40}  {label}")

    print()
    print(f"grand_total_per_frame_ms={grand_total_ns / n_frames / 1_000_000.0:.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
