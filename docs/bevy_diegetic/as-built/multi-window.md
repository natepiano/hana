# bevy_diegetic — multi-window screen-space panels

> **Archived 2026-06-07 — implemented.** Its deletion commit (f29c568,
> 2026-05-18) verified it against the codebase. The planned public API is the
> current one: per-panel `WindowRef` (default `Primary`), `.window(...)` /
> window-entity sugar on the builder, and the `WindowRef → Entity` resolver —
> the resolver in `src/screen_space/mod.rs:63` matches the helper sketched
> below. The multicam plan this unblocked is archived at
> [`../../bevy_lagrange/as-built/multicam.md`](../../bevy_lagrange/as-built/multicam.md).

Teach `bevy_diegetic`'s screen-space panel system to work correctly when more
than one `Window` entity exists. Today the system silently disables itself —
any app that spawns a second window loses all screen-space panels (title
bars, control panels, description panels). This document plans the fix.

## Why this is a prerequisite

Every other multi-window feature in the workspace depends on this. Concretely
blocking right now:

- `crates/bevy_lagrange/examples/viewports_windows.rs` — the example whose
  conversion to canonical fairy_dust prompted this work (see
  `docs/bevy_lagrange/as-built/multicam.md`). Its second OS window is the
  reproduction case.
- Any future fairy_dust capability that wants per-window HUD (inspector
  panes, tool palettes, secondary-display readouts).

The decision to do this work as its own focused change — not as a sub-phase
of the multicam plan — reflects that the affected systems and the public API
choices belong inside `bevy_diegetic`, not inside a downstream example.

## Today's behavior (the bug)

Three coupled assumptions break the moment a second `Window` exists:

1. **Positioning queries a single window.**
   `crates/bevy_diegetic/src/screen_space/mod.rs:57-63`:
   ```rust
   fn position_screen_space_panels(
       windows: Query<&Window>,
       mut panels: Query<(&mut Transform, &mut DiegeticPanel, &ComputedDiegeticPanel)>,
   ) {
       let Ok(window) = windows.single() else {
           return;
       };
       ...
   }
   ```
   `single()` returns `Err` when there are 0 or >1 windows. The early
   return silently disables positioning for *every* screen-space panel in
   the app — even the ones in the primary window. The user sees panels
   either at frame-1 placeholder positions or whatever stale transform
   they last had.

2. **Overlay cameras default to the primary window.**
   `screen_space/mod.rs:174-198` — the `setup_screen_space_view` observer
   spawns one `Camera` per unique `(camera_order, render_layers)` pair.
   That `Camera` is constructed with `..default()`, which leaves
   `Camera.target` as `RenderTarget::Window(WindowRef::Primary)`. So even
   if positioning were fixed, panels intended for the second window have
   no camera *drawing* them there.

3. **The overlay-camera dedup key is window-blind.**
   `screen_space/mod.rs:167-172` — the `already_exists` check matches on
   `(order, render_layers)` only. Two panels with the same order/layers
   in two different windows would (incorrectly) share one camera.

The result of all three is: any app with two windows shows nothing
screen-space at all, anywhere. The black-screen failure of the previous
multicam attempt is consistent with this.

## Goals

- Each `DiegeticPanel::screen()` panel knows which window it lives in.
- `position_screen_space_panels` iterates windows and resolves each panel
  against its target window's dimensions. Single-window apps behave
  identically to today.
- `setup_screen_space_view` keys overlay-camera dedup by
  `(camera_order, render_layers, window)` and explicitly sets
  `Camera.target = RenderTarget::Window(WindowRef::Entity(window))` on the
  spawned camera.
- `cleanup_screen_space_view` cleans up the matching `(order, layers, window)`
  triple when the last panel using it goes away.
- All existing examples (`zoom_to_fit`, `world_text`, every other
  `bevy_diegetic` example, every existing fairy_dust capability) keep
  working without code changes.

### Non-goals (defer)

- Cross-window panels (a single panel that mirrors content into multiple
  windows). One panel = one window.
- Reparenting a panel from one window to another at runtime. The window
  association is set at build time.
- Auto-discovering "the right window" based on cursor position or focus.
  Callers say which window explicitly; default is `Primary`.
- Per-window camera-order namespacing. Orders still collide across
  windows if a caller chooses badly — that's the caller's job to manage.

## Design

### Public API — opt-in, default-compatible

Add a window-reference field to `CoordinateSpace::Screen`:

```rust
// crates/bevy_diegetic/src/panel/coordinate_space.rs
pub enum CoordinateSpace {
    World { ... },
    Screen {
        position:      ScreenPosition,
        width:         Sizing,
        height:        Sizing,
        camera_order:  isize,
        render_layers: RenderLayers,
        /// Window this panel renders into. Defaults to `WindowRef::Primary`.
        /// Use `WindowRef::Entity(...)` to pin a panel to a specific window.
        window:        WindowRef,
    },
}
```

Builder method on `DiegeticPanelBuilder<Screen, ...>`:

```rust
// crates/bevy_diegetic/src/panel/builder.rs
pub fn window(mut self, window: WindowRef) -> Self {
    if let CoordinateSpace::Screen { window: w, .. } = &mut self.data.coordinate_space {
        *w = window;
    }
    self
}

/// Sugar for `.window(WindowRef::Entity(entity))`.
pub fn window_entity(mut self, entity: Entity) -> Self {
    self.window(WindowRef::Entity(entity))
}
```

Default = `WindowRef::Primary`. Any existing call site that doesn't call
`.window(...)` keeps targeting the primary window — identical to today.

`new_screen()` at `panel/builder.rs:140` initializes the new field to
`WindowRef::Primary` so the default round-trips.

### Resolver helper

A small helper that converts `WindowRef` → `Entity` against the world's
`PrimaryWindow`. Reused by every system below.

```rust
// crates/bevy_diegetic/src/screen_space/mod.rs
fn resolve_window_ref(
    window_ref: WindowRef,
    primary: &Query<Entity, With<PrimaryWindow>>,
) -> Option<Entity> {
    match window_ref {
        WindowRef::Primary => {
            let resolved = primary.single().ok();
            if resolved.is_none() {
                bevy::log::warn_once!(
                    "bevy_diegetic: screen panel asked for WindowRef::Primary \
                     but no PrimaryWindow exists; panel will be ignored"
                );
            }
            resolved
        },
        WindowRef::Entity(entity) => Some(entity),
    }
}
```

`primary.single()` is safe here: there is exactly one `PrimaryWindow` per
Bevy app for the foreseeable future. If a future Bevy lifts that, this
helper is the only site to update.

The `warn_once!` catches the headless-test misconfiguration (no
`WindowPlugin`) which would otherwise silently drop panel positioning.
Stale `WindowRef::Entity(e)` references (window despawned but panel
survived) are handled by the window-close observer below — they don't
reach the resolver because the panel is despawned alongside the window.

### Positioning system — iterate windows, resolve per-panel

Replace the `windows.single()` early-return with a per-panel lookup. Each
panel asks for its own window's dimensions.

```rust
fn position_screen_space_panels(
    windows: Query<(Entity, &Window)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut panels: Query<(&mut Transform, &mut DiegeticPanel, &ComputedDiegeticPanel)>,
) {
    // Build a small map: Entity → (width, height). Typically 1–3 entries.
    let mut by_entity: HashMap<Entity, (f32, f32)> = HashMap::new();
    for (entity, window) in &windows {
        let w = window.width();
        let h = window.height();
        if w > 0.0 && h > 0.0 {
            by_entity.insert(entity, (w, h));
        }
    }

    for (mut transform, mut panel, computed) in &mut panels {
        let CoordinateSpace::Screen {
            position,
            width,
            height,
            window: window_ref,
            ..
        } = panel.coordinate_space()
        else {
            continue;
        };
        // Snapshot copies BEFORE mutably borrowing `panel`.
        let position = *position;
        let width_sizing = *width;
        let height_sizing = *height;
        let window_ref = *window_ref;

        let Some(window_entity) = resolve_window_ref(window_ref, &primary) else {
            continue;
        };
        let Some(&(window_width, window_height)) = by_entity.get(&window_entity) else {
            continue;
        };

        // (rest is unchanged from today, just using the per-panel
        //  window_width / window_height instead of globals)
        ...
    }
}
```

Hot-path notes: the `HashMap` is bounded by the window count, typically 1.
For single-window apps the map has one entry and the per-panel lookup is
one `HashMap::get`. No allocation on the steady-state path — could be a
`SmallVec<[(Entity, (f32, f32)); 4]>` if benchmarking shows it matters.
Probably it does not.

### Overlay camera — target the right window, dedup by triple

Two changes to `setup_screen_space_view` at `screen_space/mod.rs:145`:

1. Include the window in the dedup key.
2. Set `Camera.target` explicitly.

```rust
fn setup_screen_space_view(
    trigger: On<Add, DiegeticPanel>,
    panels: Query<&DiegeticPanel>,
    cameras: Query<&ScreenSpaceCamera>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok(panel) = panels.get(trigger.entity) else {
        return;
    };
    let CoordinateSpace::Screen {
        camera_order,
        ref render_layers,
        window: window_ref,
        ..
    } = *panel.coordinate_space()
    else {
        return;
    };
    let Some(window_entity) = resolve_window_ref(window_ref, &primary) else {
        return;
    };

    commands.entity(trigger.entity).insert(render_layers.clone());

    let already_exists = cameras.iter().any(|cam| {
        cam.order == camera_order
            && cam.render_layers == *render_layers
            && cam.window == window_entity
    });
    if already_exists {
        return;
    }

    commands.spawn((
        ScreenSpaceCamera {
            render_layers: render_layers.clone(),
            order:         camera_order,
            window:        window_entity, // new field
        },
        Camera3d { ... },
        Camera {
            order: camera_order,
            target: RenderTarget::Window(WindowRef::Entity(window_entity)),
            clear_color: ClearColorConfig::None,
            ..default()
        },
        ...
    ));

    commands.spawn((
        ScreenSpaceLight {
            render_layers: render_layers.clone(),
        },
        DirectionalLight { ... },
        ...
    ));
}
```

Only `ScreenSpaceCamera` gains a `window: Entity` field. The light stays
keyed by `render_layers` only — it's a singleton per layer, app-wide.

Reason: `DirectionalLight` contributions on a shared `RenderLayers`
accumulate in Bevy's PBR shader. Two identical 5000-lux lights on
layer 31 would illuminate any layer-31 panel at ~10 000 lux from
*both* cameras' perspectives, so opening a second window would visibly
brighten every panel app-wide. Keeping one light per layer matches
today's single-window behavior exactly.

The cleanup logic (next section) handles the cross-window correctness:
the light is despawned only when no panel on its layer survives in
*any* window.

### Cleanup — two complementary paths, one owner

There are two distinct teardown triggers, but only one of them owns
camera/light despawn:

**A. Panel removed** — `cleanup_screen_space_view` at
`screen_space/mod.rs:277-323`. This is the **sole** owner of camera and
light despawn. Update its `still_in_use` predicate and the camera
despawn loop to compare `(camera_order, render_layers, window)`. The
light despawn loop keeps its existing `render_layers`-only key, but
its `still_in_use` check now counts panels across *all* windows.

**B. Window removed** — new observer `cleanup_screen_space_on_window_close`,
triggered by `On<Remove, Window>`. Despawns **only** panels whose
window matches the removed entity. Each despawn fires observer A,
which then cascade-cleans the camera and (if it was the last panel on
that layer anywhere) the light.

```rust
fn cleanup_screen_space_on_window_close(
    trigger: On<Remove, Window>,
    panels: Query<(Entity, &DiegeticPanel)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let removed = trigger.entity;
    for (entity, panel) in &panels {
        let CoordinateSpace::Screen { window: window_ref, .. } =
            panel.coordinate_space()
        else {
            continue;
        };
        if resolve_window_ref(*window_ref, &primary) == Some(removed) {
            commands.entity(entity).despawn();
        }
    }
}
```

Single-owner cleanup prevents a double-despawn panic: only one observer
ever issues `despawn()` on a given camera or light entity. In Bevy 0.18,
calling `despawn()` on an already-despawned entity panics — by routing
all camera/light teardown through observer A, that risk is eliminated
by construction.

This is observer-based, not hierarchy-based — we deliberately do **not**
parent cameras/lights/panels to the window entity. `Camera` carries a
`Transform`, `Window` does not; parenting would introduce an unusual
hierarchy convention that other crates might not expect.

### What we do NOT touch

- `propagate_screen_space_render_layers` (`mod.rs:224-245`) — purely
  layer propagation, no window awareness needed.
- `panel/compute_layout.rs` — layout math runs in panel-local units; the
  window is only relevant for the final positioning step.
- `panel/gizmos.rs` — draws debug gizmos. Will need a separate look in
  Phase 4 of this plan, in case it also assumes one window.

## Phases — each ships green

### Phase 0 — capture the baseline

Test baseline (already recorded): `cargo nextest run -p bevy_diegetic`
on `main` at commit `990b3f2` — **189 passed, 1 skipped, 0 failed**.
Phase 1+ keeps this number stable.

Examples at risk of visual regression (the only ones that use
screen-space panels — directly or via fairy_dust's `TitleBar`):

- `screen_space.rs` — canonical screen-space demo
- `panel_rendering.rs`
- `world_text.rs`
- `sdf.rs`
- `typography.rs`
- `units.rs`
- `text_alpha.rs`
- `taa_shimmer.rs`

The other 11 examples (`atlas_pages`, `dimensions`, `font_features`,
`font_loading`, `hue_offset`, `paper_sizes`, `preload_text`, `shadows`,
`side_by_side`, `sizes`, `text_stress`) are pure world-space and not
affected by anything in this plan.

Baseline-capture strategy: this work happens on a worktree
(`../bevy_hana_multicam`, branch `feat/multicam`). The `main` checkout
at `/Users/natemccoy/rust/bevy_hana` stays clean and is used to launch
any of the 8 at-risk examples on demand when a phase needs a visual
comparison. No screenshots are captured up front.

**Exit:** baseline numbers recorded above; at-risk example list known.

### Phase 1 — add the `window` field, default `Primary`

- **First**, grep for exhaustive matches on `CoordinateSpace::Screen`:
  ```
  rg -n 'CoordinateSpace::Screen\s*\{' crates/ --type rust
  ```
  Any site that destructures with named fields and no `..` will fail to
  compile after the field is added. Update those to use `..` or add the
  new field. Today's likely sites: `screen_space/mod.rs:73`, `:154`,
  `:287`, plus `panel/builder.rs:344`, `:365`, `:429`, `:438`, `:447`,
  `:544`, `:614`.
- Add `window: WindowRef` to `CoordinateSpace::Screen` in
  `panel/coordinate_space.rs`.
- Update `new_screen()` at `panel/builder.rs:140` to initialize it to
  `WindowRef::Primary`.
- Add `.window()` and `.window_entity()` builder methods on
  `DiegeticPanelBuilder<Screen, ...>`.
- **No system changes yet.** The new field is silently ignored. Every
  call site keeps targeting the primary window because every panel still
  has `WindowRef::Primary`.

**Exit:** `cargo build -p bevy_diegetic` clean. Every example still
renders identically to Phase 0 (visual diff against screenshots).
`cargo nextest run -p bevy_diegetic` no new failures.

### Phase 2 — resolver helper + iterating positioning

- Add `resolve_window_ref` helper.
- Rewrite `position_screen_space_panels` to iterate windows and resolve
  per-panel as described above.
- Behavior on single-window apps: identical — the map has one entry,
  every panel resolves to the same window, every positioning calc uses
  the same `(w, h)` it would have today.

**Exit:** `cargo nextest run -p bevy_diegetic`; every existing example
still matches the Phase 0 baseline. Add one new unit test in
`screen_space/mod.rs::tests` that spawns two windows and asserts panels
in each window get positioned against their own window's size.

### Phase 3 — overlay camera per-window + window-close observer

- Add `window: Entity` to `ScreenSpaceCamera`. `ScreenSpaceLight` is
  unchanged (singleton per layer, app-wide).
- Update `setup_screen_space_view` to dedup cameras by
  `(order, layers, window)` and to set `Camera.target` explicitly. Light
  dedup remains `layers`-only.
- Update `cleanup_screen_space_view` (panel-remove path):
  - Camera despawn predicate becomes the triple.
  - Light despawn predicate stays `layers`-only, but the `still_in_use`
    check now counts panels across all windows on that layer.
- Add the new `cleanup_screen_space_on_window_close` observer
  (`On<Remove, Window>`). It despawns **only** panels matching the
  removed window — camera and light teardown cascades through the
  existing panel-remove observer, keeping single-owner cleanup.
- Register the new observer in `ScreenSpacePlugin::build`.

**Exit:** all Phase 0 visuals unchanged. Three new integration tests:
1. Two windows, two panels on the same render layer — asserts two
   `ScreenSpaceCamera` entities exist with different `window` fields,
   each camera's `Camera.target` points at its own window, exactly one
   `ScreenSpaceLight` exists for that layer.
2. Same setup, then despawn one window — asserts the corresponding
   camera and panel are gone, the light is still there (other window's
   panel is still using it), and the surviving window's camera/panel
   are untouched.
3. Same setup, despawn both windows — asserts everything (cameras,
   light, panels) is gone.

### Phase 4 — audit sweep

The reviewer already ran the audit; pre-confirmed findings:

- `screen_space/mod.rs:61` — the known `windows.single()` site, fixed in
  Phase 2.
- All other `PrimaryWindow` / `windows.single()` references in
  `bevy_diegetic` are inside `#[cfg(test)]` blocks (e.g.
  `screen_space/mod.rs:328,369`; `panel/compute_layout.rs:192,489,546,621`).
  No production code.
- `panel/gizmos.rs:54-67` — `pixels_per_meter` picks an arbitrary camera
  via `.iter().next()`. **Latent bug** in multi-camera worlds (gizmo
  line width uses wrong camera's ppm in one window) but does not
  black-screen anything. **Out of scope** for this plan; logged as Open
  follow-up below.
- No `bevy_picking` integration in `bevy_diegetic/src` to audit.
- RTT uses `RenderTarget::Image`, not `Window` — window-agnostic.

**Exit:** rerun `rg -n 'windows\.single|PrimaryWindow|WindowRef::Primary'
crates/bevy_diegetic/src` and confirm no new production-code sites have
appeared since this plan was written.

### Phase 5 — minimal end-to-end smoke test

A new example, `crates/bevy_diegetic/examples/two_window_panels.rs`:

- Spawns two `Window`s.
- Spawns one `DiegeticPanel::screen()` per window, each pinned with
  `.window_entity(...)`, each rendering distinct text.
- Asserts visually: each window shows its own panel; closing one window
  leaves the other working.

This is the test that proves the work and the reference for downstream
consumers like `viewports_windows.rs`.

**Exit:** the example runs and shows one panel in each window.

## Test plan summary

1. Phase 0 baseline: `cargo nextest run -p bevy_diegetic` clean at
   189 / 1 skipped / 0 failed on commit `990b3f2`. The 8 at-risk
   examples are listed in Phase 0 above.
2. After each phase: `cargo nextest run -p bevy_diegetic` clean. For
   any phase that could affect rendering, launch the relevant at-risk
   example from the `main` checkout (`/Users/natemccoy/rust/bevy_hana`)
   and side-by-side against the same example built from
   `feat/multicam`. No automated visual diff tool.
3. New unit tests added in phases 2 and 3.
4. New `two_window_panels.rs` example added in phase 5.
5. Final check: `cargo build --workspace` and re-run `zoom_to_fit`
   plus a screen-panel-using example (e.g. `world_text`) to confirm
   nothing downstream broke.

## Files touched

- **Modified:**
  - `crates/bevy_diegetic/src/panel/coordinate_space.rs` — add `window` field.
  - `crates/bevy_diegetic/src/panel/builder.rs` — initializer and
    `.window()` / `.window_entity()` methods.
  - `crates/bevy_diegetic/src/screen_space/mod.rs` — resolver helper
    (with `warn_once!`), iterating positioning, new `window: Entity`
    field on `ScreenSpaceCamera` (light unchanged), per-window dedup for
    cameras, cross-window cleanup predicate for lights, explicit
    `Camera.target`, new `cleanup_screen_space_on_window_close`
    observer that despawns matching panels (camera/light cascade
    through the existing panel-remove observer).
- **New:**
  - `crates/bevy_diegetic/examples/two_window_panels.rs` — the
    end-to-end smoke test.

## Open follow-ups (out of scope for this plan)

- **`panel/gizmos.rs:54-67`** picks an arbitrary camera for the
  pixels-per-meter calculation via `.iter().next()`. Wrong in
  multi-camera worlds — gizmo line width in window B uses window A's
  ppm. Not a black-screen bug; deferred to its own change.

## Reviewer-confirmed resolutions

These were open questions in the first draft; the review settled them:

- **`WindowRef` in the enum is the right surface** (not a sidecar
  component). Keeps everything the screen-space system needs in one
  place; the destructure in `setup_screen_space_view` already pulls
  every other field out of the enum in one match.
- **Reflection is fine.** `WindowRef` derives `Reflect` in Bevy 0.18
  with `reflect(Debug, Default, Clone)`. No `#[reflect(ignore)]` needed.
- **`PrimaryWindow` timing is fine.** It's spawned in
  `WindowPlugin::build`, before any `Update` schedule, so the resolver
  always finds it in apps that include `WindowPlugin`. Headless tests
  without `WindowPlugin` are caught by the resolver's `warn_once!`.
- **`HashMap` per-frame is fine.** Window count is 1–3, panel count
  dominates. No need for `SmallVec`.
- **Light keying** changed to `(layers, window)` after review —
  necessary for correct cleanup when one window's panel is removed
  while another window's panel on the same layer survives.
- **`propagate_screen_space_render_layers` is unchanged.** It walks
  panel hierarchies, not windows.

