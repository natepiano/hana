# Zoom direction affordance + naming

How the camera-control panel highlights the zoom row actually engaged (pressing
RT lights `rt`, not `lt`), and the two orthogonal zoom types behind it. Spans
`bevy_lagrange` (row data + live direction) and the `fairy_dust` camera control
panel (display + highlight).

Every zoom affordance shows as two rows (in / out) across all input methods, and
only the row matching the live zoom direction highlights.

## Type model

Two orthogonal types, correctly named:

- `ZoomInversion { Normal, Inverted }` — global invert-zoom **config** toggle
  (relative). In `input/bindings/builder.rs`; field / builder method / accessor
  are all `zoom_inversion` (`OrbitCamBindings::zoom_inversion()`, descriptor
  field `zoom_inversion`).
- `ZoomDirection { In, Out }` — actual absolute direction (display/derived). In
  `input/control_summary.rs`. Two variants, no neutral.

Both are re-exported from `lib.rs` (`pub use input::ZoomDirection` /
`ZoomInversion`).

### Row tag

- `OrbitCamControlRow.zoom_direction: Option<ZoomDirection>` —
  `Some(In)`/`Some(Out)` for every zoom row; `None` only for orbit/pan rows
  (genuinely not zoom). The `+/-` keyboard case splits into two rows, so there is
  no neutral variant. Set via `OrbitCamControlRow::with_zoom_direction`.

### Live direction

`OrbitCamInteractionState::zoom_direction(): Option<ZoomDirection>` holds the
runtime direction. `input/lifecycle.rs::reported_zoom_direction` computes it from
the sign of `OrbitCamInput` zoom delta
(`input.zoom_coarse().amount() + input.zoom_smooth().amount()`): positive = `In`,
negative = `Out`. It holds the previous direction on a zero-delta frame (persists
through the reporting-debounce window) and clears to `None` when no zoom is
reported. Row sign and live sign both derive from the same binding scale, so they
match regardless of any inversion setting; reading the live sign means a reversal
(in → out) updates at once without waiting on a settle.

## Row split — every zoom source becomes two rows

Labels (In = zoom in):

| Source                   | In         | Out       |
| ------------------------ | ---------- | --------- |
| Gamepad                  | `rt`       | `lt`      |
| Gamepad slow             | `rb+rt`    | `lb+lt`   |
| Keyboard                 | `+`        | `-`       |
| Mouse wheel              | `wheel ↑`  | `wheel ↓` |
| Pinch                    | `pinch out`| `pinch in`|
| Smooth-scroll (trackpad) | `scroll ↑` | `scroll ↓`|

Generated in `input/control_summary.rs`:

- Gamepad triggers are separate unidirectional bindings → one row each; tagged
  from the sign of the motion scale (`RightTrigger2` `+` = In, `LeftTrigger2` `-`
  = Out). Slow variants are the gated `rb+`/`lb+` rows. Handled by
  `describe_zoom_held_entry` (single trigger tagged by scale sign; keyboard `+`/`-`
  split per entry).
- Keyboard `bidirectional_keys(Equal, Minus)` is one binding → two rows: `+` (In)
  and `-` (Out), split by entry sign in `describe_zoom_held_entry`.
- Wheel / pinch / smooth-scroll are single bidirectional sources → `push_zoom_pair`
  / `push_trackpad_zoom_pair` synthesize two rows each with the locked labels.
  `zoom_inversion_sign` (via `zoom_direction_from_sign`) flips only these
  synthesized tags; native triggers/keys derive their own sign and are not
  inverted at runtime.

Button-drag zoom and two-finger touch zoom (custom configs only, not in any
built-in preset) stay single rows.

## Wheel/pinch bindings

`OrbitCamMouseWheelZoom` and `OrbitCamPinchZoom` are both marker structs
(`input/bindings/builder.rs`). Inversion is governed solely by `ZoomInversion`;
`zoom_signed` in `input/adapter/inject.rs` applies the inversion sign
(`ZoomInversion::Normal` → `1.0`, `Inverted` → `-1.0`) and takes no per-binding
polarity param.

## Slow-mode scaling

Slow-mode zoom scaling applies once in `input/adapter/resolve.rs`. `AdapterScale`
is built per camera from the slow-mode-active flag
(`AdapterScale::from_bindings(.., is_slow_mode_active(..))`) and applied to each
accumulated value (`adapter_scale.f32(...)`) for zoom (and orbit/pan). The active
speed (`ControlSpeed::Normal`/`Slow`) is derived from whether the slow action
carries any value (`f32_speed` / `vec2_speed`) and reported separately.

## Highlight mechanism (fairy_dust panel)

`crates/fairy_dust/src/camera_control_panel/`:

- `guidance.rs` — `CameraGuidanceRow` carries the `Option<ZoomDirection>` tag
  (propagated in `From<OrbitCamControlRow>`).
- `snapshot.rs` — `row_active` factors in direction.
- `layout.rs` — `build_guidance_group` matches each row's direction against the
  live direction (`display.zoom_direction()`).
- `display.rs` — `CameraGuidanceDisplay` / `CameraGuidanceDisplayState` carry the
  live `zoom_direction` alongside sources, held through the existing
  `SOURCE_HOLD_SECONDS` window.
- `mod.rs` — `track_live_zoom_direction` runs each frame, mirroring the bound
  camera's `OrbitCamInteractionState::zoom_direction()` onto the display state.
  The `refresh_on_*` observers handle sources (`OrbitCamInteractionStarted` /
  `OrbitCamInteractionSourcesChanged` / `Ended`) and speed
  (`OrbitCamInteractionSpeedChanged`).

Match rule: a zoom row lights when its source is active for that (kind, speed)
group AND its `ZoomDirection` equals the live zoom direction (unknown live
direction → light both). Orbit/pan rows (`None`) match by source only.

## Adjacent panel behavior

- Panel `→` action arrows share a left-aligned action column (`layout.rs`).
- `lock_camera_preset()` (`fairy_dust` builder, `builder/sprinkle.rs`) sets
  `CameraPresetSwitching::Disabled` to pin the camera to one preset; the
  keyboard-shortcut overlay (`screen_panels/help_overlay.rs`) drops its
  preset-cycle entry to match.
