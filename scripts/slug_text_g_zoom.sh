#!/usr/bin/env bash
set -euo pipefail

PORT="${BRP_PORT:-15702}"
RESTART="false"
LOG_DIR="${TMPDIR:-/tmp}"
LOG_PATH="${LOG_DIR%/}/slug_text_g_zoom.log"
SCREENSHOT_PATH="/tmp/slug_g_inside_curve_script_restored.png"
TAKE_SCREENSHOT="true"
VIEW="g"
SHUTDOWN_ONLY="false"

usage() {
    cat <<'USAGE'
Usage:
  scripts/slug_text_g_zoom.sh [--restart] [--port 15702] [--view home|g]
  scripts/slug_text_g_zoom.sh [--screenshot /tmp/view.png]
  scripts/slug_text_g_zoom.sh [--no-screenshot]
  scripts/slug_text_g_zoom.sh [--shutdown-only]

Launches or reuses examples/slug_text.rs, waits for BRP, and moves the
OrbitCam to the saved lowercase-g inside-curve zoom view unless --view home
is selected.
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --restart)
            RESTART="true"
            shift
            ;;
        --port)
            PORT="${2:-}"
            if [[ -z "${PORT}" ]]; then
                usage
                exit 2
            fi
            shift 2
            ;;
        --screenshot)
            SCREENSHOT_PATH="${2:-}"
            if [[ -z "${SCREENSHOT_PATH}" ]]; then
                usage
                exit 2
            fi
            TAKE_SCREENSHOT="true"
            shift 2
            ;;
        --view)
            VIEW="${2:-}"
            case "${VIEW}" in
                home | g) ;;
                *)
                    usage
                    exit 2
                    ;;
            esac
            shift 2
            ;;
        --no-screenshot)
            TAKE_SCREENSHOT="false"
            shift
            ;;
        --shutdown-only)
            SHUTDOWN_ONLY="true"
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

json_rpc() {
    local payload="$1"
    curl --fail --silent --show-error \
        --header 'Content-Type: application/json' \
        --data "${payload}" \
        "http://127.0.0.1:${PORT}/jsonrpc"
}

brp_ready() {
    json_rpc '{"jsonrpc":"2.0","id":1,"method":"rpc.discover","params":{}}' >/dev/null 2>&1
}

shutdown_if_running() {
    if ! brp_ready; then
        return
    fi

    json_rpc '{"jsonrpc":"2.0","id":2,"method":"brp_extras/shutdown","params":{}}' >/dev/null || true
    for _ in {1..100}; do
        if ! brp_ready; then
            return
        fi
        sleep 0.05
    done
}

launch_if_needed() {
    if brp_ready; then
        return
    fi

    BRP_EXTRAS_PORT="${PORT}" nohup cargo run -p bevy_diegetic --example slug_text \
        > "${LOG_PATH}" 2>&1 &

    for _ in {1..300}; do
        if brp_ready; then
            return
        fi
        sleep 0.1
    done

    printf 'Timed out waiting for slug_text BRP on port %s.\n' "${PORT}" >&2
    printf 'Launch log: %s\n' "${LOG_PATH}" >&2
    exit 1
}

orbit_entity() {
    json_rpc '{
        "jsonrpc": "2.0",
        "id": 3,
        "method": "world.query",
        "params": {
            "filter": { "with": ["bevy_lagrange::orbit_cam::OrbitCam"] },
            "data": {
                "components": [
                    "bevy_lagrange::orbit_cam::OrbitCam",
                    "bevy_transform::components::transform::Transform"
                ]
            }
        }
    }' | python3 -c 'import json,sys
data=json.load(sys.stdin)
rows=data.get("result") or []
if not rows:
    raise SystemExit("No OrbitCam entity found")
print(rows[0]["entity"])'
}

mutate_orbit_cam() {
    local entity="$1"
    json_rpc "{
        \"jsonrpc\": \"2.0\",
        \"id\": 4,
        \"method\": \"world.mutate_components\",
        \"params\": {
            \"entity\": ${entity},
            \"component\": \"bevy_lagrange::orbit_cam::OrbitCam\",
            \"path\": \"\",
            \"value\": {
                \"focus\": [-0.07180324, 0.38033515, 2.0064423],
                \"radius\": 0.08216206,
                \"yaw\": 0.0,
                \"pitch\": 0.055,
                \"target_focus\": [-0.07180324, 0.38033515, 2.0064423],
                \"target_yaw\": 0.0,
                \"target_pitch\": 0.055,
                \"target_radius\": 0.08216206,
                \"yaw_upper_limit\": null,
                \"yaw_lower_limit\": null,
                \"pitch_upper_limit\": null,
                \"pitch_lower_limit\": null,
                \"focus_bounds_origin\": [0.0, 0.0, 0.0],
                \"focus_bounds_shape\": null,
                \"zoom_upper_limit\": null,
                \"zoom_lower_limit\": 0.00000010000000116860974,
                \"orbit_sensitivity\": 1.0,
                \"orbit_smoothness\": 0.10000000149011612,
                \"pan_sensitivity\": 1.0,
                \"pan_smoothness\": 0.019999999552965164,
                \"zoom_sensitivity\": 1.0,
                \"zoom_smoothness\": 0.10000000149011612,
                \"upside_down_policy\": \"Prevent\",
                \"initialization\": \"Complete\",
                \"update_request\": \"None\",
                \"axis\": [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                \"time_source\": \"Virtual\"
            }
        }
    }" >/dev/null
}

mutate_transform() {
    local entity="$1"
    json_rpc "{
        \"jsonrpc\": \"2.0\",
        \"id\": 5,
        \"method\": \"world.mutate_components\",
        \"params\": {
            \"entity\": ${entity},
            \"component\": \"bevy_transform::components::transform::Transform\",
            \"path\": \"\",
            \"value\": {
                \"translation\": [-0.07180324, 0.38485178, 2.0884802],
                \"rotation\": [-0.02749653, 0.0, 0.0, 0.99962193],
                \"scale\": [1.0, 1.0, 1.0]
            }
        }
    }" >/dev/null
}

capture_screenshot() {
    local path="$1"
    local last_size=0
    local stable_count=0

    rm -f "${path}"
    json_rpc "{
        \"jsonrpc\": \"2.0\",
        \"id\": 6,
        \"method\": \"brp_extras/screenshot\",
        \"params\": {
            \"path\": \"${path}\"
        }
    }" >/dev/null

    for _ in {1..600}; do
        if [[ -s "${path}" ]]; then
            local size
            size="$(stat -f '%z' "${path}")"
            if [[ "${size}" == "${last_size}" ]]; then
                stable_count=$((stable_count + 1))
                if [[ "${stable_count}" -ge 4 ]]; then
                    return
                fi
            else
                last_size="${size}"
                stable_count=0
            fi
        fi
        sleep 0.05
    done

    printf 'Timed out waiting for screenshot: %s\n' "${path}" >&2
    exit 1
}

main() {
    if [[ "${SHUTDOWN_ONLY}" == "true" ]]; then
        shutdown_if_running
        printf 'slug_text shutdown requested on port %s\n' "${PORT}"
        return
    fi

    if [[ "${RESTART}" == "true" ]]; then
        shutdown_if_running
    fi

    launch_if_needed
    local entity="default-camera"
    if [[ "${VIEW}" == "g" ]]; then
        entity="$(orbit_entity)"
        mutate_orbit_cam "${entity}"
        mutate_transform "${entity}"
    fi
    if [[ "${TAKE_SCREENSHOT}" == "true" ]]; then
        capture_screenshot "${SCREENSHOT_PATH}"
    fi
    printf 'slug_text %s view ready on port %s using %s\n' "${VIEW}" "${PORT}" "${entity}"
    printf 'launch log: %s\n' "${LOG_PATH}"
    if [[ "${TAKE_SCREENSHOT}" == "true" ]]; then
        printf 'screenshot: %s\n' "${SCREENSHOT_PATH}"
    fi
}

main
