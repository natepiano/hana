# Zoom direction affordance + naming cleanup

Working spec for the camera-control-panel zoom feature and the surrounding
zoom-type naming cleanup. Spans `bevy_lagrange` (row data) and the `fairy_dust`
camera control panel (display + highlight).

## Goal

The panel must highlight only the zoom row actually engaged. Pressing RT lights
`rt`, not `lt`. Generalized by decision: **every zoom affordance shows as two
rows (in / out) across all input methods**, and only the row matching the live
zoom direction highlights.

## Type model

- `ZoomDirection { In, Out }` — new, in `control_summary.rs`. The absolute
  direction a zoom row drives. Two variants, no neutral.
- `OrbitCamControlRow.zoom_direction: Option<ZoomDirection>` —
  `Some(In)`/`Some(Out)` for every zoom row; `None` **only** for orbit/pan rows
  (genuinely not zoom). `None` is never the `=/-` case — that splits into two
  rows now — so there is no `Both`/neutral variant.
- Live direction at runtime = sign of the camera's zoom delta from
  `OrbitCamInput` (`SmoothZoomDelta` + `CoarseZoomDelta`): positive = `In`,
  negative = `Out`. Row sign and live sign both derive from the same binding
  scale, so they match regardless of any inversion setting.

## Naming end-state (three zoom-direction-ish types → two, correctly named)

- `ZoomInversion { Normal, Inverted }` — global invert-zoom **config** toggle
  (relative). Was `ZoomDirection`; the rename is done, including the field /
  builder method / accessor `zoom_direction` → `zoom_inversion`. Its variant doc
  comments may still say "zooms in the default/opposite direction" — reword to
  describe inversion.
- `WheelZoomPolarity { Normal, Inverted }` — **DELETE.** Dead (never set to
  `Inverted` anywhere; the `.polarity` field on `OrbitCamMouseWheelZoom` is never
  read) and redundant with `ZoomInversion`. Deleting makes `OrbitCamMouseWheelZoom`
  a marker struct (like `OrbitCamPinchZoom`) and drops the `polarity` param from
  `zoom_signed` in `adapter/inject.rs`. Breaking public API (pre-1.0, accepted).
  Do this AFTER the rename — it edits `inject.rs`, which the rename also touched.
- `ZoomDirection { In, Out }` — the freed name, now means actual direction
  (display/derived). One inversion type + one absolute-direction type, orthogonal.

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

- Gamepad triggers are already separate unidirectional bindings → one row each;
  tag from the sign of the motion scale (`RightTrigger2` `+` = In,
  `LeftTrigger2` `-` = Out). Slow variants are the gated `rb+`/`lb+` rows.
- Keyboard `bidirectional_keys(Equal, Minus)` is one binding → emit two rows:
  `+` (In) and `-` (Out). Split the bidirectional held binding by entry sign.
- Wheel / pinch / smooth-scroll are single bidirectional sources → synthesize
  two rows each with the labels above.
- **Deferred** (not in any built-in preset, custom configs only): button-drag
  zoom and two-finger touch zoom stay single rows for now. Touch would mirror
  pinch (`pinch out` / `pinch in`) if/when split.

## Highlight mechanism (fairy_dust panel)

- Row generation: `crates/bevy_lagrange/src/input/control_summary.rs`.
- Panel: `crates/fairy_dust/src/camera_control_panel/`
  - `guidance.rs` — `CameraGuidanceRow` carries the `Option<ZoomDirection>` tag
    (propagate in `From<OrbitCamControlRow>`, add `with_*` + accessor).
  - `snapshot.rs` — `row_active` factors in direction.
  - `layout.rs` — `build_guidance_group` matches row direction vs live direction.
  - `display.rs` — `CameraGuidanceDisplay`/`...State` carries the live zoom
    direction alongside sources, held through the existing
    `SOURCE_HOLD_SECONDS` window.
  - `mod.rs` — observers set live direction from the bound camera's
    `OrbitCamInput` zoom sign on `OrbitCamInteractionStarted` /
    `OrbitCamInteractionSourcesChanged` (the input is finalized before the event
    fires, so the sign is current). On `Ended` keep the captured direction.
- Match rule: a zoom row lights when its source is active for that (kind, speed)
  group AND its `ZoomDirection` equals the live zoom direction. Orbit/pan rows
  (`None`) match by source only, as today.

## Files

- `bevy_lagrange`: `input/control_summary.rs` (type + split + tag);
  `lib.rs` + `input/mod.rs` (export `ZoomDirection`); `bindings/builder.rs` +
  `adapter/inject.rs` + `lib.rs` + `input/mod.rs` (delete `WheelZoomPolarity`).
- `fairy_dust`: `camera_control_panel/{guidance,snapshot,layout,display,mod}.rs`.

## Status / sequence

1. DONE — `TrackpadZoomTransform` deleted, split into `spawn_trackpad_binding` +
   `spawn_trackpad_zoom_binding` + `insert_trackpad_condition` (`adapter/install.rs`).
2. DONE — `zoom_direction` → `zoom_inversion` rename (user, in editor).
3. DONE — `ZoomDirection {In, Out}` enum + `Option<ZoomDirection>` on
   `OrbitCamControlRow`; `describe_zoom_held_entry` splits the smooth-zoom held
   bindings (single trigger tagged by scale sign; keyboard `+`/`-` split per
   entry); `push_zoom_pair` / `push_trackpad_zoom_pair` synthesize the wheel /
   pinch / smooth-scroll in/out rows with the locked labels; `inversion_sign`
   flips only the wheel/pinch/smooth-scroll tags (native triggers/keys derive
   their own sign and are not inverted at runtime). Tested.
4. DONE — panel highlight wired to live zoom sign: `CameraGuidanceRow` carries
   the tag, `row_active` matches direction (unknown → light both), display state
   holds the live direction through the `SOURCE_HOLD_SECONDS` window, and the
   observers read it from `OrbitCamInput` (written before the lifecycle event
   fires — confirmed in `lifecycle::finalize_orbit_cam_input`). Tested.
5. DONE — `WheelZoomPolarity` deleted; `OrbitCamMouseWheelZoom` is a marker
   struct; `zoom_signed` drops the polarity param.
6. DONE — `ZoomInversion` variant docs reworded; stale "zoom direction policy"
   accessor/builder/module doc comments corrected.

## Adjacent panel tasks (tracked separately, not part of zoom)

- DONE — panel `→` arrows now share a left-aligned action column (`layout.rs`).
- DONE — Shift-C preset cycling disabled in the gamepad example via
  `lock_camera_preset()` (sets `CameraPresetSwitching::Disabled`); the
  keyboard-shortcut overlay drops its entry to match.
