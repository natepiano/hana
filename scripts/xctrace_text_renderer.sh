#!/usr/bin/env bash
set -euo pipefail

DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR

PACKAGE="bevy_diegetic"
EXAMPLE="text_renderer_gpu_bench"
BINARY="target/release/examples/${EXAMPLE}"
TRACE_DIR="target/xctrace"
GPU_XPATH='/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]'
EXAMPLE_PROCESS_PATTERN='target/(debug|release)/examples/'

usage() {
    cat <<'USAGE'
Usage:
  scripts/xctrace_text_renderer.sh build
  scripts/xctrace_text_renderer.sh record <empty|slug|sdf|msdf|mtsdf> [time-limit] [instances]
  scripts/xctrace_text_renderer.sh export <empty|slug|sdf|msdf|mtsdf>
  scripts/xctrace_text_renderer.sh record-all [time-limit] [instances]
  scripts/xctrace_text_renderer.sh export-all

Outputs:
  target/xctrace/text-renderer-<mode>.trace
  target/xctrace/text-renderer-<mode>-gpu-intervals.xml
USAGE
}

require_mode() {
    case "${1:-}" in
        empty | slug | sdf | msdf | mtsdf) ;;
        *)
            usage
            exit 2
            ;;
    esac
}

ensure_dirs() {
    mkdir -p "${TRACE_DIR}"
}

ensure_no_other_examples() {
    local mode="$1"
    local matches
    matches="$(pgrep -fl "${EXAMPLE_PROCESS_PATTERN}" || true)"
    if [[ -z "${matches}" ]]; then
        return
    fi

    printf 'Refusing to benchmark while another Bevy example process is running:\n%s\n' "${matches}" >&2
    printf 'Close the app or shut it down through BRP before rerunning.\n' >&2
    printf 'Requested benchmark mode: %s\n' "${mode}" >&2
    exit 3
}

build_example() {
    /bin/zsh -lc 'exec "$@"' zsh cargo build -p "${PACKAGE}" --release --example "${EXAMPLE}"
}

trace_path() {
    printf '%s/text-renderer-%s.trace\n' "${TRACE_DIR}" "$1"
}

xml_path() {
    printf '%s/text-renderer-%s-gpu-intervals.xml\n' "${TRACE_DIR}" "$1"
}

record_mode() {
    local mode="$1"
    local time_limit="${2:-15s}"
    local instances="${3:-720}"
    require_mode "${mode}"
    ensure_dirs
    build_example
    ensure_no_other_examples "${mode}"

    local trace
    trace="$(trace_path "${mode}")"
    if [[ -e "${trace}" ]]; then
        rm -rf "${trace}"
    fi

    "${BINARY}" \
        --mode "${mode}" \
        --instances "${instances}" \
        --warmup-frames "${WARMUP_FRAMES:-180}" \
        --sample-frames "${SAMPLE_FRAMES:-240}" \
        > "${TRACE_DIR}/text-renderer-${mode}.stdout.log" 2>&1 &
    local bench_pid="$!"

    sleep 1
    set +e
    xcrun xctrace record \
        --template 'Metal System Trace' \
        --time-limit "${time_limit}" \
        --output "${trace}" \
        --no-prompt \
        --attach "${bench_pid}"
    local status="$?"
    wait "${bench_pid}"
    local bench_status="$?"
    set -e
    if [[ "${status}" != "0" && "${status}" != "54" ]]; then
        exit "${status}"
    fi
    if [[ "${bench_status}" != "0" ]]; then
        exit "${bench_status}"
    fi
}

export_mode() {
    local mode="$1"
    require_mode "${mode}"
    ensure_dirs
    xcrun xctrace export \
        --input "$(trace_path "${mode}")" \
        --xpath "${GPU_XPATH}" \
        --output "$(xml_path "${mode}")"
}

record_all() {
    local time_limit="${1:-15s}"
    local instances="${2:-720}"
    for mode in empty slug sdf msdf mtsdf; do
        record_mode "${mode}" "${time_limit}" "${instances}"
    done
}

export_all() {
    for mode in empty slug sdf msdf mtsdf; do
        export_mode "${mode}"
    done
}

main() {
    case "${1:-}" in
        build)
            build_example
            ;;
        record)
            record_mode "${2:-}" "${3:-15s}" "${4:-720}"
            ;;
        export)
            export_mode "${2:-}"
            ;;
        record-all)
            record_all "${2:-15s}" "${3:-720}"
            ;;
        export-all)
            export_all
            ;;
        *)
            usage
            exit 2
            ;;
    esac
}

main "$@"
