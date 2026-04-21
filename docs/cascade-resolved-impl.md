# Cascade / `Resolved<T>` — implementation plan

Companion to `docs/cascade-resolved.md`. That doc is the design contract. This doc is the ordered set of concrete edits: file paths, moves, and per-phase acceptance criteria.

Nothing here overrides the design doc. When this plan and the design disagree, fix this plan.

## Current-code map (pre-refactor baseline)

Types the plan deletes or replaces:

| thing | location | role |
|---|---|---|
| `UnitConfig` resource | `src/layout/units.rs:720-781` | today's global-defaults container for font/world_font/layout units |
| `TextAlphaModeDefault` resource | `src/render/transparency.rs:67-72` | today's global-default alpha mode |
| `StaleAlphaMode` marker component | `src/render/mod.rs:57-58` | synthetic invalidation marker (the hack being replaced) |
| `queue_alpha_default_refresh` system | `src/render/mod.rs:64-81` | scans on `TextAlphaModeDefault.is_changed()`, marks every text entity `StaleAlphaMode` |
| `DiegeticPanel::resolved_font_unit(&UnitConfig)` | `src/panel/diegetic_panel.rs` | inline tier-1→tier-3 resolution |
| inline `.or(...).unwrap_or(...)` resolutions | `src/render/text_renderer.rs:773-775`, `src/render/world_text.rs:225,271` | ad hoc per-attribute cascade reads |

Types the plan modifies (fields referenced by the traits):

| type | location | relevant fields / accessors |
|---|---|---|
| `DiegeticPanel` | `src/panel/diegetic_panel.rs:50-97` | `font_unit: Option<Unit>` (line 64), `text_alpha_mode: Option<AlphaMode>` (line 94) |
| `PanelTextChild` | `src/render/world_text.rs` | `alpha_mode: Option<AlphaMode>` |
| `WorldTextStyle` (alias `TextProps<ForStandalone>`) | `src/layout/text_props.rs:199,219-245` | `alpha_mode()` (line 419), `unit()` (line 305) |

Reader systems that will drop inline resolution and read `Resolved<A>`:

| system | location |
|---|---|
| `shape_panel_text_children` | `src/render/text_renderer.rs:558` |
| `render_world_text` | `src/render/world_text.rs:189` |
| panel-builder / reconcile path for font unit | `src/panel/...`, `src/render/panel_geometry.rs:110`, `src/render/panel_rtt.rs:128` |

`UnitConfig` call sites to audit when `UnitConfig` is deleted: `panel_geometry.rs:110`, `text_renderer.rs:330,441`, `panel_rtt.rs:128`, `world_text.rs:189`.

Plugin registration today:

- `DiegeticUiPlugin::build` at `src/lib.rs:200-213` — inits `UnitConfig`.
- `RenderPlugin::build` at `src/render/mod.rs:37-49` — inits `TextAlphaModeDefault`, registers `queue_alpha_default_refresh`.

Test infrastructure: `tests/` exists and is empty. No existing integration test harness to reuse; create `tests/cascade_harness.rs` fresh.

## Module layout

New module: `src/cascade/` with `mod.rs`. Sits alongside `layout/`, `panel/`, `render/`, `text/`, `screen_space/`. Internal split:

```
src/cascade/
  mod.rs              // module rustdoc (the spec surface), re-exports, Resolved<A>
  defaults.rs         // CascadeDefaults resource + should_propagate_defaults helper
  traits.rs           // CascadePanelChild, CascadePanel, CascadeEntity, CascadeTarget
  resolve.rs          // resolve_panel_child, resolve_panel, resolve_entity, resolve_target
  plugin_panel_child.rs  // CascadePanelChildPlugin + on_panel_added / on_panel_child_added / reconcile_panel_resolved / propagate_panel_to_children
  plugin_target.rs    // CascadePanelPlugin, CascadeEntityPlugin, build_cascade_target, on_cascade_target_added, propagate_global_default_to_entity
  set.rs              // CascadeSet
```

Public surface from the cascade module: `CascadeDefaults`, `CascadeSet`, the three public `Cascade*` traits, the three `Cascade*Plugin<A>` types. `Resolved<A>` is `pub(crate)`. Attribute newtypes (`PanelTextAlpha`, `WorldTextAlpha`, `PanelFontUnit`, `WorldFontUnit`) are `pub(crate)` and live in the module that defines the attribute's domain component — or, if tidy, in a single `src/cascade/attributes.rs`. Pick at implementation time; note in module rustdoc wherever they land.

## Phase 1 — framework scaffold (no behavior change)

**Goal:** the cascade module compiles, is wired through `DiegeticUiPlugin`, and round-trips a throwaway test attribute. Zero production cascades use it yet. No existing behavior removed.

Edits:

1. Create `src/cascade/mod.rs` with the module rustdoc from design §"Module-level rustdoc" (all six numbered sections). The doc is the first artifact written.
2. Create `src/cascade/set.rs` with `CascadeSet::Propagate`. Public.
3. Create `src/cascade/defaults.rs`:
   - `CascadeDefaults` struct with four fields per design §"`CascadeDefaults` resource" (`text_alpha`, `panel_font_unit`, `world_font_unit`, `layout_unit`), `Default` impl matching today's `UnitConfig::default()` + `TextAlphaModeDefault::default()`.
   - `should_propagate_defaults<A: Copy + PartialEq>(current: A, last_seen: &mut Option<A>) -> bool` per design §"Shared `should_propagate_defaults` helper".
4. Create `src/cascade/traits.rs`:
   - Public traits `CascadePanelChild`, `CascadePanel`, `CascadeEntity` per design §"Traits".
   - Private supertrait `CascadeTarget` with blanket impls for `CascadePanel` and `CascadeEntity` per design §"`CascadePanelPlugin<A>` and `CascadeEntityPlugin<A>` build".
5. Create `src/cascade/resolve.rs` with `resolve_panel_child`, `resolve_panel`, `resolve_entity`, `resolve_target` per design §"Uniform resolvers".
6. Create `src/cascade/plugin_panel_child.rs`:
   - `CascadePanelChildPlugin<A>` + `on_panel_added::<A>`, `on_panel_child_added::<A>`, `reconcile_panel_resolved::<A>`, `propagate_panel_to_children::<A>`. Code bodies follow the design exactly.
   - The ordering `.before(propagate_panel_to_children::<A>)` constraint on `reconcile_panel_resolved::<A>` (design §"Ordering and re-entrancy").
7. Create `src/cascade/plugin_target.rs`:
   - `CascadePanelPlugin<A>`, `CascadeEntityPlugin<A>`, shared `build_cascade_target::<A>`, `on_cascade_target_added::<A>`, `propagate_global_default_to_entity::<A>` with the per-entity change guard.
8. Define `Resolved<A>` in `src/cascade/mod.rs` with the full bound `A: Copy + PartialEq + Send + Sync + FromReflect + TypePath + 'static`.
9. Wire `CascadeDefaults` into `DiegeticUiPlugin::build` (`src/lib.rs:200`) — `app.init_resource::<CascadeDefaults>()`. **Keep** `UnitConfig` init and `TextAlphaModeDefault` init untouched for now; both resources continue to exist in parallel. Phase 1 does not remove anything.
10. `pub mod cascade;` in `src/lib.rs` and whatever re-exports make sense.
11. Create `tests/cascade_harness.rs` with the four helpers from design §"`tests/cascade_harness.rs`". `app()` builds a minimal `App` with `MinimalPlugins` + `DiegeticUiPlugin` (or a slimmer shim if `DiegeticUiPlugin` drags too much — evaluate at write time; prefer `MinimalPlugins` + the cascade module's plugins directly).
12. Add one integration test in `tests/cascade_smoke.rs` that defines a throwaway test attribute (e.g., `struct TestAttr(u32);` with a `CascadePanel` impl pointing at a test-only component) and exercises: spawn with no override → `Resolved<TestAttr>` equals default; mutate `CascadeDefaults` → `Resolved<TestAttr>` updates; mutate override → `Resolved<TestAttr>` reflects it. Uses the harness end-to-end.

Acceptance:

- `cargo build` clean.
- `cargo +nightly fmt` clean.
- `cargo nextest run` passes, including the smoke test.
- No production reader queries `Resolved<A>` yet; `UnitConfig`, `TextAlphaModeDefault`, `StaleAlphaMode`, `queue_alpha_default_refresh` all still present and functioning exactly as before.

Risk: medium. This phase validates the generic-plugin pattern under Bevy 0.18. Any `FromReflect`/`TypePath`/observer-registration quirks surface here before any production code migrates.

## Phase 2 — `PanelTextAlpha` migration (3-tier, the hard one)

**Goal:** replace `StaleAlphaMode`/`queue_alpha_default_refresh` with `CascadePanelChildPlugin::<PanelTextAlpha>`. The 3-tier path is exercised end-to-end on real data.

Edits:

1. Define `PanelTextAlpha` newtype + `CascadePanelChild` impl (design §"`PanelTextAlpha`").
2. Register `CascadePanelChildPlugin::<PanelTextAlpha>::default()` in the root plugin's manifest.
3. `shape_panel_text_children` at `src/render/text_renderer.rs:558`:
   - Drop the `With<StaleAlphaMode>` filter on line 568 and the `commands.entity(...).remove::<StaleAlphaMode>()` at the loop head (if present).
   - Switch filter to `Or<(Changed<Resolved<PanelTextAlpha>>, Changed<PanelTextChild>, ...existing change filters)>` as appropriate — keep existing text-content change triggers.
   - Add `Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>` to read the parent panel's cached value (the `With<DiegeticPanel>` filter is **required** per design — without it the query also matches child entities).
   - Tier-1 re-resolve: when mutating a `PanelTextChild`, call `resolve_panel_child::<PanelTextAlpha>` with `(child_over, panel_raw_override, defaults)` — or read `panel.Resolved<A>.0` as the tier-2-resolved default. Design §"Integration: tier-1 re-resolve" mandates the full chain; never shortcut to entity override alone.
   - Schedule `.after(CascadeSet::Propagate)` to see same-frame propagation. Otherwise move to `PostUpdate`.
4. Remove inline alpha resolution at `src/render/text_renderer.rs:773-775`. Replace with a read of `Resolved<PanelTextAlpha>`.
5. Delete `StaleAlphaMode` (`src/render/mod.rs:57-58`) and `queue_alpha_default_refresh` (`src/render/mod.rs:64-81`) and its registration at `src/render/mod.rs:47`.
6. `TextAlphaModeDefault` is still referenced by standalone world text in phase 2. **Keep** `TextAlphaModeDefault` until phase 3 completes, but have `CascadePanelChildPlugin::<PanelTextAlpha>`'s `global_default` read from `CascadeDefaults.text_alpha`. At phase 2 end, `TextAlphaModeDefault` is read only by `render_world_text`.

Tests added in `tests/cascade_panel_text_alpha.rs` — the full matrix from design §"Test matrix per attribute" for `PanelTextAlpha` (3-tier variant with all six rows). Each test ≤ 20 lines using the harness.

Acceptance:

- `cargo nextest run` clean.
- Launch the repo's main example; mutating `CascadeDefaults.text_alpha` at runtime via BRP changes alpha on entities with no override within one frame, leaves overridden entities alone.
- `grep StaleAlphaMode` → zero hits.

Risk: medium-high. This is the first real 3-tier path. Watch: spawn ordering (child-before-panel-flush), same-frame `Changed<Resolved<A>>` visibility across `reconcile_panel_resolved` → `propagate_panel_to_children`, reader schedule ordering vs. `CascadeSet::Propagate`.

## Phase 3 — `WorldTextAlpha` migration (2-tier entity)

**Goal:** standalone world text alpha flows through `CascadeEntityPlugin::<WorldTextAlpha>`. `TextAlphaModeDefault` disappears.

Edits:

1. Define `WorldTextAlpha` + `CascadeEntity` impl (design §"`WorldTextAlpha`").
2. Register `CascadeEntityPlugin::<WorldTextAlpha>::default()` in the root plugin.
3. `render_world_text` at `src/render/world_text.rs:189`:
   - Drop `Res<TextAlphaModeDefault>`; add `&Resolved<WorldTextAlpha>` to the query.
   - Replace inline resolution at `src/render/world_text.rs:271` (and wherever alpha is read) with the `Resolved<WorldTextAlpha>` read.
   - Tier-1 re-resolve via `resolve_entity::<WorldTextAlpha>` when `WorldTextStyle.alpha_mode()` mutates.
   - `.after(CascadeSet::Propagate)` or move to `PostUpdate`.
4. Delete `TextAlphaModeDefault` (`src/render/transparency.rs:67-72`) and its `init_resource` at `src/render/mod.rs:44`. Remove the public re-export at `src/render/mod.rs:25`.
5. Update `src/render/transparency.rs:14` docstring to reference `CascadeDefaults.text_alpha`.

Tests: `tests/cascade_world_text_alpha.rs`, 2-tier matrix rows (no tier-2 row).

Acceptance:

- `cargo nextest run` clean.
- `grep TextAlphaModeDefault` → zero hits.
- Visual smoke: a standalone `WorldText` entity's alpha tracks `CascadeDefaults.text_alpha` mutations.

Risk: low. Mechanical after phase 2.

## Phase 4 — `PanelFontUnit` migration (2-tier panel)

**Goal:** panel text font unit flows through `CascadePanelPlugin::<PanelFontUnit>`. `DiegeticPanel::resolved_font_unit` disappears. `UnitConfig.font` stops being read.

Edits:

1. Define `PanelFontUnit` + `CascadePanel` impl (design §"`PanelFontUnit`").
2. Register `CascadePanelPlugin::<PanelFontUnit>::default()` in the root plugin.
3. Delete `DiegeticPanel::resolved_font_unit(&UnitConfig)`.
4. Audit `UnitConfig` call sites using `UnitConfig.font`:
   - `src/render/text_renderer.rs:330,441` — replace with a `Res<CascadeDefaults>` + the panel's `&Resolved<PanelFontUnit>` where a panel context exists, or with the raw `CascadeDefaults.panel_font_unit` where no panel is present.
   - `src/render/panel_geometry.rs:110` — classify as "reads panel-resolved" vs. "reads global"; switch to `Resolved<PanelFontUnit>` query for the former.
   - `src/render/panel_rtt.rs:128` — same classification.
   - `UnitConfig::font_scale()`: the method fans out across the file; every call site must be classified per design §"Internal deletions" ("some read global, some read panel-resolved"). Replace usages individually; do not leave a wrapper method.
5. Keep `UnitConfig` alive for `world_font` and `layout` until phases 5 / deletion step. `UnitConfig.font` is the only field touched by this phase.

Tests: `tests/cascade_panel_font_unit.rs`, 2-tier-panel matrix.

Acceptance:

- `cargo nextest run` clean.
- `grep "resolved_font_unit\b"` → zero hits.
- `grep "UnitConfig.*\.font\b"` (excluding `world_font`, `layout`) → zero hits outside the `CascadeDefaults` compat shim (see phase 6).

Risk: medium. The call-site audit is the real work; every site must be correctly classified as "global" vs. "panel-resolved." Getting this wrong produces silently-stale rendering.

## Phase 5 — `WorldFontUnit` migration (2-tier entity)

**Goal:** standalone world text font unit flows through `CascadeEntityPlugin::<WorldFontUnit>`. `UnitConfig.world_font` stops being read.

Edits:

1. Define `WorldFontUnit` + `CascadeEntity` impl (design §"`WorldFontUnit`").
2. Register `CascadeEntityPlugin::<WorldFontUnit>::default()` in the root plugin.
3. In `render_world_text` (`src/render/world_text.rs:189`): drop `Res<UnitConfig>`; replace inline unit resolution at `src/render/world_text.rs:225` with a read of `Resolved<WorldFontUnit>`.
4. Tier-1 re-resolve path for mutations to `WorldTextStyle.unit()`.

Tests: `tests/cascade_world_font_unit.rs`.

Acceptance:

- `cargo nextest run` clean.
- `grep "UnitConfig.*world_font"` → zero hits.

Risk: low.

## Phase 6 — delete `UnitConfig`, migration docs

**Goal:** old public resources are fully gone; public surface matches design §"Breaking public-API changes".

Edits:

1. Delete `UnitConfig` struct and all its methods (`src/layout/units.rs:720-781`).
2. Delete its init at `src/lib.rs:202`.
3. Remove `pub use` re-exports — grep crate root + `src/layout/mod.rs`.
4. Ensure `CascadeDefaults.layout_unit` is read at panel construction (design §"Attribute definitions" notes `layout_unit` is **not** cascade-propagated). Audit the panel builder to make sure it reads `CascadeDefaults.layout_unit` at spawn time.
5. Update examples — grep `examples/` for `UnitConfig::`/`TextAlphaModeDefault`; apply the migration table from design §"Migration path".
6. Update `CHANGELOG.md` / release notes with the migration table verbatim.
7. Cross-reference check against design §"Success criteria":
   - [ ] `Resolved<A>` is sole source of truth for all four cascades
   - [ ] `StaleAlphaMode`, `queue_alpha_default_refresh`, `resolved_font_unit`, all inline `or(...).unwrap_or(...)` for in-scope attrs — deleted
   - [ ] `CascadeDefaults` replaces both old resources
   - [ ] Runtime mutation of any tier propagates within one frame
   - [ ] `app.register_type::<Resolved<A>>()` present for every registered cascade (grep)
   - [ ] `tests/cascade_harness.rs` exists; four attribute test files exist; tests ≤ 20 lines

Acceptance:

- `cargo nextest run` clean (including example-tests if any).
- `cargo build --examples` clean.
- `grep -E "UnitConfig|TextAlphaModeDefault|StaleAlphaMode|queue_alpha_default_refresh|resolved_font_unit"` → zero hits anywhere in `src/`, `examples/`, `tests/`.
- `cargo +nightly fmt` clean.
- Manual: launch the main example; mutate each of the four `CascadeDefaults` fields at runtime via BRP; confirm within-one-frame propagation to entities without overrides, no effect on overridden entities.

Risk: low per edit, but this is the PR that breaks downstream users. The migration table in the design doc is the entire compatibility story — no shim, no deprecation window.

## Invariants to enforce in review (lifted from design)

1. `DiegeticPanel` mutators must not incidentally touch the tier-2 override field for any `CascadePanelChild` attribute. Review rule: any setter that mutates `text_alpha_mode` as a side effect requires discussion.
2. A reader's tier-1 re-resolve always calls the full `resolve_*` helper, never shortcuts to the entity override alone. The `Some → None` transition must fall through to tier-2.
3. Every `Cascade*Plugin<A>` calls `app.register_type::<Resolved<A>>()` in `build`. Enforced at plugin-impl site; reviewers grep per cascade.
4. Spawn order: panel before child within any command batch (today's crate honors this). If a future spawn pattern batches parent+child, `on_panel_child_added` reads the panel's raw `A::PanelOverride` (not its `Resolved<A>`), which remains visible — but this invariant is documented in `src/cascade/mod.rs`.

## Rollback plan

Phases 2–5 are independent once phase 1 lands. If any migration phase reveals a blocker (e.g., Bevy 0.18 observer-ordering edge case), revert that phase's PR and keep the others. Phase 1 is revert-safe at any time because it adds without removing.
