# Headless Widgets

> **Status: IMPLEMENTATION PLAN — phased; ready for delegation.** Adds headless widgets (buttons, sliders, tooltips, focus, interactivity) to `hana_diegetic`: widgets own semantic behavior and typed events, visuals stay ordinary layout primitives, widgets reify as panel child entities targeted by Bevy picking, and anchoring comes from `hana_valence`. Phase 12 remains the required demonstration-design stop after the implementation work orders.

## Delegation Context

- **Project** — `hana_diegetic` (workspace member at `crates/hana_diegetic`). Diegetic UI layout engine for Bevy — in-world panels driven by a Clay-inspired layout algorithm. This plan adds a headless `widgets` module that reifies widgets as panel child entities.
- **Stack** — Rust (edition 2024). Bevy `0.19.0` is pinned in the root `Cargo.toml`. `thiserror` `2.0.18` is a workspace dependency and direct `hana_diegetic` dependency. `bevy_picking` + `mesh_picking` features are already enabled; widget presentation reads the all-pointer `bevy_picking::PickingInteraction` aggregate, and one diegetic picking backend owns the ordered panel+widget hit group. `bevy_enhanced_input` `0.26.0` is a workspace dependency and becomes a direct `hana_diegetic` dependency in Phase 5.5. `hana_valence` is a workspace path dependency declared in the root `Cargo.toml`. No bevy_ui.
- **Layout** (only phase-touched paths):
  - `crates/hana_diegetic/src/widgets/` — NEW module: `mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `id.rs`, `relationship.rs`, `interactivity.rs`, `focus.rs`, `input.rs`, `picking.rs`, `reify.rs`, `visual.rs`, `presets/` (`mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `style.rs`).
  - `crates/hana_diegetic/src/layout/` — `builder.rs`, `element.rs`, and the engine output that produces widget records and visual-slot references.
  - `crates/hana_diegetic/src/ime/` — `activation.rs`, `field.rs`, `ids.rs`, `mod.rs` (`ImePlugin`).
  - `crates/hana_diegetic/src/panel/` — `builder.rs`, `anchoring.rs`, `anchor_geometry.rs`, `arrangement.rs`, `diegetic_panel.rs`, `valence_provider.rs`, `perf.rs`.
  - `crates/hana_diegetic/src/render/` — `panel_geometry.rs`, the batch-record update paths used by visual overrides, and `panel_text/` (`reify.rs`, `relationship.rs`, `mod.rs`).
  - `crates/hana_diegetic/src/screen_space/anchoring/` — `candidate.rs`, `placement.rs`, `projection.rs`, `rect.rs`, `resolve.rs`, `window.rs`, `mod.rs`.
  - `crates/hana_diegetic/src/cascade/` — `attributes.rs`, `resolved.rs`, `mod.rs`; diegetic attribute defaults and typed public verbs over the shared engine.
  - `crates/bevy_kana/src/cascade.rs` — read-only shared `Cascade<T>` authoring, `CascadeFrom` relationship, propagation, and `Resolved<T>` cache.
  - `crates/hana_diegetic/src/lib.rs` — curated public re-exports.
- **Key files:**
  - `Cargo.toml` + `crates/hana_diegetic/Cargo.toml` — workspace and crate dependency declarations; Phase 0.5 added `thiserror` before widget validation expands `PanelBuildError`.
  - `src/layout/builder.rs` + `src/layout/element.rs` — `El`, `CommonEl`, `Element`, `LayoutTree`, exhaustive tree-change classification, element-id traversal, clipping, precomposition, and the actual homes of `.button`/`.slider` authoring data.
  - `src/panel/builder.rs` — panel builder and the `thiserror`-derived `PanelBuildError`; `build()` calls `tree.duplicate_named_element_id()`. Widget ids reuse this validation path, while runtime tree replacement calls the same validator before accepting a tree.
  - `src/render/panel_text/reify.rs` — text reify: `reify_text_entities` (`:179`) and `update_reused_panel_text_child` (`:417`, the reuse-on-diff pattern widget reify mirrors).
  - `src/render/panel_text/relationship.rs` — `TextRunOf` / `PanelTextRuns` (template for `WidgetOf`/`PanelWidgets`; no `linked_spawn`).
  - `src/render/panel_text/mod.rs` — text-child ordering in `PanelChildSystems::Build`; widget semantic reify does **not** copy that `PostUpdate` schedule because screen attachment resolution needs widget entities and rects during `Update`.
  - `src/ime/activation.rs` — IME double-click activation observer: `On<Pointer<Click>>` gated `click.count < 2` (`:28`); calls `computed.field_at_local_position(panel_local)` (`:39`).
  - `src/panel/diegetic_panel.rs` — `field_at_local_position(&self, panel_local: Vec2) -> Option<&PanelFieldRecord>`; panel-local record-lookup pattern for the picking backend.
  - `src/ime/ids.rs` — id types and `PanelElementId::auto`. Widget ids land in this element-id namespace; no new `WidgetId` newtype.
  - `src/ime/mod.rs` — `ImePlugin` (`pub(crate)` `:70`, `impl Plugin` `:89`); mirror for `WidgetsPlugin`.
  - `crates/bevy_kana/src/cascade.rs` — `Cascade<A>`, `CascadeFrom` / `CascadeChildren`, `CascadeDefault<A>`, `CascadePlugin<A>`, `Resolved<A>`, and `CascadeSet::Propagate`. `ChildOf` is deliberately unrelated to cascade inheritance.
  - `src/cascade/mod.rs`, `attributes.rs`, and `resolved.rs` — private shared-engine imports plus diegetic attribute root defaults, typed `override_*` / `inherit_*` commands, and resolved readers. `hana_diegetic` does not re-export raw `Cascade<T>`.
  - `src/panel/anchor_geometry.rs` — read-only panel geometry API: `PanelAnchorGeometryParam`, `PanelScreenBounds`, `PanelPlane`, and `ResolvedPanelAnchorGeometry`. `src/panel/valence_provider.rs` is the world-panel provider for the `hana_valence::ResolvedAnchorGeometry` component that widgets also publish (see `docs/hana_valence/as-built/anchoring-and-arrangements.md`).
  - `src/panel/anchoring.rs` — insert-only `AnchoredToPanel` authoring, private `PanelAttachmentAuthored`, world-only lowering to `hana_valence::AnchoredTo`, offset lowering, and `PanelSpace` reconciliation. Screen panels keep the shared authoring without the world relation.
  - `src/render/panel_geometry.rs` — current flat `PanelInteractionMesh`; Phase 3 moves it out of the generic mesh backend and makes the diegetic backend emit the panel and widget hits together.
  - `src/screen_space/anchoring/candidate.rs` + `resolve.rs` — screen placement builds candidates from private `PanelAttachmentAuthored` and delegates ordering and diagnostics to `hana_valence::resolve_attachments`; it accepts panel targets only today, and Phase 4.5 teaches it widget targets.
  - `src/panel/perf.rs` + `src/panel/constants.rs` — `DiegeticPerfStats` (`perf.rs:45`), `pub reify_ms: f32` (`perf.rs:54`), and `DIAG_PANEL_REIFY_MS` (`constants.rs:35`, published at `perf.rs:258`).
  - `src/render/mod.rs` — `PanelChildSystems` set enum (`:128`); `TextRunOf`/`PanelTextRuns` re-exports.
  - `src/lib.rs` — curated re-exports, including `PanelBuildError`; widget public types re-export here.
- **Build:** `cargo build && cargo +nightly fmt` after changes.
- **Test:** `cargo nextest run` (never `cargo test`).
- **Lint:** the `clippy` skill. Workspace lints are strict: `all`/`cargo`/`nursery`/`pedantic` denied, `unwrap_used`/`expect_used`/`panic`/`unreachable` denied, `missing_docs = "deny"`, `self_named_module_files` denied (use `module/mod.rs` directory form).
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/hana_diegetic_widgets`
- **Invariants:**
  - **Valence gate:** `hana_valence` exists at `crates/hana_valence`; its resolver, panel bridge, and screen-adapter integration are described in `docs/hana_valence/as-built/anchoring-and-arrangements.md`. Hana Valence types stay out of diegetic's public widget signatures. Diegetic authoring lowers to `hana_valence::AnchoredTo` only for world sources; screen sources retain `PanelAttachmentAuthored` and use the shared attachment graph without carrying the world relation.
  - No bevy_ui / bevy_a11y dependency. `WidgetDisabled`, `WidgetFocused`, and pointer-capture state stay bespoke; `PickingInteraction` supplies all-pointer hover/press presentation and `bevy_enhanced_input` supplies the opt-in semantic-action adapter.
  - Widgets reify as panel child entities under `ChildOf(panel)`; the `WidgetOf`/`PanelWidgets` relationship is a traversal index only, no `linked_spawn` — `ChildOf` owns despawn.
  - Behavior modules never construct layout/render primitives (`El`, `LayoutTree`, `PanelDraw`, materials, `TextStyle`, `DrawZIndex`). Presets depend on behavior, never the reverse.
  - No relayout on hover/press/focus/disabled/value flips. Presets author stable private visual-slot ids; changed widget state patches only those slots' retained batch records through widget-owned override components. It never regenerates the `LayoutTree` or writes `DiegeticPanel`/`ComputedDiegeticPanel` merely to restyle a widget.
  - Widget semantic reify runs in `Update`, after `PanelSystems::ComputeLayout` and before `PanelSystems::ResolvePanelAttachments`, with an explicit `ApplyDeferred` fence. Existing cascade propagation remains before layout so inherited layout attributes update geometry in the same frame. From Phase 2 onward, Bevy Kana's insertion observer seeds newly reified widget cascade state during the reify fence; later widget resolvers run after that fence. Render-child batching remains in `PostUpdate`.
  - Change-gated systems, never unconditional per-frame walks: reify is gated on `Changed<ComputedDiegeticPanel>` and reuses entities by id; interactivity writes `WidgetDisabled` only on diff; anchor geometry exists only while world or screen demand is nonempty and is removed after the last demand ends.
  - Widget ids reuse `PanelElementId` and its `duplicate_named_element_id` → `DuplicateElementId` validation; event-emitting widgets require `Named` ids (auto ids reposition on structural edits and would fire spurious cancels).
  - Widget interactivity is one logical cascade across both storage domains: global/default and panel/explicit ancestors are ECS participants, while parent/child `El` authoring is folded during layout traversal. The folded layout-tree `Cascade<WidgetInteractivity>` is reified onto the widget entity, whose explicit `CascadeFrom(panel)` lets Bevy Kana produce the final `Resolved<WidgetInteractivity>`. The `LayoutTree` remains authoritative for the reified widget component, matching panel-text style reification; no second virtual-layout component or custom precedence resolver exists.
  - Widget events derive `EntityEvent` targeting the widget entity; the panel-local id is a payload convenience only, never the routing key. Owning panel resolves through `WidgetOf`, never duplicated on components or events.
  - Exported `hana_diegetic` error types derive `thiserror::Error`, declare messages beside their variants, and have exhaustive stable-message tests. Converting sources and intentionally lossy normalization mappings stay explicit.
  - Widget picking geometry stays in **panel-local space**. The first implementation uses the current flat interaction-mesh hit conversion. Curved-panel support is gated on Phase 5 of `surface-panels.md`, which replaces that one boundary with `PanelSurface::project()`; widget rectangle tests remain unchanged and never place geometry independently in world space.
  - The first API rejects interactive descendants inside a widget and widgets inside precomposed subtrees. Arbitrary non-interactive child layout remains valid; nested/precomposed interaction needs a later ownership and hit-order design.
  - Tooltip authoring is separate from `Button`/`Slider` authoring. Reify creates a lightweight tooltip entity with `TooltipFor(target)`; first eligibility materializes that same entity into a hidden anchored panel. The semantic relationship exists before the placement relationship and does not itself create anchor-geometry demand.
- **Public contract ledger (fixed before delegation except where a phase-local pending-decision block names the unresolved surface):**
  - Authoring methods are `El::button(self, id: impl Into<PanelElementId>, button: Button) -> Self` and `El::slider(self, id: impl Into<PanelElementId>, slider: Slider) -> Self`. Both assign the element id and crate-private widget variant atomically. `Button` is a private-field `Clone + Debug + PartialEq + Default` authoring builder with `new()` and Phase 7's `on_click(...)`; `Slider` is a private-field `Clone + Debug + PartialEq` validated authoring builder with no `Default` because range and initial value are required. Neither is an ECS component. Crate-private `WidgetSpec` is exactly `Button(Button) | Slider(Slider)`, while runtime slider data lives in `SliderState`, so no public `Spec` suffix is needed.
  - New validation variants are `PanelBuildError::WidgetRequiresNamedId(PanelElementId)`, `WidgetContainsInteractiveDescendant(PanelElementId)`, and `WidgetInsidePrecomposedSubtree(PanelElementId)`. Phase 1 adds them to the `thiserror::Error` enum established in Phase 0.5, with direct `#[error(...)]` messages and stable-message tests.
  - Runtime tree replacement has one public path: `DiegeticPanelCommands::set_tree(&mut self, entity: Entity, tree: LayoutTree) -> Result<(), PanelBuildError>`. It validates synchronously with the same validator as panel construction and queues the deferred replacement only for a valid tree; rejection queues nothing and preserves the current tree. `Ok(())` means validation succeeded and the replacement was queued, not that the deferred command later found a live panel entity. There is no `try_set_tree` companion; Phase 1 migrates every internal caller to handle the result explicitly.
  - Identity exports are `PanelWidget`, `PanelWidgetReader`, `WidgetOf`, and `PanelWidgets`. `PanelWidget` exposes only `id()`, and `WidgetOf` exposes only `panel()`; relationship mutation remains internal. `PanelWidgetReader` is the read-only `SystemParam` bridge from an authored `(panel, PanelElementId)` to the live reified widget entity for app-initiated entity-targeted control. Entity events already carry their widget target and never require this lookup.
  - Interactivity/focus exports are `WidgetInteractivity`, `WidgetDisabled`, `PanelWidgetWriter`, `WidgetFocusable`, `WidgetFocused`, `RequestWidgetFocus`, `ClearWidgetFocus`, `WidgetFocusChanged`, and `WidgetFocusChangeCause`. Element authoring is `El::widget_interactivity(self, value: WidgetInteractivity) -> Self`. `PanelWidgetWriter::override_interactivity(widget, value)` and `inherit_interactivity(widget)` update the authoritative widget `El` by following `PanelWidget` + `WidgetOf`; an event target can be passed directly, while `(panel, id)` callers resolve it through `PanelWidgetReader`. The existing `CascadeEntityCommandsExt` gains `override_widget_interactivity(value)` and `inherit_widget_interactivity()` for panels and other ECS-authored cascade ancestors; raw `Cascade<T>` remains owned by `bevy_kana` and is not re-exported. The focus request payload is `{ window, widget }`, clear is `{ window }`, and change is `{ window, previous, current, cause }`. Cause variants are `Pointer`, `Traversal`, `Semantic`, `Application`, `ExplicitClear`, `WidgetRemoved`, `FocusabilityRemoved`, and `ScopeLost`; disable is intentionally absent.
  - The six exported semantic action names are `FocusNextWidget`, `FocusPreviousWidget`, `FocusFirstWidget`, `FocusLastWidget`, `ActivateFocusedWidget`, and `CancelFocusedWidget`; Phase 5's pending decision fixes their library-independent payload/routing shape. The adapter exports `WidgetInputPlugin`, `WidgetInputBindings`, and `WidgetControlSummary`; Phase 5.5's pending decision fixes its runtime settings/rebind/disable surface. Initial installation remains `app.add_plugins(WidgetInputPlugin::new(WidgetInputBindings::default()))`, after which the plugin owns one context entity per window.
  - Button exports are `Button`, `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, `ButtonCanceled`, `ButtonCancelCause`, `ButtonPreset`, and `ButtonStyle`. Every event has `entity` as its `#[event_target]` plus `id: PanelElementId`; pressed/released add `pointer_id: PointerId`, clicked adds `pointer_id: Option<PointerId>` (`None` for semantic activation), and canceled adds `pointer_id: PointerId` plus `cause`. `ButtonCancelCause` is exactly `PointerCanceled | PointerRemoved | CaptureLost | Disabled | WidgetRemoved | WidgetKindChanged | Explicit`.
  - Slider exports are `Slider`, `SliderState`, `SliderRange`, `SliderStep`, `SliderDirection`, `SliderConfigError`, `SliderGrabbed`, `SliderChangeRequested`, `SliderReleased`, `SliderCanceled`, `SliderCancelCause`, `RequestSliderAdjustment`, `SliderAdjustment`, `slider_self_update`, `SliderPreset`, and `SliderStyle`. Phase 10 appends `TooltipTemplate`, `TooltipFor`, `Tooltips`, and `TooltipDisabledPolicy`; Phase 10.5 appends `TooltipShown`, `TooltipHidden`, and `TooltipPreset`.
  - Tooltip construction is `El::tooltip(self, template: TooltipTemplate) -> Self` for associated authoring and `commands.spawn((template, TooltipFor::new(target)))` for standalone authoring. `TooltipFor::new(target)`, `target()`, and `retargeted(target)` are public; `Tooltips::iter()` exposes reverse membership; mutation is maintained by Bevy relationship hooks; and `Tooltips` uses `linked_spawn` to despawn related tooltip controllers with their target.
  - `WidgetsPlugin`, `WidgetSpec`, `WidgetKind`, computed records, id/order maps, callback templates/handles, capture/terminal state, visual-slot ids/overrides, anchor bridges/geometry, tooltip phases/timers, and screen dependency relations remain crate-private. Raw `Cascade<T>` / `Resolved<T>` storage remains `bevy_kana` machinery rather than widget API.

## Phases

### Phase 0 — `reify` terminology rename  · status: done (`707b9c3a`)

#### Work Order

**Goal:** Rename entity-creation-from-computed-output terminology from `reconcile`/`materialize` to `reify` everywhere the concept is shared.

**Spec:**
- Rename the system `reconcile_panel_text_children` → `reify_text_entities` in `src/render/panel_text/reconcile.rs`; rename the file to `reify.rs` and update `mod.rs` accordingly.
- Rename `DiegeticPerfStats::reconcile_ms` → `reify_ms` (`src/panel/perf.rs:54`) — this field is crate-public, so update every reader. Rename the `DIAG_PANEL_RECONCILE_MS` diagnostic (`src/panel/perf.rs:258`) and its string path to the `reify` spelling.
- Update doc comments and panel-text test names that use `reconcile` in this entity-creation sense. Do not touch unrelated uses of the word.

**Files:**
- `src/render/panel_text/reconcile.rs` → `reify.rs` — system + helper renames
- `src/render/panel_text/mod.rs` — module decl, ordering references
- `src/panel/perf.rs` — field + diagnostic rename
- any test files referencing the renamed items (find with `rg -n "reconcile" crates/hana_diegetic`)

**Constraints from prior phases:** None. Note: this phase is independent of the valence gate; the gate blocks Phase 1, not this rename.

**Acceptance gate:** `cargo build && cargo +nightly fmt` clean; `cargo nextest run` green; `rg -n "reconcile" crates/hana_diegetic` shows no remaining uses in the entity-creation sense.

#### Retrospective

**What worked:**
- `panel_text/reify.rs`, `reify_text_entities`, `DiegeticPerfStats::reify_ms`, and the `panel/reify_ms` diagnostic landed as a behavior-preserving terminology change.
- Both reviews found no issues; 1,220 tests passed with 4 skipped, and `diegetic_text_stress` exercised sustained text reification plus the renamed timing reader.

**Implications for remaining phases:**
- Phase 1 can use `src/render/panel_text/reify.rs` and `reify_text_entities` as the shipped entity-reuse template.
- All future performance readers use `DiegeticPerfStats::reify_ms`; unrelated anchoring and render-batch reconciliation terminology remains unchanged.

#### Phase 0 Review

- Updated the shared Delegation Context to the shipped text-reify module, system, performance field, diagnostic, and line references.
- Updated computed-widget entity-creation wording in Phases 1 and 5 to `reify`; tooltip panel materialization retains its distinct term.
- Reviewed Phases 1–12; no remaining phase needs scope, ordering, file, constraint, or acceptance-gate changes because of Phase 0.

### Phase 0.5 — Adopt `thiserror` for diegetic errors  · status: done (`25ce5f2f`)

#### Work Order

**Goal:** Put `hana_diegetic` error declarations on the same `thiserror` convention as `hana_catalyst` before widget validation expands `PanelBuildError`.

**Spec:**
- Add `thiserror = "2.0.18"` to the root workspace dependencies and `thiserror.workspace = true` to `crates/hana_diegetic/Cargo.toml`. Let Cargo update `Cargo.lock` if the resolved lockfile changes.
- Convert these seven public error types from handwritten `Display` / `Error` implementations to `#[derive(thiserror::Error, ...)]`: `PanelBuildError`, `PanelAnchorGeometryError`, `PanelProjectionError`, `FontLoaderError`, `OutlineError`, `InvalidSize`, and `InvalidPanelScalar`. Preserve their existing non-error derives and every current display string exactly.
- Use direct `#[error("...")]` declarations for leaf variants and value-bearing messages. `FontLoaderError::Io` remains a converting source with the existing `failed to read font file: {error}` message. Make `PanelBuildError::InvalidSize` a transparent `#[from]` wrapper so callers retain the current display text and can now discover the underlying `InvalidSize` through `Error::source`; this source-chain addition is the phase's only intentional behavior change.
- Keep normalization conversions handwritten where the source error is deliberately collapsed or mapped: `ComputeGlobalTransformError` → `PanelAnchorGeometryError`, `ComputeGlobalTransformError` → `PanelProjectionError`, and `PanelAnchorGeometryError` → `PanelProjectionError`. Do not annotate those conversions as sources or carry their original values.
- Remove only imports and handwritten implementations made obsolete by the derives. Leave the crate-private `MaterialSlotIdError` unchanged: it is an internal validation enum, not part of this public error convention.
- Add focused unit tests following the `hana_catalyst` pattern: exhaustively pin enum variant messages, pin value-bearing struct/variant messages, verify the I/O and invalid-size conversion/source chains, and verify the lossy transform/anchor mappings still produce the same normalized variants.

**Files:**
- `Cargo.toml`, `Cargo.lock`, `crates/hana_diegetic/Cargo.toml` — dependency declaration and resolution
- `crates/hana_diegetic/src/panel/builder.rs` — `PanelBuildError`
- `crates/hana_diegetic/src/panel/anchor_geometry.rs` — `PanelAnchorGeometryError`
- `crates/hana_diegetic/src/panel/conversion/error.rs` — `PanelProjectionError`
- `crates/hana_diegetic/src/text/font/loader.rs` — `FontLoaderError`
- `crates/hana_diegetic/src/text/slug/glyph/outline.rs` — `OutlineError`
- `crates/hana_diegetic/src/layout/units/invalid_size.rs` — `InvalidSize`
- `crates/hana_diegetic/src/layout/line.rs` — `InvalidPanelScalar`
- Read-only reference: `../hana_tool_graph/crates/hana_catalyst/src/error.rs`

**Constraints from prior phases:** Phase 0's reify rename is complete and does not overlap this migration. Do not add widget-specific errors yet; Phase 1 owns those variants and their validation behavior.

**Acceptance gate:** `cargo build && cargo +nightly fmt` clean; `cargo nextest run` green; full workspace clippy green through the `clippy` skill; all seven public error types derive `thiserror::Error` with stable-message coverage; `FontLoaderError::Io` and `PanelBuildError::InvalidSize` expose their wrapped sources; normalized transform/anchor conversions remain unchanged; `MaterialSlotIdError` remains internal and untouched. Live smoke the `font_loading` example and shut it down through BRP without a panic or fatal error.

#### Retrospective

**What worked:**
- All seven public errors moved to `thiserror::Error` with their existing messages pinned by tests; converting sources and lossy normalization mappings remained explicit.
- Build, nightly formatting, 1,232 tests, full workspace clippy, rustdoc, and the BRP-driven `font_loading` smoke all passed.

**What deviated from the plan:**
- `PanelBuildError::InvalidSize` uses `#[error("{0}")] InvalidSize(#[from] InvalidSize)` rather than `#[error(transparent)]` so `Error::source` returns the wrapped leaf error while display output remains unchanged.

**Surprises:**
- A `thiserror` transparent wrapper forwards to the wrapped error's own source; because `InvalidSize` is a leaf, that would produce no source instead of exposing `InvalidSize`.

**Implications for remaining phases:**
- Phase 1 extends the shipped `thiserror`-derived `PanelBuildError` with direct `#[error(...)]` declarations and stable-message tests; it must not restore handwritten formatting or error implementations.

#### Phase 0.5 Review

- Updated the shared Delegation Context to the shipped `thiserror` dependency and stable-message convention, and corrected drifted symbol/document references.
- Phase 1 now owns both button and slider authoring/configuration, matching its existing computed-record and acceptance contracts; Phases 6 and 8 extend those modules with runtime behavior and lifecycle finalization.
- Phase 8 now consumes the Phase 1 slider construction and `SliderConfigError` contract instead of redefining it.
- Resolved the runtime tree-replacement error channel as one fallible `set_tree` API. The crate has not been published, so Phase 1 updates internal callers directly rather than carrying a compatibility `try_set_tree`/`set_tree` pair.
- Reviewed Phases 2–5, 7, and 9–12; Phase 0.5 does not otherwise change their scope, ordering, files, constraints, or acceptance gates.

### Phase 1 — Widget identity, authoring, relationship, reify, plugin skeleton  · status: done (`0b7d1b8a`)

#### Work Order

**Goal:** Widgets can be authored in a panel's element tree, represented in computed output, and reify as reused, relationship-indexed panel child entities with a stable id lookup.

**Precondition (verify before starting):** the shipped Hana Valence resolver,
world-panel provider, `AnchoredToPanel` lowering, and screen attachment adapter
still match the Valence gate invariant. If that contract has changed, reconcile
this plan before implementing widgets.

**Spec:**
- **Ids and validation** (`widgets/id.rs` + `layout/element.rs`): widget ids ARE `PanelElementId` — no newtype. Event-emitting widgets require `Named` ids; the exact `PanelBuildError` variants are fixed in the public contract ledger. Duplicate rejection reuses `duplicate_named_element_id` → `PanelBuildError::DuplicateElementId`. A single tree validator runs from both `DiegeticPanelBuilder::build` and runtime tree replacement, and also rejects interactive descendants of widgets and widgets under `PrecomposeMode`. Add direct `#[error(...)]` declarations and exhaustive message tests for all three new variants; update `PanelBuildError` and both public panel-builder `# Errors` sections.
- **Runtime tree replacement** (`panel/diegetic_panel.rs`): change the existing method to `DiegeticPanelCommands::set_tree(&mut self, entity: Entity, tree: LayoutTree) -> Result<(), PanelBuildError>`. Validate synchronously with the shared tree validator, queue the existing deferred replacement only on success, and return the typed validation error without replacing or otherwise mutating the current tree on failure. Document that `Ok(())` covers tree validation and queueing only because `Commands` cannot synchronously guarantee that the entity still exists when the deferred command applies. Do not add `try_set_tree` or a second logging policy. Migrate every internal workspace caller: application, example, and benchmark paths handle and report rejection through their existing error/logging conventions; tests assert success or inspect the exact error without introducing lint-denied `expect`/`unwrap` calls.
- **Authoring builders** (`widgets/button.rs` + `widgets/slider.rs`): create `Button` as the private-field `Clone + Debug + PartialEq + Default` builder with `new()`. Create the Phase 1 portion of the approved slider contract: `SliderDirection::{LeftToRight, RightToLeft, BottomToTop, TopToBottom}`; `SliderRange::new(start, end) -> Result<SliderRange, SliderConfigError>` for finite strictly ordered endpoints; `SliderStep::new(step) -> Result<SliderStep, SliderConfigError>` for finite positive steps; and `Slider::new(range, initial_value) -> Result<Slider, SliderConfigError>` with private-field `step(SliderStep)` and `direction(SliderDirection)` builders. `SliderConfigError` is exactly `NonFiniteRange | UnorderedRange | NonFiniteValue | NonPositiveStep`, derives `thiserror::Error`, and has exhaustive stable-message tests. These are authoring/configuration types, not ECS components; Phase 8 adds `SliderState`, requests, events, and runtime behavior.
- **Element authoring** (`layout/builder.rs` + `layout/element.rs`): implement both exact public-contract methods, `El::button` and `El::slider`. They are config methods mirroring `.editable_field(id, spec)`, not `LayoutBuilder` leaves. `CommonEl::widget: Option<WidgetSpec>` is carried onto `Element` parallel to `editable`, through every constructor/clone/destructure path; crate-private `WidgetSpec` is exactly `Button(Button) | Slider(Slider)`. `LayoutTree::classify_change` treats a widget-record-only edit as `VisualOnly`, and the visual-only commit path refreshes computed widget records. Phase 10 adds a separate tooltip declaration field; it is never nested inside `WidgetSpec`.
- **Computed record:** layout output owns one crate-private `ComputedWidgetRecord` per valid widget: `PanelElementId`, `WidgetKind`, current computed-tree preorder, and the authored `Button` or `Slider` snapshot. Phase 2 added the folded interactivity cascade; Phase 3 adds the panel-local/clipped rect and interaction rank; Phase 7.5 adds visual-slot references. `ComputedDiegeticPanel` exposes the crate-private record slice consumed by reify even on a visual-only tree update.
- **Identity and lookup:** each entity carries public read-only `PanelWidget { id: PanelElementId }` plus `WidgetOf(panel)`. `PanelWidgets` remains the Bevy-maintained membership set. Public read-only `SystemParam` `PanelWidgetReader` exposes `entity(&self, panel: Entity, id: &PanelElementId) -> Option<Entity>` over a private panel-local map rebuilt during reify, and validates that the mapped entity is still a live `PanelWidget` with `WidgetOf(panel)` before returning it. This bridges author-time identity to runtime ECS identity when app code starts with `(panel, id)` but needs an entity for Phase 2's tree-backed `PanelWidgetWriter`, a focus or slider request, an entity-scoped observer/effect, or a standalone tooltip. Identical ids on different panels resolve independently; missing, not-yet-reified, removed, or stale entries return `None`. Widget entity events already supply their target and do not use the reader.
- **Relationship** (`widgets/relationship.rs`): `WidgetOf` / `PanelWidgets`, modeled on `TextRunOf`/`PanelTextRuns` (`src/render/panel_text/relationship.rs`). No `linked_spawn` — widgets sit under `ChildOf(panel)`, which owns despawn; the relationship is a membership index, not the focus-order source. Phase 2 separately inserts `CascadeFrom(panel)` because `ChildOf` and `WidgetOf` do not imply cascade inheritance.
- **Reify** (`widgets/reify.rs`): a change-gated system walking `Changed<ComputedDiegeticPanel>`. It reuses entities by panel-local id, writes components only on diff, rebuilds the id map and current preorder, and sweeps every unvisited entity. Same-id/same-kind updates preserve Phase 1 state; a kind change retains entity identity while replacing `WidgetKind` and the complete Phase 1-owned authored snapshot without leaving stale components. Widget removal despawns the reified entity. Phases 6 and 8 extend these kind-change/removal paths with lifecycle finalization once button and slider behavior exists.
- **Schedule and plugin** (`widgets/mod.rs`): `WidgetsPlugin` (`pub(crate)`, mirror `ImePlugin`) defines `WidgetSystems::Reify` in `Update`, after `PanelSystems::ComputeLayout` and before `PanelSystems::ResolvePanelAttachments`. Preserve the existing `CascadeSet::Propagate` before `PanelSystems::ComputeLayout`; layout consumes `Resolved<FontUnit>` and must observe inherited/default changes in the same frame. An explicit `ApplyDeferred` fence after reify makes new widget entities visible before later widget resolvers and attachment work. Phase 2's inserted `Cascade` state is seeded during that fence by Bevy Kana's insertion observer, so reify does not need to precede the scheduled propagation pass. Do not put semantic widget reify in `PanelChildSystems::Build`; that `PostUpdate` timing is too late for same-frame screen targets. Register the plugin where `ImePlugin` is registered.
- **Module structure:** private `widgets` module next to `ime`; curated public types re-exported from `lib.rs`/`widgets/mod.rs`, never the whole tree.

**Files:**
- `src/widgets/mod.rs`, `src/widgets/id.rs`, `src/widgets/relationship.rs`, `src/widgets/reify.rs` — new
- `src/widgets/button.rs`, `src/widgets/slider.rs` — new authoring/configuration builders; no behavior yet
- `src/layout/builder.rs`, `src/layout/element.rs` — `.button(...)`, `.slider(...)`, `common.widget`, validation, tree diffing
- `src/layout/engine/` + `src/panel/compute_layout.rs` — computed widget records and visual-only refresh
- `src/panel/diegetic_panel.rs` — fallible `set_tree`, shared validation, rejection docs, and focused tests
- `src/panel/builder.rs` — extend the `thiserror`-derived `PanelBuildError` and add the shared validation call
- `src/lib.rs` — re-exports + plugin registration site
- every Rust caller of `.set_tree(...)` under `crates/` — explicit result handling; no ignored fallible replacements
- Read-only templates: `src/render/panel_text/relationship.rs`, `src/render/panel_text/reify.rs`, `src/ime/mod.rs`

**Constraints from prior phases:** Phase 0 renamed `reconcile_panel_text_children` → `reify_text_entities` and `reconcile.rs` → `reify.rs`. Phase 0.5 added the direct `thiserror` dependency and migrated `PanelBuildError`; add the three widget-validation variants with `#[error(...)]` declarations and stable-message tests rather than restoring handwritten formatting/error implementations.

**Acceptance gate:** `cargo nextest run` green with new tests: duplicate widget id rejected via `DuplicateElementId`; auto id, nested interactive content, and precomposed widgets rejected by typed errors at panel build and `set_tree`; invalid `set_tree` returns the exact typed error, queues no replacement, and preserves the current tree; valid `set_tree` returns `Ok(())` and its deferred replacement applies; an identical valid replacement preserves widget identity and lookup; all internal callers handle the result explicitly and no `try_set_tree` API exists; all new error messages are pinned; invalid slider range/value/step construction is rejected; visual-only `Button`/`Slider` authoring changes refresh computed records; reify creates `PanelWidget` entities under `ChildOf(panel)` with relationship and id lookup; `PanelWidgetReader` resolves the same id independently on two panels and returns `None` for missing, not-yet-reified, removed, or stale entities; structural reorder keeps entities but rebuilds current preorder; removing one widget sweeps it while the panel survives; same-id kind replacement retains the entity and swaps every Phase 1-owned kind/authored component without stale state; panel despawn drops all widgets without double-despawn; inherited/default font-unit changes still reach layout in the same update and the `text_cascade` example initializes without a schedule cycle.

#### Retrospective

**What worked:**
- `Button`/`Slider` authoring, shared tree validation, fallible `set_tree`, computed widget records, relationship-indexed reification, and `PanelWidgetReader` landed as one coherent identity path.
- The full workspace suite passed with 1,255 tests and 4 skips; both `text_cascade` and a temporary public-API widget app completed live smoke checks.

**What deviated from the plan:**
- The first implementation ordered layout before cascade propagation; review restored propagation before layout and kept widget reify after layout with its own deferred-command fence.
- Identical valid tree replacement now advances the tree revision while preserving text and widget lookup maps, rather than clearing indexes that unchanged computed output would not rebuild.

**Surprises:**
- Reversing cascade propagation and layout created a schedule cycle through the font-unit refresh systems, not only a one-frame stale-value risk; `text_cascade` exposed it immediately.
- An identical replacement can leave `ComputedDiegeticPanel` unchanged, so clearing retained lookup maps without a corresponding reify trigger makes live entities unreachable.

**Implications for remaining phases:**
- Phase 2 must retain propagation → layout → reify → deferred-command fence → interactivity resolution; Bevy Kana's insertion observer seeds cascade state for newly reified widgets during the fence.
- Later phases must preserve the Phase 1 entity and lookup indexes unless they also guarantee a reify-triggering computed change.

#### Phase 1 Review

- Phase 2 now names the post-reify command fence and owns interactivity-only tree-change classification plus lookup-preserving replacement coverage.
- Phase 3 now extends the actual Phase 1 computed-record construction and regeneration paths; Phase 4.5 correctly attributes identity to Phase 1 and geometry to Phase 3.
- Phase 8 now constructs first-spawn `SliderState` through the approved snap-then-clamp path and tests out-of-range and off-step authored values.
- Phase 10 now orders associated tooltip reify after the named fence and proves exact-tree replacement preserves its controller and indexes.
- Phases 4–7, 9, and 12 remain dispatch-ready without further changes.

### Phase 2 — Interactivity resolution  · status: done (`a65bd872`)

#### Work Order

**Goal:** Enabled/disabled resolves across global, panel, layout-subtree, and widget levels into a `WidgetDisabled` marker on widget entities.

**Spec:**
- Effective enum (`widgets/interactivity.rs`):
  ```rust
  #[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
  pub enum WidgetInteractivity {
      Enabled,
      Disabled,
  }
  ```
  `WidgetInteractivity` has no `Inherit` variant. Authored state uses `bevy_kana::Cascade<WidgetInteractivity>` in both ordinary layout structs and ECS components: `Cascade::Inherit` continues to the logical parent, while an absent ECS component means non-participation. `WidgetDisabled(())` is the final derived presence marker with a private field, queried through `Has<WidgetDisabled>` and not constructible by callers. No `ResolvedWidgetInteractivity`, `enabled: bool`, disabled-reason type, or separate layout-interactivity value exists.
- **One logical cascade:** root-to-leaf precedence is global default → explicit ECS ancestors / owning panel → parent layout Els → child layout Els → the widget El. `CommonEl` stores `Cascade<WidgetInteractivity>`, defaulting to `Inherit`; `El::widget_interactivity(value)` authors `Override(value)`. The layout walk carries the nearest authored override and writes one folded `Cascade<WidgetInteractivity>` into `ComputedWidgetRecord`: `Override(value)` when any enclosing/widget El supplies one, otherwise `Inherit`. A child `Enabled` override inside a disabled parent is enabled; sticky ancestor disabling is rejected. Layout elements remain ordinary tree data and never become private cascade-scope entities.
- **Tree-to-ECS authoring bridge:** `WidgetsPlugin` installs `CascadePlugin::new(WidgetInteractivity::Enabled)`. Reify synchronizes the computed record's folded `Cascade<WidgetInteractivity>` onto the widget entity and keeps `CascadeFrom::new(panel)` synchronized independently of `ChildOf` and `WidgetOf`. This mirrors panel-text reification: the `LayoutTree` is authoritative and the reified component is derived output, updated only when the folded authored value differs. Bevy Kana's normal `Resolved<WidgetInteractivity>` is therefore final: a folded layout override stops there, while `Inherit` follows the panel, explicit ECS ancestors, and `CascadeDefault`. Do not add `WidgetLayoutInteractivity`, a second runtime-authoring layer, or a custom precedence resolver.
- **Tree-backed widget mutation:** add public read-write `SystemParam` `PanelWidgetWriter`, the mutation counterpart to `PanelWidgetReader`. `override_interactivity(&mut self, widget: Entity, value: WidgetInteractivity) -> bool` and `inherit_interactivity(&mut self, widget: Entity) -> bool` validate the live `PanelWidget` / `WidgetOf`, locate its named widget El in the owning panel's authoritative tree, and update that El's local `Cascade`. `inherit_interactivity` reveals the nearest parent-El override, then the panel/global result when no layout override remains. `false` means the live widget, owner panel, or authored source could not be resolved; an unchanged successful write returns `true` without dirtying the panel. An event target can be passed directly; code starting from `(panel, id)` resolves the entity through `PanelWidgetReader` first. The existing `CascadeEntityCommandsExt` gains `override_widget_interactivity(value)` and `inherit_widget_interactivity()` for panels and other ECS-authored cascade ancestors. Direct mutation of a reified widget's derived `Cascade` is not the durable widget-authoring path; later tree synchronization may replace it, just as for reified text-run style components.
- **Tree changes and invalidation:** `LayoutTree::classify_change` compares the new field and classifies an interactivity-only replacement as `VisualOnly`. Add the same narrow tree mutation/change-classification path used by `PanelText::set_style` so `PanelWidgetWriter` bumps the tree revision and refreshes computed widget records without recomputing geometry. Parent-subtree edits refresh every affected descendant record; widget-local edits refresh only the authored source while preserving widget entity identity and `PanelWidgetReader` lookup.
- **Final marker and first frame:** scheduled `CascadeSet::Propagate` remains before panel layout for existing ECS participants. Phase 2 names the post-reify command fence `WidgetSystems::ReifyCommandsApplied`, places the existing `ApplyDeferred` in it after `WidgetSystems::Reify`, and orders `WidgetSystems::ResolveInteractivity` after it. Bevy Kana's insert observer resolves newly inserted or synchronized widget cascade components after `CascadeFrom` is present during that fence. `ResolveInteractivity` reads only `Resolved<WidgetInteractivity>` and inserts/removes `WidgetDisabled` on an actual effective-value edge. The ordered propagation → layout → reify → cascade-seed fence → marker path produces the correct marker in the widget's creation frame, before its first following `PreUpdate` picking pass. Later `Update` systems that consume newly reified widgets order after `ReifyCommandsApplied`, not merely after `Reify`.
- Disabled changes are visual/state-only by default: no layout recompute unless a preset explicitly opts into different content or dimensions.

**Files:**
- `src/widgets/interactivity.rs` — value type, `PanelWidgetWriter`, and final-marker synchronization
- `src/widgets/mod.rs` — shared plugin registration and ordered fences/system sets
- `src/widgets/reify.rs` — synchronize folded tree authoring and the explicit panel cascade relation
- `src/cascade/attributes.rs`, `src/cascade/resolved.rs` — typed commands and `Enabled` root default
- `src/layout/builder.rs`, `src/layout/element.rs`, `src/layout/engine/` — El-level `Cascade` authoring, parent/child folding, lookup, and tree mutation
- `src/panel/diegetic_panel.rs` — tree-backed widget mutation and visual-only change classification
- `src/lib.rs` — curated widget type re-exports; no `Cascade<T>` re-export
- Read-only templates: `src/render/panel_text/access.rs`, `src/render/panel_text/reify.rs`
- Read-only cascade engine: `crates/bevy_kana/src/cascade.rs`, `docs/bevy_kana/cascade.md`, `src/cascade/mod.rs`

**Constraints from prior phases:** Phase 1 built `widgets/reify.rs` (change-gated tree-walk, reuse by id), `WidgetSpec` on `common.widget`, public `PanelWidgetReader`, and `WidgetsPlugin`/`WidgetSystems`. Its post-reify `ApplyDeferred` is ordered but not named; this phase introduces `WidgetSystems::ReifyCommandsApplied` so every later consumer can order after entity creation rather than only after the reify system. Preserve Phase 1's identical-tree lookup retention and same-id widget identity.

**Acceptance gate:** `cargo nextest run` green with new tests: widgets carry explicit `CascadeFrom(panel)` and do not rely on `ChildOf`; global and panel override changes propagate through the shared cache; nearest parent/child layout override is folded into the widget's one reified `Cascade`, including child `Enabled` inside a disabled parent and layout `Inherit` falling through to panel/global; no `WidgetLayoutInteractivity` or custom precedence resolver exists; `PanelWidgetWriter` accepts both a direct event-target entity and an entity first resolved from `(panel, id)`, writes the authoritative tree, persists across unrelated reify, and makes `inherit_interactivity` reveal the parent-El rule; unsuccessful/stale targets return `false`; unchanged writes do not dirty the panel; a `set_tree` replacement changing only layout-subtree interactivity updates `WidgetDisabled` through the visual-only path without geometry recomputation, respawn, or lookup loss; parent-subtree changes update every affected descendant; a newly reified disabled widget carries the correct marker before its first picking frame; unchanged `Resolved` values do not rewrite `WidgetDisabled` across propagation or tree refresh.

#### Retrospective

**What worked:**
- Folding layout-tree rules into one reified `Cascade<WidgetInteractivity>` let Bevy Kana's existing `Resolved` cache drive `WidgetDisabled` without a second precedence system.
- `PanelWidgetWriter` preserves tree authority and the visual-only path refreshes widget records without another geometry solve.

**What deviated from the plan:**
- The panel-local widget lookup moved from `DiegeticPanel` into required private `PanelWidgetIndex`, so `PanelWidgetReader` and the tree-mutating writer can coexist in one ordinary system.
- Reify now updates kind, authored snapshot, preorder, cascade, and panel relation independently so a kind change does not rewrite unchanged cascade state.

**Surprises:**
- Removing a component does not remove its required sibling components; `PanelWidgetReader` therefore filters the retained index with `With<DiegeticPanel>` before trusting it.
- Acceptance coverage needed a per-panel test-only solve counter to distinguish command regeneration from a full layout solve.

**Implications for remaining phases:**
- Later widget systems can use `PanelWidgetReader` alongside `PanelWidgetWriter`; they must keep the separate `PanelWidgetIndex` synchronized or explicitly clear it when replacing a panel tree.
- Consumers of newly reified widgets still order after `WidgetSystems::ReifyCommandsApplied`, and behavior gates read the derived `WidgetDisabled` marker rather than editing reified cascade storage.

#### Phase 2 Review

- Phase 3 now extends the shipped five-field-plus-cascade computed record without dropping folded interactivity, and carries the explicit pending decision for owner-panel component removal cleanup.
- Screen widget-target work moved directly after world geometry as Phase 4.5; Phase 4 is now independently testable with world demand, and Phase 4.5 generalizes retirement to combined world/screen demand.
- Phase 5 now owns a post-interactivity deferred fence and spawn-only `WidgetFocusable`; Phase 5.5 isolates the optional enhanced-input adapter. Their unresolved public routing/configuration surfaces are recorded as pending decisions instead of being left implicit.
- Same-id/same-kind authored or computed refresh preserves button/slider capture; only removal, kind change, disable, teardown, or an explicit terminal cause cancels behavior.
- `.on_click` registration and retained visual-slot infrastructure are separate Phases 7 and 7.5. Tooltip controller reify and lazy panel materialization are separate Phases 10 and 10.5, each with its own deferred-command visibility point.
- Phase 8 carries the propagated capture/fence constraints; Phases 9 and 12 need no direct Phase 2 correction.

### Phase 3 — Widget `Transform`, single rect source, custom picking backend  · status: todo

#### Work Order

**Goal:** Widgets are first-class Bevy picking targets via a custom backend testing panel-local rects; pointer hover works on widget entities.

**Spec:**
- **Transform:** widgets carry a real panel-local `Transform` — translation = the widget's panel-local offset; `GlobalTransform` propagates via `ChildOf(panel)`. This is deliberately unlike text runs (which carry no `Transform`; their placement is baked into run records) — copying the text-run shape would break the picking backend and collapse anchor geometry to the panel origin.
- **Single rect source:** layout writes the widget's panel-local rect, effective ancestor-clipped rect, current computed-tree preorder, and interaction rank into `ComputedWidgetRecord` once. The shipped Phase 2 record in `widgets/id.rs` contains `id`, `kind`, `preorder`, `authored`, and folded `interactivity`; Phase 3 extends that exact record rather than replacing or reconstructing it. `LayoutTree::computed_widget_records()` cannot see `LayoutResult::computed`, so replace or extend that construction so the full-layout commit joins computed bounds, clipping, and draw order into each record. Make `ComputedDiegeticPanel::regenerate_commands` use the same record path so visual-only updates preserve current geometry and folded cascade while refreshing authored snapshots and ranks. Picking bounds and Phase 4 anchor points project that record; no subsystem recomputes the rect with different invalidation triggers. Fully clipped widgets are not hit targets. Overlap order is deterministic: visual `DrawZIndex`, then source order, with a nested-interaction error from Phase 1 removing ambiguous ancestor/descendant targets.
- **One diegetic backend** (`widgets/picking.rs`): iterate Bevy's `(camera, pointer)` rays, apply the mesh backend's camera order, visibility, `RenderLayers`, `Pickable`, and render-target filters, and immediately raycast only `PanelInteractionMesh` entities. Test only `PanelWidgets` belonging to intersected panels. Emit the panel and all matching widgets in **one** ordered `PointerHits` group so widget depth is actually comparable with its panel; exclude panel interaction meshes from the generic mesh backend. Widget hits are slightly nearer than their panel and ordered against one another by the computed interaction rank.
- **Flat-now/surface-later boundary:** extract the current affine hit→panel-local conversion from `ime/activation.rs` into one shared flat projection helper. Phase 3 supports the currently shipped flat interaction mesh. Phase 5 of `surface-panels.md` later replaces that helper and mesh with `PanelSurface::project()` plus the curved interaction mesh; until then this plan makes no curved-panel picking claim.
- **Pointer presentation:** use Bevy's `PickingInteraction` aggregate for hover/pressed/none presentation across mouse, touch, stylus, and custom pointers. Do not insert `Hovered`: Bevy 0.19 updates it from `PointerId::Mouse` only and performs a linear scan of every entity carrying it. Pointer-specific capture still uses `PointerId` in Phases 6 and 8.

**Pending decision: lifecycle when `DiegeticPanel` is removed but the entity remains**

Actual problem:
Phase 2 moved widget lookup into required `PanelWidgetIndex`. Bevy retains required sibling components when `DiegeticPanel` alone is removed, and the panel's widget children also remain because the owner entity was not despawned. `PanelWidgetReader` safely rejects the retained index through `With<DiegeticPanel>`, but Phase 3 picking and later behavior need an explicit cleanup contract for those still-live widget entities.

What exists now:
- Despawning the panel recursively despawns its widget children through `ChildOf`.
- Removing only `DiegeticPanel` leaves `PanelWidgetIndex` and the widget children resident; the reader returns `None`, but no system tears the children down.

What should change:
- Define whether removing `DiegeticPanel` is a supported conversion path. If it is, an `On<Remove, DiegeticPanel>` cleanup path must clear widget-owned retained state and finalize/despawn widget children before later picking and behavior systems can observe them. If it is not, document that callers must despawn the entity or use a supported panel replacement API instead.

Recommendation:
Support component removal as teardown: finalize widget behavior while targets are still queryable, despawn widget children, and clear panel-private retained indexes/state. This makes `remove::<DiegeticPanel>()` behave like removing the panel role rather than leaving inert semantic children behind.

**Files:**
- `src/widgets/picking.rs` — new (backend)
- `src/widgets/reify.rs` — Transform + computed rect/rank writes
- `src/layout/element.rs` — extend the shipped `computed_widget_records` source-tree walk with computed-layout inputs
- `src/layout/engine/`, `src/render/clip.rs`, `src/render/draw_order.rs` — clipped bounds and interaction rank
- `src/panel/compute_layout.rs` — build geometry-bearing records during a full layout commit
- `src/panel/diegetic_panel.rs` — keep `regenerate_commands` on the same record construction path
- `src/widgets/id.rs`, `src/widgets/mod.rs` — preserve the Phase 2 record fields and implement the chosen owner-component removal lifecycle
- `src/render/panel_geometry.rs`, `src/ime/activation.rs` — owned panel raycast and shared flat conversion

**Constraints from prior phases:** Phase 1 reifies widgets under `ChildOf(panel)` and reuses entities by id. The shipped Phase 2 `ComputedWidgetRecord` stores id, kind, preorder, authored snapshot, and folded interactivity cascade through both full and visual-only panel updates; it does not yet carry computed geometry. Phase 2 supplies `WidgetDisabled` (the backend may still report hits on disabled widgets; behavior systems gate on the marker), the separate required `PanelWidgetIndex`, and the named `WidgetSystems::ReifyCommandsApplied` fence.

**Acceptance gate:** `cargo nextest run` green with new tests: pointer over a widget yields `Over`/`Out` on the widget; one hit group orders widget before panel; partial/full ancestor clipping gates hits; overlapping widgets follow `DrawZIndex` then source order; hidden, layer-mismatched, and non-pickable panels do not hit; two cameras preserve the originating camera and order; mouse and a non-mouse pointer update `PickingInteraction`; an off-origin widget picks at its actual location; full-layout and visual-only record regeneration preserve the folded interactivity value while updating geometry/authored fields independently; the chosen `DiegeticPanel`-removal contract leaves no pickable or reader-resolvable orphan widget.

### Phase 4 — Lazy anchor-geometry publication  · status: todo

#### Work Order

**Goal:** Entities can anchor to widgets: diegetic publishes current `hana_valence` geometry and transforms only while a widget has attachment demand.

**Spec:**
- Publish `ResolvedAnchorGeometry` (the Hana Valence contract component) **lazily** for world attachments. World demand is nonempty `AnchoredHere`; fill on new demand or widget-rect change, and remove geometry after final world demand ends. Phase 4.5 generalizes retirement to combined world-or-screen demand. Never publish on every widget and never use `Changed<Transform>` as the refill trigger.
- World publication runs in `AnchorSystems::FillGeometry` after Phase 1 reify commands are flushed and before `Resolve`. Reify owns the rect in `Update`; the geometry provider projects it in `PostUpdate` without rewriting it.
- Geometry points are projections of the Phase 3 single rect, expressed in the **widget-local frame** matching the panel provider's centered convention; the resolver composes `global_transform * geometry[anchor]`, which is why the widget's own `Transform` must carry its panel-local offset.
- **World resolver bridge:** ordinary transform propagation runs after valence resolution, and a widget's owner panel may itself move inside that same resolver pass. While a world widget has demand, add a private internal `hana_valence::AnchoredTo` bridge from the widget to its owning panel using the widget rect's current panel-local offset. The widget becomes a real resolver candidate only while demanded: graph order resolves an anchored owner panel first, writes the widget's current transform/`resolved_globals` entry second, then resolves sources targeting that widget. Remove the bridge with final world demand. This covers first spawn, parented panels, same-frame panel motion, and anchored-panel→widget→tooltip chains without resolving every widget every frame; no valence type enters the public widget API.
- **Offsets:** generalize `write_panel_anchor_offsets` around a private `AnchorTargetMetrics::{Panel, Widget}`. A widget target resolves its owning panel through `WidgetOf`, uses that panel's layout-unit conversion, and keeps nonzero x/y/z offsets under translation, rotation, and scale. Do not let the existing `Query<(&DiegeticPanel, &GlobalTransform)>` silently remove widget offsets.
- **Diagnostics:** use `AttachmentResolveDiagnostics`' source/target/reason key when an attachment names missing geometry or a despawned target. World failures already flow through `ResolveDiagnostics`; the screen adapter keeps its coordinate-space-specific reason type over the same bounded diagnostic mechanism.

**Files:**
- `src/widgets/reify.rs`, `src/widgets/relationship.rs` — rect ownership and demand transitions
- `src/widgets/mod.rs` — `AnchorSystems::FillGeometry` set membership
- `src/panel/anchoring.rs` — widget-aware offset lowering
- Read-only: `src/panel/valence_provider.rs` (centered provider convention), `crates/hana_valence` (contract types), `docs/hana_valence/as-built/anchoring-and-arrangements.md`

**Constraints from prior phases:** Phase 3 built the single panel-local rect source and gave widgets a real panel-local `Transform`. Phase 1 reify runs during `Update` and flushes before either screen or world attachment work.

**Acceptance gate:** `cargo nextest run` green with new tests: first-frame and same-frame panel motion place an ordinary world attachment at an off-origin widget corner; an anchored owner panel → widget → dependent chain resolves in graph order in one pass; geometry and the private bridge are absent without world demand, refill on rect change, and are removed after final world demand; two world dependents keep them resident until both detach; nonzero pixel and physical-unit offsets survive transformed owning panels; missing-geometry diagnostics deduplicate by source, target, and reason. Screen demand and combined retirement belong to Phase 4.5.

### Phase 4.5 — Screen-placer widget targets  · status: todo

#### Work Order

**Goal:** Screen-space anchored panels can target widgets, not only panels, and shared widget geometry remains resident while either world or screen placement needs it.

**Spec:**
- The screen placer builds candidates from `PanelAttachmentAuthored` but accepts panel targets only today. Teach it to recognize a widget, resolve the owning screen panel/window through `WidgetOf`, and derive the target rectangle from current widget-local geometry plus the owning panel's screen rect/transform instead of `ScreenPanelRect` on the target. The screen source still must not carry `hana_valence::AnchoredTo`.
- Add a private source/target relationship for screen widget attachments, analogous to `AnchoredTo`/`AnchoredHere` but without `linked_spawn`. Insert/replace/remove/despawn and retargeting keep the reverse target membership exact. Nonempty membership is screen geometry demand, supports multiple sources, and prevents geometry retirement until the last source detaches. Future `TooltipFor` semantic membership does not count as geometry demand before materialization.
- Screen demand synchronization and widget geometry publication run in `Update` after `WidgetSystems::ReifyCommandsApplied`, followed by their own `ApplyDeferred`, and before `PanelSystems::ResolvePanelAttachments`. Ordering after `Reify` alone is insufficient because it does not guarantee that deferred widget entities and relationships are visible. This is separate from the world `AnchorSystems::FillGeometry` provider in `PostUpdate`; no ordering claim crosses schedules.
- **Graph dependency proxy:** for every demanded widget, add a private resolver candidate `widget → owning panel`. Its placement action recomputes the widget's screen rect after the owning panel's current placement; real attachments still target the widget candidate. This gives `resolve_attachments` the required owner-panel→widget→dependent order when the owner panel is itself attached, without exposing the proxy as authoring or mutating the widget hierarchy.
- Reuse Phase 4's `AnchorTargetMetrics::Widget` for screen layout-unit offsets. A missing owner, window, geometry, or transform yields the screen adapter's source/target/reason diagnostic instead of a panel-only fallback.
- Generalize Phase 4 geometry retirement so a widget keeps shared geometry while either world `AnchoredHere` or screen reverse membership is nonempty. Final detachment in one space must not retire geometry still demanded by the other.
- Keep window and viewport projection in diegetic, but continue delegating graph ordering, cycles, fallback, and diagnostics to `hana_valence::resolve_attachments`. Missing widget geometry uses the screen adapter's `AttachmentResolveDiagnostics` source/target/reason key.

**Files:**
- `src/screen_space/anchoring/mod.rs` — order demand publication after `WidgetSystems::ReifyCommandsApplied` and before attachment resolution
- `src/screen_space/anchoring/candidate.rs` — widget-target candidate rects
- `src/screen_space/anchoring/resolve.rs` — target resolution
- `src/screen_space/anchoring/projection.rs` — reuse/extend projection helpers
- `src/widgets/relationship.rs`, `src/widgets/reify.rs` — screen reverse relationship, combined demand, and geometry retirement

**Constraints from prior phases:** Phase 1 reifies widget identity early in `Update`; Phase 2 names the post-reify command fence `WidgetSystems::ReifyCommandsApplied`; Phase 3 adds the widget rects and panel-local `Transform`; Phase 4 owns world-demand geometry, the private world bridge, and target metrics. Screen sources retain private `PanelAttachmentAuthored` and do not enter the world resolver.

**Acceptance gate:** `cargo nextest run` green with new tests: a new or relaid-out widget target resolves in the same frame; an attached owner panel → widget → dependent chain follows graph order; placement uses the widget viewport rect and nonzero offset; two screen attachments maintain demand until both detach; one world and one screen attachment retain geometry until both are gone; retargeting moves reverse membership; final combined detach removes geometry; panel targets remain unchanged; missing owner/geometry warnings deduplicate per source, target, and reason.

### Phase 5 — Focus subsystem and semantic routing  · status: todo

#### Work Order

**Goal:** Window-scoped focus and binding-library-independent semantic requests work across all widgets with deterministic panel-local traversal.

**Spec:**
- `widgets/focus.rs`. Focus is shared, not button-local. One crate-private authoritative resource maps each window to its active panel and focused widget; marker state is never an independent authority. The exact public request, clear, change, cause, and semantic-action types are fixed in the public contract ledger.
- `WidgetFocusable` participation component, inserted only when a widget entity is first spawned; removing it opts that live widget out of keyboard traversal without changing pointer picking. Same-id reify, reorder, authored refresh, and kind-preserving updates must not restore a deliberately removed marker.
- `WidgetFocused(())` is a public read-only presence marker with a private field, synchronized only by one focus-transition function. Public app control uses typed request/clear events carrying the window and target; `WidgetFocusChanged` reports old/new entities and a cause. `RequestWidgetFocus` remains entity-targeted: callers starting from an authored `(panel, id)` resolve it through `PanelWidgetReader`, while pointer and existing event flows already have the entity. There is no parallel id-targeted focus request.
- Focus is gained by pointer focus, traversal, semantic routing, or app request. It is lost by transfer, despawn/removal, `WidgetFocusable` removal, panel/window input-scope loss, or explicit clear — **not** by disable. Disabled focusable widgets may retain or receive focus and participate in traversal; behavior modules ignore activate/change input while disabled.
- Traversal order is the current `ComputedWidgetRecord` preorder rebuilt in Phase 1, never `PanelWidgets` relationship insertion order. Next/previous/first/last stay within the active panel for that window and wrap deterministically; focusing a widget on another panel transfers the active panel. Structural reorder changes traversal without respawning entities.
- Define the six semantic action types: next, previous, first, last, activate-focused, cancel-focused. Core focus requests/events and their observable routing do not depend on a binding library; Phase 5.5 only translates enhanced-input action edges into this core path.
- Add `WidgetSystems::InteractivityCommandsApplied`: an `ApplyDeferred` fence after Phase 2's `ResolveInteractivity`. Focus/semantic behavior and every later button, slider, and tooltip behavior system that reads `WidgetDisabled` orders after this fence, so same-frame marker insert/remove commands are visible before behavior runs.
- **Ordering and IME:** pointer focus is visible before same-frame activate handling. Semantic widget input runs after `ImeSystemSet::PublishInputBlockers` and ignores a window while `ImeInputBlocker::blocks_window(window)` is true.
- Design with accessibility in mind (structure the traversal so an a11y layer can attach later), without adding bevy_a11y.

**Pending decision: core semantic-action routing contract**

Actual problem:
The public ledger names six semantic action types, but it does not say whether they are binding-library action markers, app-sendable window-scoped requests, or entity-targeted events. Without that contract, Phase 5 cannot prove how `ActivateFocusedWidget` reaches the currently focused widget, and Phase 5.5 cannot be a replaceable adapter.

What exists now:
- Focus authority is window-scoped and the focused widget is crate-private state.
- Pointer/app focus requests already use typed public events, while `bevy_enhanced_input` is intended to remain optional.
- Button and slider behavior need one entity-targeted routed intent, not knowledge of input contexts or bindings.

What should change:
- Freeze one library-independent public request shape carrying the window for next/previous/first/last/activate/cancel.
- Define one private routed intent carrying the resolved widget entity that later behavior modules consume after focus and IME gating.
- Make the enhanced-input adapter translate action edges into the public/core request path rather than becoming a second authority.

Recommendation:
Make the six exported semantic types app-sendable window-scoped messages, then route them through focus authority into one private entity-targeted semantic intent. This gives applications and tests a stable headless API while keeping `bevy_enhanced_input` entirely inside Phase 5.5.

**Files:**
- `src/widgets/focus.rs` — new
- `src/widgets/input.rs` — core semantic request types and routing only
- `src/widgets/mod.rs` — systems in `WidgetSystems` after picking; post-interactivity deferred fence
- `src/widgets/reify.rs` — default `WidgetFocusable` insertion

**Constraints from prior phases:** Phase 2 supplies `WidgetDisabled`; Phase 3 supplies pick targets and `PickingInteraction`; Phase 1 supplies current traversal order. Activation of a focused button lands in Phase 6; this phase routes the action to that later behavior hook.

**Acceptance gate:** `cargo nextest run` green with new tests: next/previous/first/last and wrap order; structural reorder updates order while preserving entities; two windows hold isolated focus; an app focus request can target a widget resolved from `(panel, id)`, while pointer focus uses its existing entity; app clear and change causes; focus loss on despawn, `WidgetFocusable` removal, and explicit clear; removing `WidgetFocusable` survives same-id reify and reorder; disabled widgets retain and can receive focus but activate is a no-op; a same-frame interactivity edge is applied before semantic behavior; pointer-focus plus activate works in one frame; IME blocks semantic actions only in its leased window; the resolved semantic intent names the focused entity exactly once.

### Phase 5.5 — Enhanced-input adapter  · status: todo

#### Work Order

**Goal:** An opt-in `bevy_enhanced_input` adapter installs, rebinds, disables, and removes per-window contexts while feeding only Phase 5's core semantic path.

**Spec:**
- Add the direct workspace dependency and expose `WidgetInputPlugin`, `WidgetInputBindings`, and neutral `WidgetControlSummary` from the public contract ledger. No raw key handling lives in widgets.
- `WidgetInputPlugin::new(bindings)` owns a context entity for each live window. Action edges translate into Phase 5's settled window-scoped semantic requests; focus and behavior modules never query enhanced-input action entities directly.
- Binding changes reconcile per-window action/context entities by diff. Window removal or adapter disable removes every plugin-owned action/context entity; repeating install/rebind/disable/remove is a no-op.
- Adapter action processing runs after `ImeSystemSet::PublishInputBlockers`; a leased IME window produces no semantic request, while other windows remain active.

**Pending decision: runtime binding and adapter-disable API**

Actual problem:
`WidgetInputPlugin::new(bindings)` defines initial installation, but the current ledger promises later rebind/remove/disable behavior without naming any public mutation surface. A delegate cannot implement or test idempotent reconciliation without knowing where desired bindings and enabled state live.

What exists now:
- `WidgetInputBindings` and `WidgetControlSummary` are promised exports.
- The plugin owns per-window enhanced-input context/action entities.
- Phase 5 owns the semantic request contract and remains usable without this plugin.

What should change:
- Name one public source of desired adapter configuration and enabled state.
- Specify whether removing/disabling the adapter preserves focus state (recommended) while only deleting adapter-owned context/action entities.
- Define how apps perform a runtime rebind and how invalid/no-op updates are reported.

Recommendation:
Have the plugin insert a public `WidgetInputSettings` resource containing `{ enabled, bindings }`; apps mutate it through `ResMut`, and changed-only reconciliation updates every live window. Disabling removes only adapter-owned entities and leaves Phase 5 focus/semantic APIs intact. Keep `WidgetInputBindings` as the validated binding value inside that resource and expose read-only `WidgetControlSummary` for UI/help text.

**Files:**
- `src/widgets/input.rs` — enhanced-input action/context adapter and public configuration surface
- `src/widgets/mod.rs` — adapter ordering and registration
- `crates/hana_diegetic/Cargo.toml` — direct `bevy_enhanced_input` dependency
- `src/lib.rs` — curated adapter exports
- Read-only reference: `crates/bevy_lagrange/src/input/`

**Constraints from prior phases:** Phase 5 owns focus authority, IME gating, and the settled core semantic request/routed-intent contract. The adapter must not mutate focus directly or expose enhanced-input types through core behavior APIs.

**Acceptance gate:** `cargo nextest run` green with new tests: default install creates one context per live window; action edges emit the corresponding Phase 5 semantic request once; IME blocks only its leased window; adding/removing windows reconciles ownership; runtime rebind updates contexts without duplicates; repeated equal configuration is a no-op; disable removes plugin-owned context/action entities without clearing focus; re-enable restores exactly one installation per window; removing the adapter-owned configuration follows the settled public contract.

### Phase 6 — Button behavior  · status: todo

#### Work Order

**Goal:** Headless button with the four-event lifecycle, emulated pointer capture, semantic activation, and IME coexistence.

**Spec:**
- `widgets/button.rs`. `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, and `ButtonCanceled` derive `EntityEvent`; their exact fields, semantic-click representation, cause enum, and crate-root exports are fixed in the public contract ledger. No double-click event.
- **Lifecycle invariant** — a pressed button resolves to exactly one terminal path:
  - `Pressed -> Released -> Clicked` for a valid pointer click.
  - `Pressed -> Released` without `Clicked` for a valid release that no longer activates.
  - `Pressed -> Canceled` for capture loss, disable-while-pressed, widget removal, same-id kind change, owner-panel teardown, pointer cancellation/removal, or explicit cancel.
  - Semantic activation emits `ButtonClicked` without entering the pointer lifecycle.
- **Identity-preserving refresh:** an accepted `set_tree` or `PanelWidgetWriter` update that preserves the same panel, named id, and `WidgetKind::Button` does not cancel an active press merely because authored/computed snapshots refreshed. Reify updates the live record in place and capture continues. Removal, kind change, disable, or the chosen owner-panel teardown contract are the structural cancellation edges.
- **Emulated capture:** a private resource maps each occupied `PointerId` to one widget. A second press for an occupied pointer or on an already-captured widget is ignored. `ButtonPress` stores the pointer and a typed terminal state (`Pending`, `Release(outcome)`, `Cancel(cause)`); every global release/cancel/drag-end path matches both the captured pointer and widget before acting.
- **Terminal choke point:** set the terminal state before removing `ButtonPress`; one `On<Remove, ButtonPress>` observer emits `Released` plus optional `Clicked`, or `Canceled`. `Pending` removal is cancellation. Widget/kind removal runs finalization and targeted event dispatch while the entity still exists, then despawns/removes the behavior bundle. Do not queue an entity-targeted terminal event after its target is gone.
- **Pointer loss:** `Pointer<Cancel>` targets only currently hovered entities and cannot cover capture over empty space. Capture cleanup also consumes raw pointer cancellation/removal and uses `DragEnd` for drag-off release-in-void. Every path removes the capture-map entry exactly once.
- **Disable-while-pressed:** inserting `WidgetDisabled` on a pressed button must actively remove the live `ButtonPress` with a Canceled cause — a flag alone lets the pending Release/DragEnd resolve as Clicked. Disabled buttons ignore pointer and semantic activation and cannot keep capture.
- **Semantic activation:** a non-pointer path (keyboard shortcuts, action systems, or Phase 5 activate-focused routing) targeting the focused or an explicitly targeted button; emits `ButtonClicked` directly, no fabricated pointer events.
- **IME coexistence:** the Phase 3 ordered hit group makes the widget the event target. Before the widget stops click propagation, call a factored IME blur-classification helper with `WidgetOf::panel()` so clicking a button commits an editor outside that focus scope. Then stop propagation so the panel's double-click field activator cannot open a field underneath the button.
- Presentation state comes from `PickingInteraction`; no bespoke hover events in the first API.

**Files:**
- `src/widgets/button.rs` — extend the Phase 1 authoring module with button behavior
- `src/widgets/mod.rs` — observers + systems registration
- `src/ime/editor.rs` — shared widget-aware blur classification
- Read-only: `src/ime/activation.rs`, `/Users/natemccoy/rust/bevy/crates/bevy_ui_widgets/src/button.rs`

**Constraints from prior phases:** Phase 1 owns `Button` authoring, same-kind entity reuse, and Phase 1-owned kind replacement/removal. Extend those transitions here so button behavior is finalized before a kind change or removal. Phase 2 supplies `WidgetDisabled`; Phase 3 supplies ordered hits and `PickingInteraction`; Phase 5 supplies activate-focused routing and the post-interactivity command fence. The optional Phase 5.5 adapter feeds that core route but is not a behavior dependency.

**Acceptance gate:** `cargo nextest run` green with new tests: press→release→click and release-without-click; pointer ids match every terminal path; a second pointer cannot terminate the first; cancel over empty space, raw pointer removal, drag-off release, disable-while-pressed, widget removal/despawn, same-id kind change, owner-panel teardown, and explicit cancel each emit exactly one `ButtonCanceled`; same-id/same-kind tree refresh and interactivity writes that remain enabled preserve capture; semantic activation emits `ButtonClicked` alone; same-panel and other-panel button clicks classify IME blur correctly while a button over a field blocks field activation.

### Phase 7 — `.on_click` sugar  · status: todo

#### Work Order

**Goal:** Retained button authoring can install ergonomic typed click callbacks without giving `LayoutBuilder` access to `World`.

**Spec:**
- **Event consumption, base path:** app code observes `ButtonClicked` globally or through an entity-scoped observer and reads the widget entity directly from the event target; the payload id is a convenience for logging or panel-local application logic. An event handler never re-resolves its target through `PanelWidgetReader`. The reader is only a pre-event bridge when app code starts from authored `(panel, id)` and needs the entity to install a scoped observer/effect or issue other entity-targeted control. This base path ships alongside the sugar, not instead of it; id alone is never globally unique.
- **`.on_click` sugar:** preserve `.on_click(closure)` without requiring `LayoutBuilder` to access a `World`. `Button` stores a private, cloneable callback template: an `Arc`-owned wrapper around a typed `SystemHandleTemplate<In<ButtonClicked>, ()>`, compared by `Arc` identity so `WidgetSpec` stays comparable. World-aware reify builds one tracked `SystemHandle`, stores it on the widget, and a uniform observer calls `run_system_with` using the clicked event. Reuse never registers again; callback replacement drops the old strong handle, and final-handle drop lets Bevy clean up the registered system. Cost: one allocation per authored callback and reference-count operations when a tree clones, with no per-click allocation.

**Files:**
- `src/widgets/button.rs`, `src/layout/builder.rs` — typed callback template and `.on_click`
- `src/widgets/reify.rs` — tracked callback handle and uniform observer
- `src/lib.rs` — `.on_click`-related curated exports if required

**Constraints from prior phases:** Phase 6 defined typed click input and lifecycle; Phase 1 requires `WidgetSpec: Clone + PartialEq`. Callback replacement is a same-id/same-kind authored refresh and must not disturb a live press except when the `Button` kind is removed.

**Acceptance gate:** `cargo nextest run` green with new tests: `.on_click` receives `ButtonClicked`; reify reuse does not re-register; callback replacement releases only the prior tracked handle and installs exactly one replacement; widget removal releases the final tracked callback; global observation reads the event target directly, and an entity-scoped observer installed on a previously reader-resolved widget receives the click without re-resolving it.

### Phase 7.5 — Retained widget visual slots and ButtonPreset  · status: todo

#### Work Order

**Goal:** Widget state can patch retained render data without relayout, and a default material-first button preset consumes that path.

**Spec:**
- **Runtime visual overrides** (`widgets/visual.rs`): ordinary fills, borders, images, and slider parts are retained render records, not ECS child entities. Presets assign stable private visual-slot ids to their `El`/`PanelDraw` primitives; layout output carries slot→record references into `ComputedWidgetRecord`. Widget entities own changed-only override components, and render batching patches only the referenced material/color/z/transform records and dirty GPU rows. Never mutate `DiegeticPanel` or `ComputedDiegeticPanel` merely for a widget state flip.
- **ButtonPreset / ButtonStyle** (`widgets/presets/button.rs`, shared helpers in `presets/style.rs`): helpers generate `LayoutTree` fragments and stable slots. Material-first: colors/images are convenience inputs resolving to `StandardMaterial`; custom shader cases use custom handles or `ExtendedMaterial`. Widget-specific names only. Presets read `PickingInteraction`, `Has<WidgetDisabled>`, `Has<WidgetFocused>`, and capture state, then write widget visual overrides. Rich content remains ordinary layout content.
- **Tree-backed disabled writes versus state presentation:** a panel/global cascade edge reaches the preset without mutating `DiegeticPanel` or `ComputedDiegeticPanel`. A widget-local `PanelWidgetWriter` edit is allowed the one authoritative tree mutation and visual-only computed-record refresh required by Phase 2; once that refreshed interactivity reaches `WidgetDisabled`, the preset must patch render rows without causing any additional panel/computed refresh or geometry solve.
- **Boundary guardrail:** presets depend on behavior, never the reverse; add a test/lint asserting behavior modules (`button.rs`, `focus.rs`, `interactivity.rs`, …) reference no layout/material types.

**Files:**
- `src/widgets/presets/mod.rs`, `src/widgets/presets/button.rs`, `src/widgets/presets/style.rs` — new
- `src/widgets/visual.rs`, `src/layout/engine/`, render batch-record writers — visual slots and changed-row patching
- `src/widgets/reify.rs` — attach stable visual-slot references without rewriting unrelated record fields
- `src/lib.rs` — preset re-exports

**Constraints from prior phases:** Phase 6 supplies button lifecycle state; Phase 3 supplies `PickingInteraction` and deterministic interaction rank; Phase 5 supplies focus/disabled ordering; Phase 2 distinguishes panel/global cascade changes from tree-backed widget authoring and guarantees interactivity-only edits skip geometry solving.

**Acceptance gate:** `cargo nextest run` green with new tests: hover/press/focus and panel/global disabled edges patch only expected render rows and do not fire `Changed<DiegeticPanel>` or `Changed<ComputedDiegeticPanel>`; a widget-local disabled edit performs exactly its Phase 2 visual-only computed refresh and no layout solve, then preset presentation causes no second panel/computed change; repeated identical state is a no-op; unrelated visual slots stay untouched; the behavior-module boundary test passes.

### Phase 8 — Slider behavior  · status: todo

#### Work Order

**Goal:** Headless slider: grab, drag, value change, release, cancel, disabled, optional snapping, with correct out-of-bounds drag mapping.

**Spec:**
- `widgets/slider.rs`: extend the Phase 1 authoring/configuration module with runtime slider state and behavior. `WidgetSpec::Slider`, `El::slider`, `Slider`, `SliderRange`, `SliderStep`, `SliderDirection`, and `SliderConfigError` already exist and must not be redefined.
- **Shipped construction contract:** Phase 1 provides `SliderDirection::{LeftToRight, RightToLeft, BottomToTop, TopToBottom}`; finite strictly ordered `SliderRange`; finite positive `SliderStep`; validated `Slider::new(range, initial_value)` plus `step` and `direction` builders; and the `thiserror`-derived `SliderConfigError::{NonFiniteRange, UnorderedRange, NonFiniteValue, NonPositiveStep}` with stable-message tests.
- **Approved runtime value contract (PD1):** private-field `SliderState` is the public component containing range, applied raw-domain value, optional step, and direction. `SliderState::new(range, value, step, direction) -> Result<Self, SliderConfigError>` and `set_value(value) -> Result<bool, SliderConfigError>` reject non-finite input, snap to the lattice anchored at `range.start()`, then clamp, with the Boolean reporting an applied-value change. It also exposes `range()`, `value()`, `step()`, and `direction()` readers.
- **App authority and request API:** `SliderChangeRequested` targets the widget and carries `{ id, value, is_final, pointer_id: Option<PointerId> }`; pointer drags send non-final proposals plus a final proposal on release, while semantic/remote requests are final and have no pointer. App code explicitly applies or rejects the proposal with `SliderState::set_value`. The exported `slider_self_update` observer is the opt-in uncontrolled convenience. `RequestSliderAdjustment { entity, adjustment }` computes and emits a proposal without applying it; an app controller that starts from authored `(panel, id)` resolves that entity through `PanelWidgetReader` before constructing the request, rather than using a second id-targeted request type. `SliderAdjustment` is exactly `Absolute(f32) | Relative(f32) | RelativeSteps(f32)`. Every adjustment validates its numeric input; `RelativeSteps` emits no proposal when the state has no step.
- **Authored/runtime ownership:** `Slider::initial_value` applies only on first spawn. Phase 1 accepts any finite authored initial value, including values outside the range or off the optional step lattice; first reification constructs `SliderState` through `SliderState::new`, applying the same snap-to-`range.start()` lattice then clamp order as later state changes. Same-id reuse preserves the live applied value; an authored range/step/direction change updates the configuration and revalidates the preserved value, while an unrelated reify does not rewrite `SliderState`. The preset reads only the applied value, never an unaccepted proposal.
- **Bevy reference check:** both the project-version `bevy_ui_widgets 0.19.0` source and local `../bevy` at `0.20.0-dev` use raw-domain state, external `ValueChange<f32>` proposals, optional self-update, and absolute/relative/relative-step remote control. Hana adopts those semantics. It intentionally does not copy Bevy's independently insertable tuple components, warn-only invalid ranges, separate `SliderPrecision`, `TrackClick`, auto-orientation, accessibility dependency, or UI-space drag delta; Hana keeps one validated state, one step lattice for pointer and semantic values, four explicit directions, and captured-camera panel-local reprojection.
- **Drag mapping:** map panel-local position to normalized value, then through the chosen range. Each `Pointer<Drag>` reprojects `pointer_location.position` via the **captured press camera and render target** → `Camera::viewport_to_world` → ray → flat panel intersection → panel-local map → clamp. `Drag.delta` is invalid for perspective world panels. Cancel if the captured camera/target disappears or no longer matches. The surface-panels integration later replaces only the flat ray→panel-local helper.
- **Lifecycle:** reuse Phase 6's capture registry, pointer matching, terminal state, raw pointer-loss handling, and finalize-before-despawn rule. `SliderGrabbed`/`SliderReleased` carry `{ entity, id, pointer_id }`; `SliderCanceled` adds `cause`, whose variants are `PointerCanceled | PointerRemoved | CaptureLost | Disabled | ProjectionUnavailable | WidgetRemoved | WidgetKindChanged | Explicit`. `WidgetDisabled` cancels an active drag and blocks pointer or semantic changes. A same-panel/same-id/same-kind tree refresh preserves capture and the live applied value; only removal, kind change, disable, projection/capture loss, explicit cancel, or owner-panel teardown terminates it.

**Files:**
- `src/widgets/slider.rs` — extend the Phase 1 authoring/configuration module with runtime state and behavior
- `src/widgets/picking.rs` (or a shared geometry module) — `panel_local_from_ray` helper
- `src/widgets/reify.rs` — slider kind reify

**Constraints from prior phases:** Phase 1 owns slider authoring, validated construction types, the `thiserror` error contract, computed snapshots, and Phase 1-owned kind replacement/removal. Extend reify here to preserve live `SliderState` on same-id reuse and finalize slider behavior on kind change/removal. Phase 6 owns the shared capture/terminal mechanism; Phase 3 supplies panel-local geometry and the press hit's camera; Phase 5 supplies the post-interactivity fence over Phase 2's `WidgetDisabled`; PD1 is accepted and fixed above.

**Acceptance gate:** `cargo nextest run` green with Phase 1's construction/error tests retained and new tests for: direction/value mapping for all four directions; lattice anchoring plus snap/clamp order; first-spawn normalization of out-of-range and off-step authored initial values; app accept/reject and opt-in self-update; absolute/relative/relative-step requests, including a remote request sent to an entity resolved from `(panel, id)`; non-final/final proposal ordering; spawn-only initial value and authored range-change clamping; zero-size track; drag beyond panel bounds; captured-camera loss and multi-camera reprojection; every cancel path including pointer loss, kind change, owner-panel teardown, and disable-while-dragging; same-id/same-kind refresh preserves an active drag; disabled slider ignores grab and semantic change.

### Phase 9 — Slider overlay preset  · status: todo

#### Work Order

**Goal:** Default slider visual preset using overlay layout.

**Spec:**
- `widgets/presets/slider.rs`: `SliderPreset` / `SliderStyle` (widget-specific names, material-first slots like ButtonPreset).
- Use `El::overlay()` — track, fill, thumb, and optional labels share one content rectangle and are layered, not arranged. `DrawZIndex` orders thumb above fill above track.
- Thumb/fill placement reads PD1's applied value. Restyle and thumb movement write Phase 7.5 visual-slot overrides, patching only retained render records — no `LayoutTree` or computed-panel regeneration per value change.
- Preset respects `SliderDirection` for fill/thumb placement in all four directions.

**Files:**
- `src/widgets/presets/slider.rs` — new
- `src/widgets/presets/style.rs` — shared helpers only where they remove real duplication
- `src/lib.rs` — re-exports

**Constraints from prior phases:** Phase 8 supplies `SliderDirection` and PD1's applied-value contract. Phase 7.5 supplies stable visual slots and the behavior/preset boundary.

**Acceptance gate:** `cargo nextest run` green with new tests: thumb/fill records track the applied value in all four directions; a value change dirties only the expected visual slots and causes no relayout; preset builds under the behavior/preset boundary test.

### Phase 10 — Tooltip template, relationship, and controller reify  · status: todo

#### Work Order

**Goal:** Associated and standalone tooltip declarations produce stable lightweight controller entities related to their targets, without materializing panels or creating anchor demand.

**Spec:**
- **Semantic relationship:** every tooltip is its own entity with public `TooltipTemplate` and `TooltipFor(target)` components; public `Tooltips` is the reverse target membership and is declared with `linked_spawn`, so target despawn owns tooltip-controller cleanup without hierarchy parenting. The target may be a widget or a `DiegeticPanel`. This relationship is semantic ownership, not placement; Phase 10.5 adds `AnchoredToPanel` only after materialization.
- **Associated authoring:** `El::tooltip(self, template: TooltipTemplate) -> Self` writes a separate tooltip declaration field parallel to `CommonEl::widget`; it is not a field on `Button`, `Slider`, or crate-private `WidgetSpec`. The first API permits at most one associated tooltip and requires the same `El` to be a widget. Layout output creates a private `ComputedTooltipRecord` keyed by `(panel, widget id)`.
- **Standalone authoring:** app code uses `commands.spawn((template, TooltipFor::new(target)))` against an existing widget or panel. When it knows a widget only by authored `(panel, id)`, it resolves the target through `PanelWidgetReader` first; a caller that already has an event target or widget entity skips lookup. Associated and standalone controllers feed the same Phase 10.5 behavior path.
- **Public template and policy builders:** `TooltipTemplate` is a cloneable component containing an immutable concrete panel blueprint behind an `Arc` plus private `show_after`, `hide_after`, and `disabled_policy` values. `TooltipTemplate::new(tree: LayoutTree)` installs `show_after = Duration::from_millis(500)`, `hide_after = Duration::ZERO`, and `TooltipDisabledPolicy::Suppress`; consuming `.show_after(Duration)`, `.hide_after(Duration)`, and `.disabled_policy(TooltipDisabledPolicy)` builders override them. There is no separate public `Tooltip` policy component or `.tooltip_with_policy(...)` path.
  ```rust
  pub enum TooltipDisabledPolicy {
      Show,
      Suppress,
  }
  ```
  No public `TooltipTrigger`, `TooltipTiming`, or fixed visible-duration setting exists; hover-or-focus behavior is assumed in Phase 10.5.
- **Template equality and deferred creation:** equality compares panel-blueprint pointer identity plus policy values. Associated authoring carries the same value through `ComputedTooltipRecord`, cloning only the `Arc`; standalone authoring inserts it directly. An identical clone is unchanged, a policy-only replacement is distinguishable from a blueprint replacement, and no phase spawns panel/render components merely because a template exists.
- **Controller reify and fence:** add `TooltipSystems::ReifyControllers` after `WidgetSystems::ReifyCommandsApplied`. It uses `PanelWidgetReader` to create/reuse associated controllers and synchronizes the template plus `TooltipFor(widget)` independently. Follow it with `TooltipSystems::ControllerCommandsApplied`, an explicit `ApplyDeferred` fence consumed by Phase 10.5. Ordering after the widget fence alone is insufficient for systems that need newly created tooltip controllers and relationships.
- **Identity and cleanup:** same panel/widget id reuses the controller across unrelated, policy-only, and identical-tree refreshes; template fields update independently. Associated declaration removal despawns that controller, and target despawn does the same through `linked_spawn`. Controller indexes follow source revision even when an identical `set_tree` does not change `ComputedDiegeticPanel`; never clear an index unless the same command path also updates/reifies it.

**Files:**
- `src/widgets/tooltip.rs` — template and lightweight controller state
- `src/widgets/mod.rs` — `TooltipSystems` controller-reify and deferred-command fence
- `src/widgets/relationship.rs` — `TooltipFor` / `Tooltips`
- `src/widgets/reify.rs` — associated tooltip controller reuse/removal after widget reify
- `src/layout/builder.rs`, `src/layout/element.rs`, `src/layout/engine/` — separate tooltip declaration and computed record
- `src/lib.rs` — curated template and relationship exports

**Constraints from prior phases:** Phase 1 preserves widget/text indexes when an identical `set_tree` advances the source revision without changing computed output; tooltip controller indexes must follow the same rule because no computed change is guaranteed to retrigger reify. Phase 2 provides `PanelWidgetReader` and `WidgetSystems::ReifyCommandsApplied`. Controller existence alone creates no world or screen demand from Phases 4/4.5.

**Acceptance gate:** `cargo nextest run` green with new tests: associated tooltip authoring does not alter `Button`/`Slider` equality; one lightweight controller and exact reverse relationship exist after `TooltipSystems::ControllerCommandsApplied` with no `DiegeticPanel`, placement relation, render data, or geometry demand; unrelated and policy-only tree replacements reuse that controller; an exact identical `set_tree` replacement preserves the same associated controller and every controller lookup/index without relying on a computed-panel change; a blueprint replacement updates the same controller; a standalone tooltip can target an entity resolved from `(panel, id)` and the same call accepts an already-known event target without lookup; omitted builders produce 500 ms show and zero hide delays plus `Suppress`; associated declaration removal and target despawn each clean up exactly once.

### Phase 10.5 — Tooltip behavior and lazy panel materialization  · status: todo

#### Work Order

**Goal:** Eligible tooltip controllers materialize once into hidden anchored panels, reveal only when ready, emit visibility events, and thereafter hide/show without respawning.

**Spec:**
- **Eligibility and ordering:** read target `PickingInteraction` plus `WidgetFocused` where applicable. Run after Phase 5's `WidgetSystems::InteractivityCommandsApplied` and Phase 10's `TooltipSystems::ControllerCommandsApplied`, so newly resolved disabled state and newly reified controllers are visible. `TooltipDisabledPolicy::Suppress` prevents or immediately ends visibility; `Show` leaves hover-or-focus eligibility unchanged.
- **One private state machine:** `TooltipPhase::{Hidden, WaitingToShow(Timer), Visible, WaitingToHide(Timer)}` is authoritative. `show_after` starts when eligibility becomes true and is canceled on loss. `hide_after` starts when eligibility becomes false, is canceled if eligibility returns, and hides on expiry; `Duration::ZERO` is immediate. Target removal despawns immediately. `Visibility` is derived from the phase; no second visible marker or simultaneous timer components. Tick only waiting entities.
- **Visibility events:** public non-propagating `TooltipShown` and `TooltipHidden` derive `EntityEvent` with sole event-target field `entity: Entity`. Emit exactly once per actual hidden→visible or visible→hidden edge. `TooltipShown` observes visible state plus ready layout/placement; `TooltipHidden` observes hidden state and fires before controller cleanup, so effects can still query the entity, transform, template, and relation. Canceled waits and redundant states emit nothing.
- **Lazy materialization and command visibility:** on the first show wait, build panel and placement components on the same controller with `Visibility::Hidden`; pre-materialized controllers carry none of them. Put insertion/rebuild commands behind `TooltipSystems::MaterializationCommandsApplied`, then let readiness checks consume that fence and the existing layout/placement completion signals. Even `show_after == Duration::ZERO` waits for successful layout and placement and never flashes at a fallback transform. Once materialized, panel/layout/render/placement state remains resident while hidden.
- **Coordinate-space inheritance:** resolve a widget target through `WidgetOf` or read a panel target directly, then build in the target panel's `World` or `Screen` coordinate space and layout unit. Cross-space placement is out of scope. World tooltips lower to `hana_valence::AnchoredTo`; screen tooltips retain panel authoring and use the Phase 4.5 screen widget-target adapter. `AnchoredToPanel` must always match `TooltipFor` on materialized controllers.
- **Lifecycle and demand:** pre-materialized `TooltipFor` membership is not anchor demand. Materialization inserts the world/screen placement relationship and starts demand; hiding retains it. Retargeting updates `TooltipFor` immediately and the materialized placement target through the same transition. World `AnchoredHere` and Phase 4.5's screen reverse relationship keep geometry resident only while placement needs it. There is no inactivity eviction.
- **Blueprint replacement and reset:** an identical template does nothing; policy-only replacement updates current/future timers without rebuilding panel components. A new blueprint first transitions hidden (emitting `TooltipHidden` if visible), then one private reset path removes all materialized panel/layout/render/placement components and clears required retained siblings/indexes, including the Phase 2 `PanelWidgetIndex`, before inserting the new blueprint on the same controller. Reuse the settled Phase 3 `DiegeticPanel`-removal teardown contract rather than hand-maintaining a second incomplete cleanup list. Reveal again only after fresh layout and placement readiness; never respawn the controller merely because its blueprint changed.
- **Defaults and presentation:** `show_after` defaults to 500 ms, `hide_after` to zero, and disabled policy to `Suppress`; anchors and offset also have defaults. `widgets/presets/tooltip.rs` supplies the default panel presentation. Rich content is ordinary panel content; overflow avoidance is out of scope.

**Files:**
- `src/widgets/tooltip.rs` — state machine, visibility events, materialization, rebuild, and readiness
- `src/widgets/mod.rs` — tooltip behavior order and materialization deferred fence
- `src/widgets/presets/tooltip.rs` — default tooltip panel presentation
- `src/panel/diegetic_panel.rs` — reuse the settled panel-role teardown/reset path
- Read-only: `crates/hana_valence` resolver and attachment graph, `src/panel/anchoring.rs`, `src/screen_space/anchoring/`

**Constraints from prior phases:** Phase 10 supplies stable controller identity, `TooltipFor`/`Tooltips`, template diff classification, and `TooltipSystems::ControllerCommandsApplied`. Phase 5 supplies `WidgetFocused` and `WidgetSystems::InteractivityCommandsApplied`; Phase 3 supplies `PickingInteraction` and the owner-panel teardown contract; Phases 4 and 4.5 supply world/screen widget target geometry and demand. Phase 7.5 supplies retained visual infrastructure for the default preset.

**Acceptance gate:** `cargo nextest run` green with new tests: show wait cancels; hide grace cancels on renewed eligibility and `Duration::ZERO` hides immediately; first show materializes hidden and reveals only after layout+placement, including zero delay and `Fit`; every later hide/show preserves the same entity and resident panel/render state; a new blueprint resets every old panel/widget/text/render/placement component and rebuilds hidden on that same controller, while an identical clone does nothing and a policy-only change does not rebuild; `Suppress` immediately hides an already-visible tooltip; `TooltipShown` and `TooltipHidden` target the tooltip entity exactly once per completed visibility edge, emit nothing for canceled waits or redundant states, expose ready transforms on show, and emit `TooltipHidden` before visible-controller cleanup; retargeting updates both semantic and materialized placement targets; visible associated removal and target despawn emit one hidden event before cleanup, while hidden cleanup emits none; standalone and associated tooltips share the path; world and screen targets inherit the correct space; a world widget tooltip honors nonzero offset.

### Phase 12 — Demonstration checkpoint (stop and discuss)  · status: todo

#### Work Order

**Goal:** Decide, with the project owner, how to demonstrate the widget system. This phase is a discussion checkpoint, not delegated implementation.

**Spec:**
- Stop after Phase 10.5 and design the demonstration plan together: which existing examples change, which new examples are added.
- The plan must prove the pieces work together in real diegetic UI: buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and existing IME/text input coexisting on one panel.

**Files:** none until the discussion lands.

**Constraints from prior phases:** All widget subsystems through Phase 10.5 complete and are tested headlessly (`HeadlessLayoutPlugin`/minimal-app tests) before demonstration work begins.

**Acceptance gate:** A written demonstration plan agreed with the project owner; no code gate.

## Team review record — 2026-07-15

### Accepted (auto-recorded)

- **F1 — Same-frame widget and attachment ordering · accepted.** Reify semantic widget entities/rects in `Update` with explicit fences; model demanded widgets as resolver dependencies of their owning panels so anchored-panel→widget→dependent chains use current transforms in both world and screen graphs.
- **F2 — Preset state lacked a render-data update path · accepted.** Stable visual slots and widget-owned overrides now patch retained batch records without relayout or computed-panel mutation.
- **F3 — Pointer capture could not enforce one terminal path · accepted.** Per-pointer ownership, typed terminal state, raw pointer-loss handling, and finalize-before-despawn make button/slider completion structural.
- **F4 — Computed widget records and stale cleanup were unspecified · accepted.** Phase 1 now owns record production, visual-only refresh, id lookup, stale sweep, runtime validation, current preorder, and same-id kind transitions.
- **F5 — Separate picking backends could not order widget over panel · accepted.** One diegetic backend emits the panel and widget hits together and excludes the interaction mesh from generic mesh picking.
- **F6 — Picking omitted clipping, overlap, camera, visibility, and layer rules · accepted.** Computed clipped bounds/rank and mesh-compatible ray filters are now required and tested.
- **F7 — The plan referenced an unimplemented surface projection · accepted.** Phase 3 is explicitly flat with one shared conversion boundary; curved support is gated on `surface-panels.md` Phase 5.
- **F8 — Interactivity represented inheritance twice and exposed a first-frame input gap · accepted, refined by PM2.** The value enum is concrete, storage-independent `bevy_kana::Cascade` authors both layout and ECS segments, the folded tree value reifies onto the widget's one cascade component, and the ordered fence seeds the final resolved state before picking.
- **F9 — Focus policy, authority, scope, and traversal conflicted · accepted.** One per-window authority, panel-local computed preorder, retained disabled focus, typed transitions, IME gating, and adapter ordering replace the contradictory rules.
- **F10 — Widget id lookup had no entity-side owner · accepted.** `PanelWidget` owns the entity→id direction, while `PanelWidgetReader` provides the panel-qualified `(panel, id) → live entity` bridge for app-initiated entity-targeted control. Entity events already supply their target and skip lookup; `PanelElementId` remains unchanged.
- **F11 — Retained `.on_click(closure)` could not register a system · accepted.** A typed tracked callback template defers world registration to reify and owns cleanup while preserving the promised call.
- **F12 — Widget anchor offsets and lazy-demand teardown were missing · accepted.** Widget-aware target metrics, current transforms, multiplicity-aware reverse relations, retargeting, and last-demand retirement are required.
- **F13 — Tooltip timing and first reveal were contradictory · accepted.** One phase enum defines show/hide grace, zero duration, suppress behavior, hidden first panel materialization, and layout+placement readiness.
- **F14 — Consuming widget clicks bypassed IME blur · accepted.** Widget click handling classifies blur through the owning panel before stopping propagation.
- **F15 — Nested and precomposed interaction had no valid ownership/order model · accepted.** The first API rejects those forms while retaining arbitrary non-interactive child layout.
- **F16 — Authoring files and the Phase 4 screen gate were wrong · accepted.** Work moved to `layout/builder.rs`/`layout/element.rs`; the post-Phase-2 review subsequently placed screen-demand work immediately after world geometry as Phase 4.5.
- **F17 — `Hovered` was mouse-only and added a linear scan · accepted.** Widget presentation now uses all-pointer `PickingInteraction`; pointer-specific behavior retains explicit `PointerId` state.
- **F18 — The public widget contract was not frozen · accepted.** Exact authoring signatures, validation errors, identity/focus/button payloads and causes, adapter installation, export lists, and private implementation types were fixed; slider and tooltip details were routed to PD1 and PD2.

## Owner decisions

### PD1 — Slider applied-value contract

- **Severity:** important
- **Source dimension:** type-system/changeability + architecture/ergonomics consensus
- **Class:** design-improvement
- **Decision:** Use a private-field `SliderState` component containing validated `SliderRange`, applied raw-domain value, optional validated `SliderStep`, and direction. Expose validated construction/read/write APIs; emit controlled change proposals that the app explicitly accepts or rejects; apply `Slider::initial_value` only on first spawn and preserve live value across same-id reuse.
- **Bevy follow-up:** confirmed against local `bevy_ui_widgets 0.19.0` and `../bevy` `0.20.0-dev`; adopted self-update and absolute/relative/relative-step request parity while retaining Hana's stricter validation and diegetic interaction model.
- **Status:** accepted — 2026-07-15

### PD1a — Public widget authoring names

- **Decision:** Public authoring uses `Button` and `Slider`, yielding `.button(id, Button::new())` and `.slider(id, Slider::new(range, initial_value)?)`. The `Spec` suffix remains only in crate-private `WidgetSpec`; runtime slider data remains unambiguously `SliderState`.
- **Reason:** there is no competing public runtime `Button` or `Slider` component, so the former `Spec`-suffixed names exposed implementation vocabulary without adding useful disambiguation.
- **Status:** accepted — 2026-07-15

### PD2 — Tooltip relationship and retained panel template

- **Severity:** important
- **Source dimension:** architecture/ergonomics + type-system/changeability consensus
- **Class:** design-improvement
- **Relationship decision:** tooltips are separate lightweight entities with `TooltipFor(target)` / `Tooltips`, never fields inside `Button`, `Slider`, or `WidgetSpec`. Associated declarations reify a controller related to the widget; standalone tooltips use the same relation directly. First eligibility materializes the controller into an anchored panel, inheriting the target's coordinate space. The linked-spawn relationship target owns cleanup.
- **Relationship status:** accepted — 2026-07-15
- **Template decision:** the tooltip entity stores public `TooltipTemplate`, which contains an immutable concrete panel blueprint behind an `Arc` plus private timing/disabled-policy values. Associated authoring is `.tooltip(template)`; standalone authoring inserts `(template, TooltipFor::new(target))`. The consuming `.show_after(...)`, `.hide_after(...)`, and `.disabled_policy(...)` builders override defaults, so no separate public `Tooltip` policy component or `.tooltip_with_policy(...)` call exists. Panel-blueprint pointer identity and policy values define equality; a policy-only change does not rebuild the panel. After first materialization, show/hide retains the same entity and panel; a new panel blueprint rebuilds hidden on that entity rather than respawning it. The crate context makes the panel nature implicit, so the public name does not repeat `Panel`.
- **Template status:** accepted — 2026-07-15
- **Event decision:** emit non-propagating `TooltipShown` and `TooltipHidden` entity events targeted at the tooltip entity exactly once per actual visibility transition. The shown event observes a ready transform; the hidden event precedes any cleanup so effects can still query the entity and its target relation.
- **Event status:** accepted — 2026-07-15
- **Timing decision:** `TooltipTemplate::new(...)` defaults `show_after` to 500 ms and `hide_after` to zero; the named builders override either duration independently.
- **Timing status:** accepted — 2026-07-15
- **Status:** accepted — 2026-07-15

### PM1 — Phase 2 refresh after shared-cascade merge

- **Trigger:** `0f10f15d feat: move cascade engine into bevy_kana` replaced Hana's former `Override<A>` / hierarchy-walk implementation with shared `Cascade<A>`, explicit `CascadeFrom`, and `Resolved<A>` propagation.
- **Decision:** Superseded by PM2; the private fallback plus independently persistent widget-entity override introduced a second authority not used by other layout-derived entities.
- **Status:** superseded — 2026-07-17

### PM2 — One tree-authoritative widget-interactivity cascade

- **Trigger:** Phase 2's pre-dispatch review compared its proposed virtual fallback with existing panel, element, and reified-text cascade ownership.
- **Decision:** `Cascade<WidgetInteractivity>` follows one logical global → ECS ancestors/panel → layout ancestors → widget-El chain. Layout traversal folds its non-entity segment into the computed record; reify synchronizes that authored `Cascade` onto the widget entity; `CascadeFrom(panel)` and Bevy Kana's normal `Resolved` complete the ECS segment. The `LayoutTree` remains authoritative, matching `PanelText::set_style`; no second runtime-authoring layer or custom precedence resolver exists.
- **Mutation boundary:** `PanelWidgetWriter` applies durable widget-local override/inherit changes to the authored widget El, using the event target entity or one obtained from `PanelWidgetReader`. Panel and explicit ECS-ancestor changes continue through typed `CascadeEntityCommandsExt` verbs. Interactivity-only tree edits remain visual-only and do not recompute geometry.
- **Public boundary:** Hana exposes the value, marker, reader/writer, and typed verbs without re-exporting raw `Cascade<T>` or `Resolved<T>` storage.
- **Status:** accepted — 2026-07-17

### Dropped proposals

- **DP1 — Remove the button authoring value entirely · dropped.** Public naming is now `.button(id, Button::new())`, but the second argument remains the home for optional retained button behavior such as `.on_click(...)`.
- **DP2 — Move `PanelElementId` out of the IME module · dropped.** The crate-root API is already stable and the move is unnecessary for widget correctness; `PanelWidget` supplies the missing entity ownership.
