# `fairy_dust` restructure plan

The crate is the example-helper sprinkled into bevy_hana examples: a fluent `SprinkleBuilder` that opts into orbit camera, transparency, restart, BRP extras, lighting, primitives, screen panels (title bar / description), and the camera-control panel. Today every file lives at the flat `src/` root — 11 files, 3036 lines, with one 1029-line file and two more above the style-guide split threshold. The target is a thin root that lists each capability as its own module, plus a `ui/` directory grouping the two diegetic-panel modules that share styling. Splits of the two over-large modules and of `lib.rs` follow in Phase 2.

## Phase overview

| Phase | What                                                                              | Risk | Rough size                          |
|-------|-----------------------------------------------------------------------------------|------|-------------------------------------|
| 1     | Placement — group the diegetic-panel modules under `ui/` and extract shared theme | Low  | 2 file moves + 1 new file, 1 commit |
| 2     | Split `camera_control_panel.rs` (1029 lines) into a directory module              | Med  | 1 directory module, 1 commit        |
| 3     | Split `lib.rs` (900 lines) — extract typestate builders into `builder/`           | Med  | 1 directory module, 1 commit        |
| 4     | Split `screen_panels.rs` (406 lines) into `description` + `title_bar`             | Low  | 1 directory module, 1 commit        |

## Phase 1 — Placement

### Proposed layout

```
crates/fairy_dust/src/
├── lib.rs                       # split in Phase 3
├── brp_extras.rs
├── camera_home.rs
├── lighting.rs
├── orbit_cam.rs
├── primitive.rs
├── restart.rs
├── save_window_position.rs
├── transparency.rs
└── ui/
    ├── mod.rs
    ├── theme.rs                 # NEW — extracted shared styling constants
    ├── camera_control_panel.rs  # split in Phase 2
    └── screen_panels.rs         # split in Phase 4
```

### Moves, with rationale

**New directory: `ui/`** — groups the two modules that build `bevy_diegetic` panels (title bar, description panel, camera-guidance panel). They are the only modules in the crate that drive panel layout trees and use `LayoutTextStyle`, `Padding`, `CornerRadius`, `Border`. Both currently duplicate the same seven styling constants. Adding the dedup'd `theme.rs` gives the directory a third member and a clear cohesion claim: "diegetic-panel UI built on bevy_diegetic, sharing one styling source".

Files moving in:

| From                                | To                                |
|-------------------------------------|-----------------------------------|
| `src/camera_control_panel.rs`       | `src/ui/camera_control_panel.rs`  |
| `src/screen_panels.rs`              | `src/ui/screen_panels.rs`         |

New file:

| Path                | Contents                                                                                          |
|---------------------|---------------------------------------------------------------------------------------------------|
| `src/ui/theme.rs`   | The duplicated block: `RADIUS`, `FRAME_PAD`, `BORDER`, `INSET`, `INNER_RADIUS`, `INNER_BG`, `BORDER_ACCENT`, `BORDER_DIM`, `TITLE_COLOR`, plus the `TITLE_SIZE` font constant (also duplicated between the two files). Each constant becomes `pub(crate)`. |

### What stays where

- `orbit_cam.rs`, `transparency.rs`, `brp_extras.rs`, `save_window_position.rs`, `lighting.rs`, `restart.rs`, `primitive.rs`, `camera_home.rs` stay at the root. Each is one capability the `SprinkleBuilder` opts into, sized 9–222 lines, with a single `install`/`spawn` entry and one or two associated types. They form the crate's public capability menu; relocating them into a `capabilities/` subdir would add a layer without adding cohesion — none of them share types or systems with each other.
- `camera_home.rs` does not move into `ui/`. It is a camera-pose feature (registers an invisible framing cube + `AnimateToFit` + H-key binding). It reads `TitleBarControlState` only to update a chip highlight when the title bar is also installed — that is a soft optional integration, not a UI-rendering responsibility.
- `lib.rs` does not move in Phase 1; its split is Phase 3.
- The `orbit_cam::FairyDustOrbitCam` marker stays in `orbit_cam.rs`. `camera_home` and `transparency` both query it; the marker belongs with the module that spawns the entity it tags.

### Module re-exports

`src/ui/mod.rs`:

```rust
pub(crate) mod theme;

pub mod camera_control_panel;
pub mod screen_panels;
```

`src/lib.rs` changes (only the relevant lines):

```rust
// before
mod camera_control_panel;
mod screen_panels;
pub use camera_control_panel::CameraGuidance;
pub use camera_control_panel::CameraGuidanceRow;
pub use screen_panels::DescriptionPanel;
pub use screen_panels::TitleBar;
pub use screen_panels::TitleBarControlState;

// after
mod ui;
pub use ui::camera_control_panel::CameraGuidance;
pub use ui::camera_control_panel::CameraGuidanceRow;
pub use ui::screen_panels::DescriptionPanel;
pub use ui::screen_panels::TitleBar;
pub use ui::screen_panels::TitleBarControlState;
```

The four external workspace consumers (`crates/bevy_lagrange/examples/programmatic_control.rs`, `zoom_to_fit.rs`, `orbit_cam_manual.rs`, `crates/bevy_diegetic/examples/world_text.rs`) reach `Anchor`, `DescriptionPanel`, `TitleBar`, `TitleBarControlState`, `Face`, `cube_face_text`, `CameraGuidance`, `CameraGuidanceRow` through `fairy_dust::Name` — the crate-root re-exports above keep every one of those paths valid without changes to the examples.

Within `ui/camera_control_panel.rs` and `ui/screen_panels.rs`, replace the local `const RADIUS = …` block with:

```rust
use crate::ui::theme::{
    BORDER_ACCENT, BORDER_DIM, FRAME_PAD, INNER_BG, INNER_RADIUS, INSET, RADIUS, TITLE_COLOR,
    TITLE_SIZE,
};
```

Anything currently `super::ensure_plugin` in those files becomes `crate::ensure_plugin` (one extra path segment — the modules now live one level deeper).

### Sequencing

The phase ships as one commit; the numbered steps are the in-flight checkpoints where you re-run `cargo build -p fairy_dust` + `cargo nextest run -p fairy_dust` between them.

1. Create `crates/fairy_dust/src/ui/` with `mod.rs` declaring `theme`, `camera_control_panel`, `screen_panels` (the latter two as empty placeholder files — they will be filled in step 2). Add `mod ui;` to `lib.rs`.
2. Move `camera_control_panel.rs` and `screen_panels.rs` into `src/ui/`. In `lib.rs` switch the five `pub use` lines to the `ui::…` paths and delete the two `mod camera_control_panel;` / `mod screen_panels;` declarations. Inside both moved files, swap `super::ensure_plugin` references to `crate::ensure_plugin`. Update `camera_home.rs:19` from `use crate::screen_panels::TitleBarControlState;` to `use crate::ui::screen_panels::TitleBarControlState;` — that is the only intra-crate `use crate::camera_control_panel::…` or `use crate::screen_panels::…` line in the codebase. Build should pass; constants are still duplicated.
3. Create `src/ui/theme.rs` with the nine constants listed above, each `pub(crate)`. In both panel files, delete the local `const` block and add the `use crate::ui::theme::…` import in one edit per file — both deletions and both new `use` lines should land in the same git operation, since an intermediate state with the `use` added but the local block still present, or vice versa, would not compile. Run `cargo +nightly fmt` to settle imports.

Dependency ordering: step 1 establishes the directory before step 2 moves files; step 3 depends on the moves to know where `theme.rs` lives. The whole sequence is leaves-first relative to the rest of the crate — no other module imports anything from these two files except via the public re-exports in `lib.rs`, which step 2 keeps stable. External example crates reach the moved types via `fairy_dust::…` crate-root re-exports, so they are unaffected.

## Phase 2 — Split `ui/camera_control_panel.rs`

After Phase 1 this file is `crates/fairy_dust/src/ui/camera_control_panel.rs` (1029 lines: 903 prod + 126 inline tests; the test block at line 904 stays where the items it tests end up — inlined per submodule or kept together with the surfaces they cover; the split below colocates each test cluster with its anchor type).

The file holds five separable concerns: the public guidance config (`CameraGuidance` + `CameraGuidanceRow`), the display state machine (`CameraGuidanceDisplayState` + `CameraGuidanceDisplaySlot` + `CameraGuidanceDisplay`), snapshot resolution (`CameraGuidanceSnapshot` + label helpers), the panel marker and Bevy systems, and the diegetic layout-tree builders. Each cluster is named below after its anchor type (per `name-submodules-after-anchor-types.md`).

### Target layout

```
ui/camera_control_panel/
├── mod.rs        # install + ensure_panel_plugins + CameraGuidancePanel marker
│                 # + spawn observers + the refresh_* Bevy systems
│                 # + re-exports of public types
├── config.rs     # CameraGuidance, CameraGuidanceContent, CameraGuidanceRow
├── display.rs    # CameraGuidanceDisplayState, CameraGuidanceDisplaySlot, CameraGuidanceDisplay
├── snapshot.rs   # CameraGuidanceSnapshot + resolve_guidance_snapshot
│                 # + snapshot_from_summary + resolve_mode_labels
│                 # + preset_mode_value + row_active + source_label
│                 # + kind_label + push_source_label
└── layout.rs     # build_guidance_tree + build_guidance_layout
                  # + build_guidance_table + build_guidance_group
                  # + unlit_panel_material + panel-local size/color constants
                  #   that are not in ui/theme.rs
```

### What goes where

| Submodule | Current lines (in `camera_control_panel.rs`) | Items |
|-----------|----------------------------------------------|-------|
| `config.rs`   | 44–191                                      | `CameraGuidance`, its `Default` + inherent impls; `CameraGuidanceContent`; `CameraGuidanceRow`, its inherent impl + `From<OrbitCamControlRow>` |
| `display.rs`  | 207–376                                     | `CameraGuidanceDisplayState`, `CameraGuidanceDisplaySlot`, `CameraGuidanceDisplay` plus all their impls |
| `snapshot.rs` | 199–205 + 756–902                           | `CameraGuidanceSnapshot`; `resolve_guidance_snapshot`, `snapshot_from_summary`, `resolve_mode_labels`, `preset_mode_value`, `row_active`, `source_label`, `kind_label`, `push_source_label` |
| `layout.rs`   | 384–402 (panel-local constants `HEADER_SIZE`, `LABEL_SIZE`, `HEADER_COLOR`, `LABEL_COLOR`, `ACTIVE_COLOR`, `SOURCE_COLOR`, `SOURCE_HOLD_SECONDS`, `TABLE_COLUMN_GAP`, `TABLE_ROW_GAP`, `TABLE_GROUP_GAP`, `TABLE_DIVIDER_WIDTH`, `ACTION_COLUMN_MIN_WIDTH`) + 588–754 | `unlit_panel_material`, `build_guidance_tree`, `build_guidance_layout`, `build_guidance_table`, `build_guidance_group` |
| `mod.rs`      | 194–196 + 404–586                           | `CameraGuidancePanel` marker; `install`, `ensure_panel_plugins`, `attach_default_guidance_on_orbit_cam_add`, `spawn_guidance_panel_on_add`; the six `refresh_*` / `update_*` systems |

Constants in `ui/theme.rs` (shared): `RADIUS`, `FRAME_PAD`, `BORDER`, `INSET`, `INNER_RADIUS`, `TITLE_SIZE`, `INNER_BG`, `BORDER_ACCENT`, `BORDER_DIM`, `TITLE_COLOR`. The panel-local constants listed in the `layout.rs` row above stay private to this directory; they are not duplicated in `screen_panels.rs`.

`CameraGuidanceContent` is the file-private enum on `CameraGuidance::content`. After the split `resolve_guidance_snapshot` in `snapshot.rs` pattern-matches against `CameraGuidanceContent::Auto` and `CameraGuidanceContent::Rows`, so the enum must be visible to siblings within the directory. Mark it `pub(super)` inside `config.rs` (instead of file-private). No re-export from `mod.rs` is needed; `snapshot.rs` reaches it as `use super::config::CameraGuidanceContent;`.

### Sequencing

The phase ships as one commit; the numbered steps are in-flight checkpoints where you re-run `cargo build -p fairy_dust` + `cargo nextest run -p fairy_dust`. Extract in this order so each step compiles against what came before:

1. Create directory `ui/camera_control_panel/` with an empty `mod.rs`. Convert the existing `ui/camera_control_panel.rs` into the directory's `mod.rs` (rename + move) so the module path is unchanged for callers. Build green here.
2. Extract `config.rs`: move `CameraGuidance` / `CameraGuidanceContent` / `CameraGuidanceRow` + impls. Mark `CameraGuidanceContent` as `pub(super)` in its new home (currently file-private). In `mod.rs` add `mod config;` and `pub use config::{CameraGuidance, CameraGuidanceRow};`.
3. Extract `display.rs`: move the three display types and their impls. `mod display;` in `mod.rs`; references inside `mod.rs` become `use display::CameraGuidanceDisplay;` etc.
4. Extract `snapshot.rs`: move `CameraGuidanceSnapshot` and the eight label/resolution helpers. The snapshot resolver and label helpers form one cluster — the helpers exist only to compute the snapshot. `snapshot.rs` opens with `use super::config::{CameraGuidance, CameraGuidanceContent, CameraGuidanceRow};` so the pattern-match on `guidance.content` continues to resolve. `mod snapshot;` + targeted `use snapshot::…;` lines in `mod.rs`.
5. Extract `layout.rs`: move the four `build_guidance_*` functions, `unlit_panel_material`, and the panel-local constants. `mod layout;` + `use layout::build_guidance_tree;` in `mod.rs`.
6. Split the inline test module. The current block at lines 904–1029 holds four tests: three exercise `CameraGuidanceDisplayState` / `CameraGuidanceDisplaySlot` only — move them into a `#[cfg(test)] mod tests` inside `display.rs`. The fourth (`source_label_lists_sources_without_brackets`) exercises only `source_label` — move it into a `#[cfg(test)] mod tests` inside `snapshot.rs`. `mod.rs` ends with no test module. This avoids adding cross-submodule `pub use` lines purely for the test block.
7. `mod.rs` now holds the marker, install, observers, refresh systems, and the four `mod`/`use` lines. Run `cargo +nightly fmt`.

Dependency ordering: `config` is referenced by everything else, so it lands first. `display` depends only on `config`. `snapshot` depends on `config` (it reads `CameraGuidanceRow` fields) and on the same `OrbitCam*` types as the rest. `layout` depends on `config` (`CameraGuidanceRow`), `snapshot` (`CameraGuidanceSnapshot`), and `ui::theme`. The refresh systems in `mod.rs` depend on every submodule, which is why they stay in `mod.rs` until the leaves are settled.

The single internal `use crate::ui::theme::…;` line each submodule needs is the only addition to imports; the rest of the changes are path-internal.

## Phase 3 — Split `lib.rs`

`lib.rs` is 900 lines: doc comment + 5 module declarations + 9 `pub use` re-exports + `LOG_FILTER` const + 2 typestate markers + 4 builder structs + `sprinkle_example()` factory + 11 large `impl` blocks across the 4 builders + the `ensure_plugin` helper. The size is driven by the builders. Pull them into a `builder/` directory and leave `lib.rs` as a thin facade.

### Target layout

```
src/
├── lib.rs            # doc + mod decls + pub-use re-exports + LOG_FILTER
│                     # + sprinkle_example() + ensure_plugin (pub(crate))
└── builder/
    ├── mod.rs        # struct defs for the 4 builders; NoOrbitCam / WithOrbitCam markers;
    │                 # module declarations
    ├── sprinkle.rs   # all SprinkleBuilder<*> impls
    ├── primitive.rs  # all PrimitiveBuilder<*> impls
    ├── camera_home.rs # all CameraHomeBuilder<*> impls
    └── title_bar.rs  # all TitleBarBuilder<*> impls
```

### What goes where

| Submodule | Current `lib.rs` lines | Items |
|-----------|------------------------|-------|
| `builder/mod.rs` | 73–84, 89–122 | `NoOrbitCam`, `WithOrbitCam` markers + their doc; the four `pub struct …Builder<S> { … }` definitions with their docs; `mod sprinkle; mod primitive; mod camera_home; mod title_bar;` |
| `builder/sprinkle.rs` | 154–300, 455–486, 864–880 | `impl<S> SprinkleBuilder<S>`, `impl SprinkleBuilder<NoOrbitCam>`, `impl SprinkleBuilder<WithOrbitCam>` |
| `builder/primitive.rs` | 302–452, 488–507, 882–889 | `impl<S> PrimitiveBuilder<S>`, `impl PrimitiveBuilder<NoOrbitCam>`, `impl PrimitiveBuilder<WithOrbitCam>` |
| `builder/camera_home.rs` | 509–633, 635–658, 660–667 | `impl<S> CameraHomeBuilder<S>`, `impl CameraHomeBuilder<NoOrbitCam>`, `impl CameraHomeBuilder<WithOrbitCam>` |
| `builder/title_bar.rs` | 669–831, 833–851, 853–860 | `impl<S> TitleBarBuilder<S>`, `impl TitleBarBuilder<NoOrbitCam>`, `impl TitleBarBuilder<WithOrbitCam>` |
| `lib.rs` (kept) | 1–72, 137–150, 891–900 | The crate doc; `mod` declarations (`mod ui; mod builder; mod brp_extras;` …); the nine `pub use` re-exports plus new re-exports for builder types; `LOG_FILTER`; `sprinkle_example()`; `pub(crate) fn ensure_plugin(…)` |

After Phase 3, `lib.rs` is ~110 lines: doc, module declarations, re-exports, `LOG_FILTER`, `sprinkle_example`, `ensure_plugin`. The four builder impl files are 80–310 lines each.

### Re-exports

`lib.rs` gains:

```rust
pub use builder::{
    CameraHomeBuilder, NoOrbitCam, PrimitiveBuilder, SprinkleBuilder, TitleBarBuilder, WithOrbitCam,
};
```

External callers continue to write `fairy_dust::SprinkleBuilder<NoOrbitCam>` etc. — paths unchanged.

`builder/mod.rs` exposes the markers and builder types as `pub` so `lib.rs` can re-export them. Each builder impl file references the types via `use super::{SprinkleBuilder, NoOrbitCam, WithOrbitCam};` etc.

### Sequencing

One commit; in-flight checkpoints between steps. The factory and `ensure_plugin` must stay reachable throughout, so they remain in `lib.rs` from start to finish — only the builder impls move.

1. Create `crates/fairy_dust/src/builder/mod.rs` with the four struct defs and the two typestate markers cut from `lib.rs` (lines 73–84 and 89–122 with their docs). Add `pub mod sprinkle; pub mod primitive; pub mod camera_home; pub mod title_bar;` (empty placeholder files for now). In `lib.rs` add `mod builder;` and `pub use builder::{CameraHomeBuilder, NoOrbitCam, PrimitiveBuilder, SprinkleBuilder, TitleBarBuilder, WithOrbitCam};`. At this point `lib.rs` still contains the impl blocks; `builder/mod.rs` only contains type definitions. Build green here — the impl blocks still type-check because they are in the same crate as the types.

   The four `impl` blocks in `lib.rs` reference the types directly. Once the types move to `builder::`, the impls in `lib.rs` need either a `use builder::{…};` line at the top of `lib.rs` or each impl block prefixed with the path. Add the `use` line.

2. Move `impl<S> SprinkleBuilder<S>`, `impl SprinkleBuilder<NoOrbitCam>`, `impl SprinkleBuilder<WithOrbitCam>` into `builder/sprinkle.rs`. At the top of `sprinkle.rs`: `use super::{NoOrbitCam, SprinkleBuilder, WithOrbitCam};` plus the same Bevy + capability imports the impl blocks currently rely on. Move the relevant module imports — `use crate::brp_extras;`, `use crate::camera_home::…;`, `use crate::lighting;`, `use crate::orbit_cam::…;`, `use crate::primitive::…;`, `use crate::restart;`, `use crate::save_window_position;`, `use crate::transparency;`, `use crate::ui::camera_control_panel;`, `use crate::ui::screen_panels::…;` — into `sprinkle.rs`.
3. Repeat step 2 for `builder/primitive.rs` with the three `PrimitiveBuilder<*>` impls and their imports.
4. Repeat step 2 for `builder/camera_home.rs` with the three `CameraHomeBuilder<*>` impls.
5. Repeat step 2 for `builder/title_bar.rs` with the three `TitleBarBuilder<*>` impls.
6. `lib.rs` now contains only doc + mod decls + re-exports + `LOG_FILTER` + `sprinkle_example()` + `ensure_plugin`. Run `cargo +nightly fmt`.

Dependency ordering: type definitions land in `builder/mod.rs` first so every impl file can `use super::{…}`. Builders are independent of each other — no `SprinkleBuilder` impl calls a `PrimitiveBuilder` method (they only construct each other via `parent: SprinkleBuilder<S>`), so the four impl files have no inter-file dependency. Each can move in isolation; the order above is convenience, not necessity.

`ensure_plugin` is referenced from every capability module (`brp_extras`, `transparency`, `screen_panels`, `camera_control_panel`, `restart`, `save_window_position`, `lighting`) plus the builder impls. Keep it in `lib.rs` and mark it `pub(crate)` (already is). All call sites already write `crate::ensure_plugin` or `super::ensure_plugin` — the latter break when `screen_panels` etc. move under `ui/` in Phase 1 and are already changed to `crate::ensure_plugin` by then.

## Phase 4 — Split `ui/screen_panels.rs`

After Phase 1 this file is `crates/fairy_dust/src/ui/screen_panels.rs` (406 lines: 378 prod + 28 inline tests). It is under the 500-line size threshold from `when-to-split-a-module.md`, so this phase is a cohesion-driven split, not a size-driven one. The file holds two independent panel types — `DescriptionPanel` (sidebar) and `TitleBar` + `TitleBarControlState` (top bar). They share only the styling constants that move to `ui/theme.rs` in Phase 1 and the `panel_frame` / `unlit_panel_material` helpers. After Phase 1, what is left has two clean clusters and no remaining cohesion.

If the user prefers to defer Phase 4, the rest of the plan still stands.

### Target layout

```
ui/screen_panels/
├── mod.rs           # re-exports + install_description + install_title_bar
│                    # + panel_frame + unlit_panel_material (shared by both submodules)
├── description.rs   # DescriptionPanel + DescriptionPanelMarker
│                    # + spawn_description_panel + build_description_layout
└── title_bar.rs     # TitleBar + TitleBarControlState + TitleBarMarker
                     # + spawn_title_bar + refresh_changed_title_bar
                     # + build_title_bar_tree + build_title_bar_layout
                     # + title_separator
```

### What goes where

| Submodule | Current `screen_panels.rs` lines | Items |
|-----------|-----------------------------------|-------|
| `description.rs` | 28–65, 163, 218–236, 284–302 | `DescriptionPanel`, `DescriptionPanelMarker`, `spawn_description_panel`, `build_description_layout` |
| `title_bar.rs`   | 69–160, 166, 238–275, 304–340, 369–377 | `TitleBar`, `TitleBarControlState`, `TitleBarMarker`, `spawn_title_bar`, `refresh_changed_title_bar`, `build_title_bar_tree`, `build_title_bar_layout`, `title_separator` |
| `mod.rs`         | 168–189 (panel-local consts not promoted to `theme.rs`: `INNER_PAD`, `BODY_SIZE`, `CONTROL_SIZE`, `DESCRIPTION_WIDTH`, `BODY_COLOR`, `CONTROL_ACTIVE_COLOR`, `CONTROL_INACTIVE_COLOR`, `DIVIDER_COLOR`) + 190–216 + 277–282 + 342–367 + 379–406 | `install_description`, `install_title_bar`, `unlit_panel_material`, `panel_frame`, panel-local constants, the existing test module |

### Sequencing

One commit; checkpoints between steps.

1. Convert `ui/screen_panels.rs` to directory module `ui/screen_panels/` with the file becoming `mod.rs` (rename + move). Build green.
2. Extract `description.rs`: move `DescriptionPanel`, `DescriptionPanelMarker`, `spawn_description_panel`, `build_description_layout`. Add `mod description;` and `pub use description::DescriptionPanel;` to `mod.rs`. Internally `install_description` calls `description::spawn_description_panel`. `description.rs` needs `use crate::ui::theme::…;` and `use super::{panel_frame, unlit_panel_material};` (or copy those helpers up to a sibling — `mod.rs` is fine).
3. Extract `title_bar.rs`: move `TitleBar`, `TitleBarControlState`, `TitleBarMarker`, the spawn/refresh systems, and the three `build_title_bar_*` / `title_separator` layout helpers. `mod title_bar;` + `pub use title_bar::{TitleBar, TitleBarControlState};` in `mod.rs`. `title_bar.rs` keeps the `use crate::camera_home::CameraHomeConfig;` line that detects the home chip auto-prepend.
4. Move the existing test module into `title_bar.rs`. Both tests (`title_bar_control_state_tracks_active_labels`, `title_bar_can_seed_active_controls`) exercise only `TitleBar` / `TitleBarControlState`, so neither leaves a test in `description.rs` nor in `mod.rs`.
5. Run `cargo +nightly fmt`.

Dependency ordering: `description` and `title_bar` are independent leaves. Either can extract first. The shared helpers (`panel_frame`, `unlit_panel_material`) live in `mod.rs` and are referenced via `super::` from both submodules; no `description`/`title_bar` cycle is possible.
