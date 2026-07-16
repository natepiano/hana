# Headless Widgets

> **Status: IMPLEMENTATION PLAN — phased; ready for delegation.** Adds headless widgets (buttons, sliders, tooltips, focus, interactivity) to `hana_diegetic`: widgets own semantic behavior and typed events, visuals stay ordinary layout primitives, widgets materialize as panel child entities targeted by Bevy picking, and anchoring comes from `hana_valence`. Phases 0–11 have fixed contracts and normal prerequisite ordering; Phase 12 remains the required demonstration-design stop.

## Delegation Context

- **Project** — `hana_diegetic` (workspace member at `crates/hana_diegetic`). Diegetic UI layout engine for Bevy — in-world panels driven by a Clay-inspired layout algorithm. This plan adds a headless `widgets` module that materializes widgets as panel child entities.
- **Stack** — Rust (edition 2024). Bevy `0.19.0` (workspace pin, `crates/hana_diegetic/Cargo.toml:14`). `bevy_picking` + `mesh_picking` features are already enabled; widget presentation reads the all-pointer `bevy_picking::PickingInteraction` aggregate, and one diegetic picking backend owns the ordered panel+widget hit group. `bevy_enhanced_input` `0.26.0` is a workspace dependency and becomes a direct `hana_diegetic` dependency in Phase 5. `hana_valence` is a workspace path dep (`Cargo.toml:43`). No bevy_ui.
- **Layout** (only phase-touched paths):
  - `crates/hana_diegetic/src/widgets/` — NEW module: `mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `id.rs`, `relationship.rs`, `interactivity.rs`, `focus.rs`, `input.rs`, `picking.rs`, `reify.rs`, `visual.rs`, `presets/` (`mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `style.rs`).
  - `crates/hana_diegetic/src/layout/` — `builder.rs`, `element.rs`, and the engine output that produces widget records and visual-slot references.
  - `crates/hana_diegetic/src/ime/` — `activation.rs`, `field.rs`, `ids.rs`, `mod.rs` (`ImePlugin`).
  - `crates/hana_diegetic/src/panel/` — `builder.rs`, `anchoring.rs`, `anchor_geometry.rs`, `arrangement.rs`, `diegetic_panel.rs`, `valence_provider.rs`, `perf.rs`.
  - `crates/hana_diegetic/src/render/` — `panel_geometry.rs`, the batch-record update paths used by visual overrides, and `panel_text/` (`reconcile.rs`, `relationship.rs`, `mod.rs`).
  - `crates/hana_diegetic/src/screen_space/anchoring/` — `candidate.rs`, `placement.rs`, `projection.rs`, `rect.rs`, `resolve.rs`, `window.rs`, `mod.rs`.
  - `crates/hana_diegetic/src/cascade/` — `attributes.rs`, `resolved.rs`, `mod.rs`; diegetic attribute defaults and typed public verbs over the shared engine.
  - `crates/bevy_kana/src/cascade.rs` — read-only shared `Cascade<T>` authoring, `CascadeFrom` relationship, propagation, and `Resolved<T>` cache.
  - `crates/hana_diegetic/src/lib.rs` — curated public re-exports.
- **Key files:**
  - `src/layout/builder.rs` + `src/layout/element.rs` — `El`, `CommonEl`, `Element`, `LayoutTree`, exhaustive tree-change classification, element-id traversal, clipping, precomposition, and the actual homes of `.button`/`.slider` authoring data.
  - `src/panel/builder.rs` — panel builder and `PanelBuildError`; `build()` calls `tree.duplicate_named_element_id()`. Widget ids reuse this validation path, while runtime tree replacement calls the same validator before accepting a tree.
  - `src/render/panel_text/reconcile.rs` — text reify: `reconcile_panel_text_children` (rename target), `update_reused_panel_text_child` (`:419`, the reuse-on-diff pattern widget reify mirrors).
  - `src/render/panel_text/relationship.rs` — `TextRunOf` / `PanelTextRuns` (template for `WidgetOf`/`PanelWidgets`; no `linked_spawn`).
  - `src/render/panel_text/mod.rs` — text-child ordering in `PanelChildSystems::Build`; widget semantic reify does **not** copy that `PostUpdate` schedule because screen attachment resolution needs widget entities and rects during `Update`.
  - `src/ime/activation.rs` — IME double-click activation observer: `On<Pointer<Click>>` gated `click.count < 2` (`:28`); calls `computed.field_at_local_position(panel_local)` (`:39`).
  - `src/panel/diegetic_panel.rs` — `field_at_local_position(&self, panel_local: Vec2) -> Option<&PanelFieldRecord>` (`:1591`); panel-local record-lookup pattern for the picking backend.
  - `src/ime/ids.rs` — id types; `PanelElementId::auto` (`:64`). Widget ids land in this element-id namespace; no new `WidgetId` newtype.
  - `src/ime/mod.rs` — `ImePlugin` (`pub(crate)` `:70`, `impl Plugin` `:89`); mirror for `WidgetsPlugin`.
  - `crates/bevy_kana/src/cascade.rs` — `Cascade<A>`, `CascadeFrom` / `CascadeChildren`, `CascadeDefault<A>`, `CascadePlugin<A>`, `Resolved<A>`, and `CascadeSet::Propagate`. `ChildOf` is deliberately unrelated to cascade inheritance.
  - `src/cascade/mod.rs`, `attributes.rs`, and `resolved.rs` — private shared-engine imports plus diegetic attribute root defaults, typed `override_*` / `inherit_*` commands, and resolved readers. `hana_diegetic` does not re-export raw `Cascade<T>`.
  - `src/panel/anchor_geometry.rs` — read-only panel geometry API: `PanelAnchorGeometryParam`, `PanelScreenBounds`, `PanelPlane`, and `ResolvedPanelAnchorGeometry`. `src/panel/valence_provider.rs` is the world-panel provider for the `hana_valence::ResolvedAnchorGeometry` component that widgets also publish (see `../hana_valence/as-built/anchoring-and-arrangements.md`).
  - `src/panel/anchoring.rs` — insert-only `AnchoredToPanel` authoring, private `PanelAttachmentAuthored`, world-only lowering to `hana_valence::AnchoredTo`, offset lowering, and `PanelSpace` reconciliation. Screen panels keep the shared authoring without the world relation.
  - `src/render/panel_geometry.rs` — current flat `PanelInteractionMesh`; Phase 3 moves it out of the generic mesh backend and makes the diegetic backend emit the panel and widget hits together.
  - `src/screen_space/anchoring/candidate.rs` + `resolve.rs` — screen placement builds candidates from private `PanelAttachmentAuthored` and delegates ordering and diagnostics to `hana_valence::resolve_attachments`; it accepts panel targets only today, and Phase 11 teaches it widget targets.
  - `src/panel/perf.rs` — `DiegeticPerfStats` (`:45`), `pub reconcile_ms: f32` (`:54`, rename target), `DIAG_PANEL_RECONCILE_MS` (`:258`).
  - `src/render/mod.rs` — `PanelChildSystems` set enum (`:128`); `TextRunOf`/`PanelTextRuns` re-exports.
  - `src/lib.rs` — curated re-exports (`PanelBuildError` `:255`); widget public types re-export here.
- **Build:** `cargo build && cargo +nightly fmt` after changes.
- **Test:** `cargo nextest run` (never `cargo test`).
- **Lint:** the `clippy` skill. Workspace lints are strict: `all`/`cargo`/`nursery`/`pedantic` denied, `unwrap_used`/`expect_used`/`panic`/`unreachable` denied, `missing_docs = "deny"`, `self_named_module_files` denied (use `module/mod.rs` directory form).
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/hana_diegetic_widgets`
- **Invariants:**
  - **Valence gate:** `hana_valence` exists at `crates/hana_valence`; its resolver, panel bridge, and screen-adapter integration are described in `../hana_valence/as-built/anchoring-and-arrangements.md`. Hana Valence types stay out of diegetic's public widget signatures. Diegetic authoring lowers to `hana_valence::AnchoredTo` only for world sources; screen sources retain `PanelAttachmentAuthored` and use the shared attachment graph without carrying the world relation.
  - No bevy_ui / bevy_a11y dependency. `WidgetDisabled`, `WidgetFocused`, and pointer-capture state stay bespoke; `PickingInteraction` supplies all-pointer hover/press presentation and `bevy_enhanced_input` supplies the opt-in semantic-action adapter.
  - Widgets materialize as panel child entities under `ChildOf(panel)`; the `WidgetOf`/`PanelWidgets` relationship is a traversal index only, no `linked_spawn` — `ChildOf` owns despawn.
  - Behavior modules never construct layout/render primitives (`El`, `LayoutTree`, `PanelDraw`, materials, `TextStyle`, `DrawZIndex`). Presets depend on behavior, never the reverse.
  - No relayout on hover/press/focus/disabled/value flips. Presets author stable private visual-slot ids; changed widget state patches only those slots' retained batch records through widget-owned override components. It never regenerates the `LayoutTree` or writes `DiegeticPanel`/`ComputedDiegeticPanel` merely to restyle a widget.
  - Widget semantic reify runs in `Update`, after `PanelSystems::ComputeLayout` and before cascade propagation and `PanelSystems::ResolvePanelAttachments`, with an explicit `ApplyDeferred` fence. Render-child batching remains in `PostUpdate`.
  - Change-gated systems, never unconditional per-frame walks: reify is gated on `Changed<ComputedDiegeticPanel>` and reuses entities by id; interactivity writes `WidgetDisabled` only on diff; anchor geometry exists only while world or screen demand is nonempty and is removed after the last demand ends.
  - Widget ids reuse `PanelElementId` and its `duplicate_named_element_id` → `DuplicateElementId` validation; event-emitting widgets require `Named` ids (auto ids reposition on structural edits and would fire spurious cancels).
  - Widget interactivity uses `bevy_kana::Cascade<WidgetInteractivity>` and `Resolved<WidgetInteractivity>`. `Cascade::Inherit` participates and follows explicit `CascadeFrom`; an absent `Cascade<A>` means non-participation. `WidgetInteractivity` itself has no inherited variant. Hana exposes typed domain verbs and does not re-export raw cascade storage.
  - Widget events derive `EntityEvent` targeting the widget entity; the panel-local id is a payload convenience only, never the routing key. Owning panel resolves through `WidgetOf`, never duplicated on components or events.
  - Widget picking geometry stays in **panel-local space**. The first implementation uses the current flat interaction-mesh hit conversion. Curved-panel support is gated on Phase 5 of `surface-panels.md`, which replaces that one boundary with `PanelSurface::project()`; widget rectangle tests remain unchanged and never place geometry independently in world space.
  - The first API rejects interactive descendants inside a widget and widgets inside precomposed subtrees. Arbitrary non-interactive child layout remains valid; nested/precomposed interaction needs a later ownership and hit-order design.
  - Tooltip authoring is separate from `Button`/`Slider` authoring. Reify creates a lightweight tooltip entity with `TooltipFor(target)`; first eligibility materializes that same entity into a hidden anchored panel. The semantic relationship exists before the placement relationship and does not itself create anchor-geometry demand.
- **Public contract ledger (fixed before delegation):**
  - Authoring methods are `El::button(self, id: impl Into<PanelElementId>, button: Button) -> Self` and `El::slider(self, id: impl Into<PanelElementId>, slider: Slider) -> Self`. Both assign the element id and crate-private widget variant atomically. `Button` is a private-field `Clone + Debug + PartialEq + Default` authoring builder with `new()` and Phase 7's `on_click(...)`; `Slider` is a private-field `Clone + Debug + PartialEq` validated authoring builder with no `Default` because range and initial value are required. Neither is an ECS component. Crate-private `WidgetSpec` is exactly `Button(Button) | Slider(Slider)`, while runtime slider data lives in `SliderState`, so no public `Spec` suffix is needed.
  - New validation variants are `PanelBuildError::WidgetRequiresNamedId(PanelElementId)`, `WidgetContainsInteractiveDescendant(PanelElementId)`, and `WidgetInsidePrecomposedSubtree(PanelElementId)`.
  - Identity exports are `PanelWidget`, `PanelWidgetReader`, `WidgetOf`, and `PanelWidgets`. `PanelWidget` exposes only `id()`, and `WidgetOf` exposes only `panel()`; relationship mutation remains internal.
  - Interactivity/focus exports are `WidgetInteractivity`, `WidgetDisabled`, `WidgetFocusable`, `WidgetFocused`, `RequestWidgetFocus`, `ClearWidgetFocus`, `WidgetFocusChanged`, and `WidgetFocusChangeCause`. Element authoring is `El::widget_interactivity(self, value: WidgetInteractivity) -> Self`. The existing `CascadeEntityCommandsExt` gains `override_widget_interactivity(value)` and `inherit_widget_interactivity()` for panel- or widget-entity authoring; `Cascade<T>` remains owned by `bevy_kana` and is not re-exported. The focus request payload is `{ window, widget }`, clear is `{ window }`, and change is `{ window, previous, current, cause }`. Cause variants are `Pointer`, `Traversal`, `Semantic`, `Application`, `ExplicitClear`, `WidgetRemoved`, `FocusabilityRemoved`, and `ScopeLost`; disable is intentionally absent.
  - The six exported semantic action types are `FocusNextWidget`, `FocusPreviousWidget`, `FocusFirstWidget`, `FocusLastWidget`, `ActivateFocusedWidget`, and `CancelFocusedWidget`. The adapter exports `WidgetInputPlugin`, `WidgetInputBindings`, and `WidgetControlSummary`; a complete install is `app.add_plugins(WidgetInputPlugin::new(WidgetInputBindings::default()))`, after which the plugin owns one context entity per window and reconciles install/rebind/remove idempotently.
  - Button exports are `Button`, `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, `ButtonCanceled`, `ButtonCancelCause`, `ButtonPreset`, and `ButtonStyle`. Every event has `entity` as its `#[event_target]` plus `id: PanelElementId`; pressed/released add `pointer_id: PointerId`, clicked adds `pointer_id: Option<PointerId>` (`None` for semantic activation), and canceled adds `pointer_id: PointerId` plus `cause`. `ButtonCancelCause` is exactly `PointerCanceled | PointerRemoved | CaptureLost | Disabled | WidgetRemoved | WidgetKindChanged | Explicit`.
  - Slider exports are `Slider`, `SliderState`, `SliderRange`, `SliderStep`, `SliderDirection`, `SliderConfigError`, `SliderGrabbed`, `SliderChangeRequested`, `SliderReleased`, `SliderCanceled`, `SliderCancelCause`, `RequestSliderAdjustment`, `SliderAdjustment`, `slider_self_update`, `SliderPreset`, and `SliderStyle`. Phase 10 appends `TooltipTemplate`, `TooltipFor`, `Tooltips`, `TooltipDisabledPolicy`, `TooltipShown`, `TooltipHidden`, and `TooltipPreset`.
  - Tooltip construction is `El::tooltip(self, template: TooltipTemplate) -> Self` for associated authoring and `commands.spawn((template, TooltipFor::new(target)))` for standalone authoring. `TooltipFor::new(target)`, `target()`, and `retargeted(target)` are public; `Tooltips::iter()` exposes reverse membership; mutation is maintained by Bevy relationship hooks; and `Tooltips` uses `linked_spawn` to despawn related tooltip controllers with their target.
  - `WidgetsPlugin`, `WidgetSpec`, `WidgetKind`, computed records, id/order maps, callback templates/handles, capture/terminal state, the virtual-layout fallback component, visual-slot ids/overrides, anchor bridges/geometry, tooltip phases/timers, and screen dependency relations remain crate-private. Raw `Cascade<T>` / `Resolved<T>` storage remains `bevy_kana` machinery rather than widget API.

## Phases

### Phase 0 — `reify` terminology rename  · status: todo

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

### Phase 1 — Widget identity, authoring, relationship, reify, plugin skeleton  · status: todo

#### Work Order

**Goal:** Widgets can be authored in a panel's element tree, represented in computed output, and materialize as reused, relationship-indexed panel child entities with a stable id lookup.

**Precondition (verify before starting):** the shipped Hana Valence resolver,
world-panel provider, `AnchoredToPanel` lowering, and screen attachment adapter
still match the Valence gate invariant. If that contract has changed, reconcile
this plan before implementing widgets.

**Spec:**
- **Ids and validation** (`widgets/id.rs` + `layout/element.rs`): widget ids ARE `PanelElementId` — no newtype. Event-emitting widgets require `Named` ids; the exact `PanelBuildError` variants are fixed in the public contract ledger. Duplicate rejection reuses `duplicate_named_element_id` → `PanelBuildError::DuplicateElementId`. A single tree validator runs from both `DiegeticPanelBuilder::build` and runtime tree replacement, and also rejects interactive descendants of widgets and widgets under `PrecomposeMode`; invalid replacement leaves the current tree intact and reports the typed error.
- **Authoring** (`layout/builder.rs` + `layout/element.rs`): the exact `El::button` signature and private/public type split are fixed in the public contract ledger; this is a config method mirroring `.editable_field(id, spec)`, NOT an `El::button` constructor and NOT a `LayoutBuilder` leaf. `CommonEl::widget: Option<WidgetSpec>` is carried onto `Element` parallel to `editable`, through every constructor/clone/destructure path. `LayoutTree::classify_change` treats a widget-record-only edit as `VisualOnly`, and the visual-only commit path refreshes computed widget records. Phase 10 adds a separate tooltip declaration field; it is never nested inside `WidgetSpec`.
- **Computed record:** layout output owns one crate-private `ComputedWidgetRecord` per valid widget: `PanelElementId`, `WidgetKind`, current computed-tree preorder, and the authored `Button` or `Slider` snapshot. Phase 3 adds the panel-local/clipped rect and interaction rank; Phase 7 adds visual-slot references. `ComputedDiegeticPanel` exposes the crate-private record slice consumed by reify even on a visual-only tree update.
- **Identity and lookup:** each entity carries public read-only `PanelWidget { id: PanelElementId }` plus `WidgetOf(panel)`. `PanelWidgets` remains the Bevy-maintained membership set; a public `PanelWidgetReader::entity(panel, id)` reads a panel-local map rebuilt during reify. Callers disambiguate identical ids on different panels with `(panel, id)` or use the event target entity.
- **Relationship** (`widgets/relationship.rs`): `WidgetOf` / `PanelWidgets`, modeled on `TextRunOf`/`PanelTextRuns` (`src/render/panel_text/relationship.rs`). No `linked_spawn` — widgets sit under `ChildOf(panel)`, which owns despawn; the relationship is a membership index, not the focus-order source. Phase 2 separately inserts `CascadeFrom(panel)` because `ChildOf` and `WidgetOf` do not imply cascade inheritance.
- **Reify** (`widgets/reify.rs`): a change-gated system walking `Changed<ComputedDiegeticPanel>`. It reuses entities by panel-local id, writes components only on diff, rebuilds the id map and current preorder, and sweeps every unvisited entity. Same-id/same-kind updates preserve interaction state; a kind change first finalizes/cancels the former lifecycle and removes its complete behavior/callback bundle before installing the new kind while retaining entity identity. Widget removal finalizes live state before despawn.
- **Schedule and plugin** (`widgets/mod.rs`): `WidgetsPlugin` (`pub(crate)`, mirror `ImePlugin`) defines `WidgetSystems::Reify` in `Update`, after `PanelSystems::ComputeLayout` and before `bevy_kana::CascadeSet::Propagate` and `PanelSystems::ResolvePanelAttachments`. An explicit `ApplyDeferred` fence between reify and propagation makes newly inserted `Cascade` / `CascadeFrom` state visible to the shared engine. Do not put semantic widget reify in `PanelChildSystems::Build`; that `PostUpdate` timing is too late for same-frame screen targets. Register the plugin where `ImePlugin` is registered.
- **Module structure:** private `widgets` module next to `ime`; curated public types re-exported from `lib.rs`/`widgets/mod.rs`, never the whole tree.

**Files:**
- `src/widgets/mod.rs`, `src/widgets/id.rs`, `src/widgets/relationship.rs`, `src/widgets/reify.rs` — new
- `src/layout/builder.rs`, `src/layout/element.rs` — `.button(id, Button::new())`, `common.widget`, validation, tree diffing
- `src/layout/engine/` + `src/panel/compute_layout.rs` + `src/panel/diegetic_panel.rs` — computed widget records and visual-only refresh
- `src/panel/builder.rs` — `PanelBuildError` integration and shared validation call
- `src/lib.rs` — re-exports + plugin registration site
- Read-only templates: `src/render/panel_text/relationship.rs`, `src/render/panel_text/reify.rs`, `src/ime/mod.rs`

**Constraints from prior phases:** Phase 0 renamed `reconcile_panel_text_children` → `reify_text_entities` and `reconcile.rs` → `reify.rs`.

**Acceptance gate:** `cargo nextest run` green with new tests: duplicate widget id rejected via `DuplicateElementId`; auto id, nested interactive content, and precomposed widgets rejected by typed errors at build and runtime replacement; visual-only `Button`/`Slider` authoring changes refresh computed records; reify creates `PanelWidget` entities under `ChildOf(panel)` with relationship and id lookup; structural reorder keeps entities but rebuilds current preorder; removing one widget sweeps it while the panel survives; same-id kind replacement cancels/removes old behavior; panel despawn drops all widgets without double-despawn.

### Phase 2 — Interactivity resolution  · status: todo

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
  `WidgetInteractivity` has no `Inherit` variant. ECS authored state is `bevy_kana::Cascade<WidgetInteractivity>`: `Cascade::Inherit` keeps the entity participating and continues through `CascadeFrom`, while absence of the component means it does not participate. `WidgetDisabled(())` is the final derived presence marker with a private field, queried through `Has<WidgetDisabled>` and not constructible by callers. No `ResolvedWidgetInteractivity`, `enabled: bool`, or disabled-reason type.
- **Shared ECS cascade:** `WidgetsPlugin` installs `CascadePlugin::new(WidgetInteractivity::Enabled)`. Reify inserts `Cascade::<WidgetInteractivity>::Inherit` only when a widget lacks authored state, so same-id reuse never overwrites a runtime override, and keeps `CascadeFrom::new(panel)` synchronized independently of `ChildOf` and `WidgetOf`. The shared `Resolved<WidgetInteractivity>` therefore represents the widget-local override when present, otherwise the owning panel's override, any explicit cascade ancestors, or the global `CascadeDefault`. The existing public `CascadeEntityCommandsExt` gains `override_widget_interactivity(value)` and `inherit_widget_interactivity()`; both panels and widget entities use those domain verbs instead of a Hana re-export of raw `Cascade<T>`.
- **Precedence and virtual layout layer:** final precedence is runtime widget `Cascade::Override` → most-specific virtual layout-subtree override → shared panel/global result. A child layout override of `Enabled` inside a disabled panel is enabled; sticky ancestor disabling is rejected. No private cascade-scope entities are created for layout elements.
- **Layout-subtree authoring:** `CommonEl` stores `Cascade<WidgetInteractivity>`, defaulting to `Inherit`; `El::widget_interactivity(value)` authors `Override(value)`. Because layout Els are not entities, the compute walk folds the nearest override into `ComputedWidgetRecord`, and reify writes that derived virtual layer to a private `WidgetLayoutInteractivity` component. Reify updates this component on diff but never writes the widget entity's live `Cascade<WidgetInteractivity>`.
- **Resolver and first frame:** after `CascadeSet::Propagate` and an explicit deferred-command fence, `WidgetSystems::ResolveInteractivity` reads the widget's local authored `Cascade`, shared `Resolved`, and private virtual-layout layer. A local `Override` wins directly; only `Inherit` permits the virtual layer to override the shared panel/global result. The resolver is input-change-gated and inserts/removes `WidgetDisabled` only on an actual effective-value edge. The ordered reify → flush → shared propagation → flush → interactivity-resolution path produces the marker in the widget's creation frame, before its first following `PreUpdate` picking pass.
- Disabled changes are visual/state-only by default: no layout recompute unless a preset explicitly opts into different content or dimensions.

**Files:**
- `src/widgets/interactivity.rs` — value type, private virtual layer, effective resolver
- `src/widgets/mod.rs` — shared plugin registration and ordered fences/system sets
- `src/widgets/reify.rs` — insert-if-missing authored state, explicit panel cascade relation, virtual-layer updates
- `src/cascade/attributes.rs`, `src/cascade/resolved.rs` — typed commands and `Enabled` root default
- `src/layout/builder.rs`, `src/layout/element.rs`, `src/layout/engine/` — El-level `Cascade` authoring and nearest virtual override in the computed record
- `src/lib.rs` — curated widget type re-exports; no `Cascade<T>` re-export
- Read-only: `crates/bevy_kana/src/cascade.rs`, `docs/bevy_kana/cascade.md`, `src/cascade/mod.rs`

**Constraints from prior phases:** Phase 1 built `widgets/reify.rs` (change-gated tree-walk, reuse by id), `WidgetSpec` on `common.widget`, and `WidgetsPlugin`/`WidgetSystems`.

**Acceptance gate:** `cargo nextest run` green with new tests: widgets carry explicit `CascadeFrom(panel)` and do not rely on `ChildOf`; global and panel override changes propagate through the shared cache; nearest virtual layout override wins over panel/global, including child `Enabled` inside a disabled panel; runtime widget override wins over the virtual layer, survives unrelated reify, and is revealed correctly by `inherit_widget_interactivity`; reify never rewrites live authored state; a newly reified disabled widget rejects input on its first picking frame; unchanged effective values do not rewrite `WidgetDisabled` across propagation or relayout.

### Phase 3 — Widget `Transform`, single rect source, custom picking backend  · status: todo

#### Work Order

**Goal:** Widgets are first-class Bevy picking targets via a custom backend testing panel-local rects; pointer hover works on widget entities.

**Spec:**
- **Transform:** widgets carry a real panel-local `Transform` — translation = the widget's panel-local offset; `GlobalTransform` propagates via `ChildOf(panel)`. This is deliberately unlike text runs (which carry no `Transform`; their placement is baked into run records) — copying the text-run shape would break the picking backend and collapse anchor geometry to the panel origin.
- **Single rect source:** layout writes the widget's panel-local rect, effective ancestor-clipped rect, current computed-tree preorder, and interaction rank into `ComputedWidgetRecord` once. Picking bounds and Phase 4 anchor points project that record; no subsystem recomputes the rect with different invalidation triggers. Fully clipped widgets are not hit targets. Overlap order is deterministic: visual `DrawZIndex`, then source order, with a nested-interaction error from Phase 1 removing ambiguous ancestor/descendant targets.
- **One diegetic backend** (`widgets/picking.rs`): iterate Bevy's `(camera, pointer)` rays, apply the mesh backend's camera order, visibility, `RenderLayers`, `Pickable`, and render-target filters, and immediately raycast only `PanelInteractionMesh` entities. Test only `PanelWidgets` belonging to intersected panels. Emit the panel and all matching widgets in **one** ordered `PointerHits` group so widget depth is actually comparable with its panel; exclude panel interaction meshes from the generic mesh backend. Widget hits are slightly nearer than their panel and ordered against one another by the computed interaction rank.
- **Flat-now/surface-later boundary:** extract the current affine hit→panel-local conversion from `ime/activation.rs` into one shared flat projection helper. Phase 3 supports the currently shipped flat interaction mesh. Phase 5 of `surface-panels.md` later replaces that helper and mesh with `PanelSurface::project()` plus the curved interaction mesh; until then this plan makes no curved-panel picking claim.
- **Pointer presentation:** use Bevy's `PickingInteraction` aggregate for hover/pressed/none presentation across mouse, touch, stylus, and custom pointers. Do not insert `Hovered`: Bevy 0.19 updates it from `PointerId::Mouse` only and performs a linear scan of every entity carrying it. Pointer-specific capture still uses `PointerId` in Phases 6 and 8.

**Files:**
- `src/widgets/picking.rs` — new (backend)
- `src/widgets/reify.rs` — Transform + computed rect/rank writes
- `src/layout/engine/`, `src/render/clip.rs`, `src/render/draw_order.rs` — clipped bounds and interaction rank
- `src/render/panel_geometry.rs`, `src/ime/activation.rs` — owned panel raycast and shared flat conversion

**Constraints from prior phases:** Phase 1: widgets reified under `ChildOf(panel)`, reuse keyed by id, `WidgetSystems` set exists. Phase 2: `WidgetDisabled` marker exists (backend may still report hits on disabled widgets; behavior systems gate on the marker).

**Acceptance gate:** `cargo nextest run` green with new tests: pointer over a widget yields `Over`/`Out` on the widget; one hit group orders widget before panel; partial/full ancestor clipping gates hits; overlapping widgets follow `DrawZIndex` then source order; hidden, layer-mismatched, and non-pickable panels do not hit; two cameras preserve the originating camera and order; mouse and a non-mouse pointer update `PickingInteraction`; an off-origin widget picks at its actual location.

### Phase 4 — Lazy anchor-geometry publication  · status: todo

#### Work Order

**Goal:** Entities can anchor to widgets: diegetic publishes current `hana_valence` geometry and transforms only while a widget has attachment demand.

**Spec:**
- Publish `ResolvedAnchorGeometry` (the Hana Valence contract component) **lazily**. World demand is nonempty `AnchoredHere`; Phase 11 adds the screen relationship target. Fill on new demand or widget-rect change, and remove geometry after both world and screen demand become empty. Never publish on every widget and never use `Changed<Transform>` as the refill trigger.
- World publication runs in `AnchorSystems::FillGeometry` after Phase 1 reify commands are flushed and before `Resolve`. Reify owns the rect in `Update`; the geometry provider projects it in `PostUpdate` without rewriting it.
- Geometry points are projections of the Phase 3 single rect, expressed in the **widget-local frame** matching the panel provider's centered convention; the resolver composes `global_transform * geometry[anchor]`, which is why the widget's own `Transform` must carry its panel-local offset.
- **World resolver bridge:** ordinary transform propagation runs after valence resolution, and a widget's owner panel may itself move inside that same resolver pass. While a world widget has demand, add a private internal `hana_valence::AnchoredTo` bridge from the widget to its owning panel using the widget rect's current panel-local offset. The widget becomes a real resolver candidate only while demanded: graph order resolves an anchored owner panel first, writes the widget's current transform/`resolved_globals` entry second, then resolves sources targeting that widget. Remove the bridge with final world demand. This covers first spawn, parented panels, same-frame panel motion, and anchored-panel→widget→tooltip chains without resolving every widget every frame; no valence type enters the public widget API.
- **Offsets:** generalize `write_panel_anchor_offsets` around a private `AnchorTargetMetrics::{Panel, Widget}`. A widget target resolves its owning panel through `WidgetOf`, uses that panel's layout-unit conversion, and keeps nonzero x/y/z offsets under translation, rotation, and scale. Do not let the existing `Query<(&DiegeticPanel, &GlobalTransform)>` silently remove widget offsets.
- **Diagnostics:** use `AttachmentResolveDiagnostics`' source/target/reason key when an attachment names missing geometry or a despawned target. World failures already flow through `ResolveDiagnostics`; the screen adapter keeps its coordinate-space-specific reason type over the same bounded diagnostic mechanism.

**Files:**
- `src/widgets/reify.rs`, `src/widgets/relationship.rs` — rect ownership and demand transitions
- `src/widgets/mod.rs` — `AnchorSystems::FillGeometry` set membership
- `src/panel/anchoring.rs` — widget-aware offset lowering
- Read-only: `src/panel/valence_provider.rs` (centered provider convention), `crates/hana_valence` (contract types), `../hana_valence/as-built/anchoring-and-arrangements.md`

**Constraints from prior phases:** Phase 3 built the single panel-local rect source and gave widgets a real panel-local `Transform`. Phase 1 reify runs during `Update` and flushes before either screen or world attachment work.

**Acceptance gate:** `cargo nextest run` green with new tests: first-frame and same-frame panel motion place an attachment at an off-origin widget corner; an anchored-panel→widget→tooltip chain resolves in graph order in one pass; geometry and the private bridge are absent without demand, refill on rect change, and are removed after final world demand; two dependents keep them resident until both detach; nonzero pixel and physical-unit offsets survive transformed owning panels; missing-geometry diagnostics deduplicate by source, target, and reason. Screen-demand tests belong to Phase 11.

### Phase 5 — Focus subsystem  · status: todo

#### Work Order

**Goal:** Window-scoped keyboard/action focus works across all widgets with deterministic panel-local traversal and an opt-in enhanced-input adapter.

**Spec:**
- `widgets/focus.rs`. Focus is shared, not button-local. One crate-private authoritative resource maps each window to its active panel and focused widget; marker state is never an independent authority. The exact public request, clear, change, cause, and semantic-action types are fixed in the public contract ledger.
- `WidgetFocusable` participation component, inserted on materialized widget entities by default; removing it opts a widget out of keyboard traversal without changing pointer picking.
- `WidgetFocused(())` is a public read-only presence marker with a private field, synchronized only by one focus-transition function. Public app control uses typed request/clear events carrying the window and target; `WidgetFocusChanged` reports old/new entities and a cause.
- Focus is gained by pointer focus, traversal, semantic routing, or app request. It is lost by transfer, despawn/removal, `WidgetFocusable` removal, panel/window input-scope loss, or explicit clear — **not** by disable. Disabled focusable widgets may retain or receive focus and participate in traversal; behavior modules ignore activate/change input while disabled.
- Traversal order is the current `ComputedWidgetRecord` preorder rebuilt in Phase 1, never `PanelWidgets` relationship insertion order. Next/previous/first/last stay within the active panel for that window and wrap deterministically; focusing a widget on another panel transfers the active panel. Structural reorder changes traversal without respawning entities.
- Define the six semantic action types: next, previous, first, last, activate-focused, cancel-focused. Core focus requests/events do not depend on a binding library.
- **Enhanced-input adapter** (`widgets/input.rs`): add the direct workspace dependency and expose the opt-in plugin, bindings, and neutral control-summary types from the public contract ledger. `WidgetInputPlugin::new(bindings)` owns a context entity for each live window, diffs later binding changes, and removes its action/context entities on window removal or plugin-owned disable; repeating install/rebind/remove is a no-op. No raw key handling lives in widgets.
- **Ordering and IME:** pointer focus is visible before same-frame activate handling. Semantic widget input runs after `ImeSystemSet::PublishInputBlockers` and ignores a window while `ImeInputBlocker::blocks_window(window)` is true.
- Design with accessibility in mind (structure the traversal so an a11y layer can attach later), without adding bevy_a11y.

**Files:**
- `src/widgets/focus.rs` — new
- `src/widgets/input.rs` — semantic actions and enhanced-input adapter
- `src/widgets/mod.rs` — systems in `WidgetSystems` after picking
- `src/widgets/reify.rs` — default `WidgetFocusable` insertion
- `crates/hana_diegetic/Cargo.toml` — direct `bevy_enhanced_input` dependency
- Read-only reference: `crates/bevy_lagrange/src/input/`

**Constraints from prior phases:** Phase 2 supplies `WidgetDisabled`; Phase 3 supplies pick targets and `PickingInteraction`; Phase 1 supplies current traversal order. Activation of a focused button lands in Phase 6; this phase routes the action to that later behavior hook.

**Acceptance gate:** `cargo nextest run` green with new tests: next/previous/first/last and wrap order; structural reorder updates order while preserving entities; two windows hold isolated focus; app request/clear and change causes; focus loss on despawn, `WidgetFocusable` removal, and explicit clear; disabled widgets retain and can receive focus but activate is a no-op; pointer-focus plus activate works in one frame; IME blocks semantic actions only in its leased window; adapter install/rebind/remove is idempotent.

### Phase 6 — Button behavior  · status: todo

#### Work Order

**Goal:** Headless button with the four-event lifecycle, emulated pointer capture, semantic activation, and IME coexistence.

**Spec:**
- `widgets/button.rs`. `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, and `ButtonCanceled` derive `EntityEvent`; their exact fields, semantic-click representation, cause enum, and crate-root exports are fixed in the public contract ledger. No double-click event.
- **Lifecycle invariant** — a pressed button resolves to exactly one terminal path:
  - `Pressed -> Released -> Clicked` for a valid pointer click.
  - `Pressed -> Released` without `Clicked` for a valid release that no longer activates.
  - `Pressed -> Canceled` for capture loss, disable-while-pressed, despawn/removal, panel/tree replacement, pointer cancellation/removal, or explicit cancel.
  - Semantic activation emits `ButtonClicked` without entering the pointer lifecycle.
- **Emulated capture:** a private resource maps each occupied `PointerId` to one widget. A second press for an occupied pointer or on an already-captured widget is ignored. `ButtonPress` stores the pointer and a typed terminal state (`Pending`, `Release(outcome)`, `Cancel(cause)`); every global release/cancel/drag-end path matches both the captured pointer and widget before acting.
- **Terminal choke point:** set the terminal state before removing `ButtonPress`; one `On<Remove, ButtonPress>` observer emits `Released` plus optional `Clicked`, or `Canceled`. `Pending` removal is cancellation. Widget/kind removal runs finalization and targeted event dispatch while the entity still exists, then despawns/removes the behavior bundle. Do not queue an entity-targeted terminal event after its target is gone.
- **Pointer loss:** `Pointer<Cancel>` targets only currently hovered entities and cannot cover capture over empty space. Capture cleanup also consumes raw pointer cancellation/removal and uses `DragEnd` for drag-off release-in-void. Every path removes the capture-map entry exactly once.
- **Disable-while-pressed:** inserting `WidgetDisabled` on a pressed button must actively remove the live `ButtonPress` with a Canceled cause — a flag alone lets the pending Release/DragEnd resolve as Clicked. Disabled buttons ignore pointer and semantic activation and cannot keep capture.
- **Semantic activation:** a non-pointer path (keyboard shortcuts, action systems, or Phase 5 activate-focused routing) targeting the focused or an explicitly targeted button; emits `ButtonClicked` directly, no fabricated pointer events.
- **IME coexistence:** the Phase 3 ordered hit group makes the widget the event target. Before the widget stops click propagation, call a factored IME blur-classification helper with `WidgetOf::panel()` so clicking a button commits an editor outside that focus scope. Then stop propagation so the panel's double-click field activator cannot open a field underneath the button.
- Presentation state comes from `PickingInteraction`; no bespoke hover events in the first API.

**Files:**
- `src/widgets/button.rs` — new
- `src/widgets/mod.rs` — observers + systems registration
- `src/ime/editor.rs` — shared widget-aware blur classification
- Read-only: `src/ime/activation.rs`, `/Users/natemccoy/rust/bevy/crates/bevy_ui_widgets/src/button.rs`

**Constraints from prior phases:** Phase 1 reuses same-kind entities and finalizes removals/kind changes. Phase 2 supplies `WidgetDisabled`; Phase 3 supplies ordered hits and `PickingInteraction`; Phase 5 supplies activate-focused routing.

**Acceptance gate:** `cargo nextest run` green with new tests: press→release→click and release-without-click; pointer ids match every terminal path; a second pointer cannot terminate the first; cancel over empty space, raw pointer removal, drag-off release, disable-while-pressed, widget removal/despawn, same-id kind change, and explicit cancel each emit exactly one `ButtonCanceled`; semantic activation emits `ButtonClicked` alone; same-panel and other-panel button clicks classify IME blur correctly while a button over a field blocks field activation.

### Phase 7 — `.on_click` sugar + ButtonPreset  · status: todo

#### Work Order

**Goal:** Ergonomic click handling and a default button visual preset.

**Spec:**
- **Event consumption, base path:** app code can observe `ButtonClicked` globally and filter by event target, or resolve `(panel, PanelElementId)` through `PanelWidgetReader`. This ships alongside the sugar, not instead of it; id alone is never globally unique.
- **`.on_click` sugar:** preserve `.on_click(closure)` without requiring `LayoutBuilder` to access a `World`. `Button` stores a private, cloneable callback template: an `Arc`-owned wrapper around a typed `SystemHandleTemplate<In<ButtonClicked>, ()>`, compared by `Arc` identity so `WidgetSpec` stays comparable. World-aware reify builds one tracked `SystemHandle`, stores it on the widget, and a uniform observer calls `run_system_with` using the clicked event. Reuse never registers again; callback replacement drops the old strong handle, and final-handle drop lets Bevy clean up the registered system. Cost: one allocation per authored callback and reference-count operations when a tree clones, with no per-click allocation.
- **Runtime visual overrides** (`widgets/visual.rs`): ordinary fills, borders, images, and slider parts are retained render records, not ECS child entities. Presets assign stable private visual-slot ids to their `El`/`PanelDraw` primitives; layout output carries slot→record references into `ComputedWidgetRecord`. Widget entities own changed-only override components, and render batching patches only the referenced material/color/z/transform records and dirty GPU rows. Never mutate `DiegeticPanel` or `ComputedDiegeticPanel` for a state flip.
- **ButtonPreset / ButtonStyle** (`widgets/presets/button.rs`, shared helpers in `presets/style.rs`): helpers generate `LayoutTree` fragments and stable slots. Material-first: colors/images are convenience inputs resolving to `StandardMaterial`; custom shader cases use custom handles or `ExtendedMaterial`. Widget-specific names only. Presets read `PickingInteraction`, `Has<WidgetDisabled>`, `Has<WidgetFocused>`, and capture state, then write widget visual overrides. Rich content remains ordinary layout content.
- **Boundary guardrail:** presets depend on behavior, never the reverse; add a test/lint asserting behavior modules (`button.rs`, `focus.rs`, `interactivity.rs`, …) reference no layout/material types.

**Files:**
- `src/widgets/presets/mod.rs`, `src/widgets/presets/button.rs`, `src/widgets/presets/style.rs` — new
- `src/widgets/button.rs`, `src/layout/builder.rs` — typed callback template and `.on_click`
- `src/widgets/reify.rs` — tracked callback handle and uniform observer
- `src/widgets/visual.rs`, `src/layout/engine/`, render batch-record writers — visual slots and changed-row patching
- `src/lib.rs` — preset re-exports

**Constraints from prior phases:** Phase 6 defined typed click input and lifecycle; Phase 1 requires `WidgetSpec: Clone + PartialEq`; Phase 3 supplies `PickingInteraction` and deterministic interaction rank.

**Acceptance gate:** `cargo nextest run` green with new tests: `.on_click` receives `ButtonClicked`; reify reuse does not re-register; replacement/removal releases the tracked callback; global observation and `(panel, id)` lookup work; hover/press/disabled/focus patch only the expected render rows and do not fire `Changed<DiegeticPanel>` or `Changed<ComputedDiegeticPanel>`; behavior-module boundary test passes.

### Phase 8 — Slider behavior  · status: todo

#### Work Order

**Goal:** Headless slider: grab, drag, value change, release, cancel, disabled, optional snapping, with correct out-of-bounds drag mapping.

**Spec:**
- `widgets/slider.rs`; extend `WidgetSpec` with the slider kind and add `.slider(id, Slider::new(...))` authoring mirroring `.button(id, Button::new())`.
- `SliderDirection` is fixed independently of PD1 as `LeftToRight | RightToLeft | BottomToTop | TopToBottom`.
- **Approved value contract (PD1):** `SliderRange::new(start, end) -> Result<SliderRange, SliderConfigError>` accepts only finite, strictly ordered endpoints. `SliderStep::new(step) -> Result<SliderStep, SliderConfigError>` accepts only finite positive values. Private-field `SliderState` is the public component containing range, applied raw-domain value, optional step, and direction; `SliderState::new(range, value, step, direction) -> Result<Self, SliderConfigError>` and `set_value(value) -> Result<bool, SliderConfigError>` reject non-finite input, snap to the lattice anchored at `range.start()`, then clamp, with the Boolean reporting an applied-value change. It also exposes `range()`, `value()`, `step()`, and `direction()` readers. `Slider::new(range, initial_value) -> Result<Self, SliderConfigError>` validates the spawn value and adds private-field `step(SliderStep)` and `direction(SliderDirection)` builders. `SliderConfigError` is exactly `NonFiniteRange | UnorderedRange | NonFiniteValue | NonPositiveStep`.
- **App authority and request API:** `SliderChangeRequested` targets the widget and carries `{ id, value, is_final, pointer_id: Option<PointerId> }`; pointer drags send non-final proposals plus a final proposal on release, while semantic/remote requests are final and have no pointer. App code explicitly applies or rejects the proposal with `SliderState::set_value`. The exported `slider_self_update` observer is the opt-in uncontrolled convenience. `RequestSliderAdjustment { entity, adjustment }` computes and emits a proposal without applying it; `SliderAdjustment` is exactly `Absolute(f32) | Relative(f32) | RelativeSteps(f32)`. Every adjustment validates its numeric input; `RelativeSteps` emits no proposal when the state has no step.
- **Authored/runtime ownership:** `Slider::initial_value` applies only on first spawn. Same-id reuse preserves the live applied value; an authored range/step/direction change updates the configuration and revalidates the preserved value, while an unrelated reify does not rewrite `SliderState`. The preset reads only the applied value, never an unaccepted proposal.
- **Bevy reference check:** both the project-version `bevy_ui_widgets 0.19.0` source and local `../bevy` at `0.20.0-dev` use raw-domain state, external `ValueChange<f32>` proposals, optional self-update, and absolute/relative/relative-step remote control. Hana adopts those semantics. It intentionally does not copy Bevy's independently insertable tuple components, warn-only invalid ranges, separate `SliderPrecision`, `TrackClick`, auto-orientation, accessibility dependency, or UI-space drag delta; Hana keeps one validated state, one step lattice for pointer and semantic values, four explicit directions, and captured-camera panel-local reprojection.
- **Drag mapping:** map panel-local position to normalized value, then through the chosen range. Each `Pointer<Drag>` reprojects `pointer_location.position` via the **captured press camera and render target** → `Camera::viewport_to_world` → ray → flat panel intersection → panel-local map → clamp. `Drag.delta` is invalid for perspective world panels. Cancel if the captured camera/target disappears or no longer matches. The surface-panels integration later replaces only the flat ray→panel-local helper.
- **Lifecycle:** reuse Phase 6's capture registry, pointer matching, terminal state, raw pointer-loss handling, and finalize-before-despawn rule. `SliderGrabbed`/`SliderReleased` carry `{ entity, id, pointer_id }`; `SliderCanceled` adds `cause`, whose variants are `PointerCanceled | PointerRemoved | CaptureLost | Disabled | ProjectionUnavailable | WidgetRemoved | WidgetKindChanged | Explicit`. `WidgetDisabled` cancels an active drag and blocks pointer or semantic changes.

**Files:**
- `src/widgets/slider.rs` — new
- `src/widgets/picking.rs` (or a shared geometry module) — `panel_local_from_ray` helper
- `src/layout/builder.rs`, `src/layout/element.rs` — `.slider(id, Slider::new(...))` and retained record
- `src/widgets/reify.rs` — slider kind reify

**Constraints from prior phases:** Phase 6 owns the shared capture/terminal mechanism; Phase 3 supplies panel-local geometry and the press hit's camera; Phase 2 supplies `WidgetDisabled`; PD1 is accepted and fixed above.

**Acceptance gate:** `cargo nextest run` green with new tests: invalid numeric construction; direction/value mapping for all four directions; lattice anchoring plus snap/clamp order; app accept/reject and opt-in self-update; absolute/relative/relative-step requests; non-final/final proposal ordering; spawn-only initial value and authored range-change clamping; zero-size track; drag beyond panel bounds; captured-camera loss and multi-camera reprojection; every cancel path including pointer loss, kind change, and disable-while-dragging; disabled slider ignores grab and semantic change.

### Phase 9 — Slider overlay preset  · status: todo

#### Work Order

**Goal:** Default slider visual preset using overlay layout.

**Spec:**
- `widgets/presets/slider.rs`: `SliderPreset` / `SliderStyle` (widget-specific names, material-first slots like ButtonPreset).
- Use `El::overlay()` — track, fill, thumb, and optional labels share one content rectangle and are layered, not arranged. `DrawZIndex` orders thumb above fill above track.
- Thumb/fill placement reads PD1's applied value. Restyle and thumb movement write Phase 7 visual-slot overrides, patching only retained render records — no `LayoutTree` or computed-panel regeneration per value change.
- Preset respects `SliderDirection` for fill/thumb placement in all four directions.

**Files:**
- `src/widgets/presets/slider.rs` — new
- `src/widgets/presets/style.rs` — shared helpers only where they remove real duplication
- `src/lib.rs` — re-exports

**Constraints from prior phases:** Phase 8 supplies `SliderDirection` and PD1's applied-value contract. Phase 7 supplies stable visual slots and the behavior/preset boundary.

**Acceptance gate:** `cargo nextest run` green with new tests: thumb/fill records track the applied value in all four directions; a value change dirties only the expected visual slots and causes no relayout; preset builds under the behavior/preset boundary test.

### Phase 10 — Tooltip relationship, behavior, and lazy panel materialization  · status: todo

#### Work Order

**Goal:** Each tooltip is a lightweight entity related to its target; on first eligibility it becomes a normal anchored panel with hover/focus show-hide policy.

**Spec:**
- **Semantic relationship:** every tooltip is its own entity with public `TooltipTemplate` and `TooltipFor(target)` components; public `Tooltips` is the reverse target membership and is declared with `linked_spawn`, so target despawn owns tooltip-controller cleanup without hierarchy parenting. The target may be a widget or a `DiegeticPanel`. This relationship is semantic ownership and eligibility, not placement; once materialized, `AnchoredToPanel` points at the same target and a consistency test prevents the two from diverging.
- **Associated authoring:** `El::tooltip(self, template: TooltipTemplate) -> Self` writes a separate tooltip declaration field parallel to `CommonEl::widget`; it is not a field on `Button`, `Slider`, or crate-private `WidgetSpec`. The first API permits at most one associated tooltip and requires the same `El` to be a widget. Layout output creates a private `ComputedTooltipRecord` keyed by `(panel, widget id)`. After widget reify and a command flush, tooltip reify uses `PanelWidgetReader` to create or reuse the lightweight controller and install the template plus `TooltipFor(widget)`.
- **Standalone authoring:** app code uses `commands.spawn((template, TooltipFor::new(target)))` against an existing widget or panel. Associated and standalone tooltips therefore share one behavior and materialization path; neither call site chooses `World` or `Screen` independently.
- **Public template and policy builders:** `TooltipTemplate` contains the `Arc`-backed panel blueprint plus private `show_after`, `hide_after`, and `disabled_policy` values. `TooltipTemplate::new(tree: LayoutTree)` installs `show_after = Duration::from_millis(500)`, `hide_after = Duration::ZERO`, and `TooltipDisabledPolicy::Suppress`; consuming `.show_after(Duration)`, `.hide_after(Duration)`, and `.disabled_policy(TooltipDisabledPolicy)` builders override them. Omitting every policy builder always uses those defaults. There is no separate public `Tooltip` policy component and no `.tooltip_with_policy(...)` authoring path.
  ```rust
  pub enum TooltipDisabledPolicy {
      Show,
      Suppress,
  }
  ```
  `TooltipDisabledPolicy::Suppress` is the default. No public `TooltipTrigger`, `TooltipTiming`, or fixed visible-duration setting exists; hover-or-focus behavior is assumed, and the tooltip remains visible while eligible.
- **One private state machine:** `TooltipPhase::{Hidden, WaitingToShow(Timer), Visible, WaitingToHide(Timer)}` is authoritative. `show_after` starts when eligibility becomes true and is canceled on loss. `hide_after` is a grace period that starts when eligibility becomes false, is canceled if eligibility returns, and hides on expiry; `Duration::ZERO` is immediate. `TooltipDisabledPolicy::Suppress` hides immediately and prevents show, while `Show` leaves eligibility unchanged. Target removal always despawns immediately. `Visibility` is derived from the phase; no second visible marker or simultaneous timer components.
- **Visibility events:** public `TooltipShown` and `TooltipHidden` derive `EntityEvent`; their sole `entity: Entity` field is the event target, and propagation is disabled. They fire exactly once per actual hidden→visible or visible→hidden edge, not when eligibility changes, a timer starts, or a canceled wait ends. `TooltipShown` fires only after phase and `Visibility` are visible and layout plus placement are ready, so an observer can immediately use the tooltip transform for an effect. `TooltipHidden` fires after phase and `Visibility` become hidden but before any controller despawn, so observers can still query the entity, transform, template, and `TooltipFor` relation. Removing a visible associated declaration or despawning its target finalizes with one `TooltipHidden`; removing an already-hidden tooltip emits nothing. Rebuilding a visible tooltip for a new panel blueprint emits `TooltipHidden`, then `TooltipShown` only when the rebuilt panel is ready. Global observers can consume every event, while entity-scoped observers can attach effects to a specific standalone or relationship-discovered tooltip.
- **Lazy panel materialization and readiness:** the controller entity exists before hover/focus but carries no `DiegeticPanel`, `AnchoredToPanel`, render data, or anchor demand. When the first show wait begins, build and insert those components on the same entity with `Visibility::Hidden`. Reveal only after both the delay has elapsed and layout plus attachment placement have succeeded, including `show_after == 0`; never flash at a fallback transform. After that first materialization, the panel, layout result, render data, and placement remain resident while hidden; later transitions only update the phase and `Visibility`, never despawn/recreate the tooltip. Tick only entities in a waiting phase.
- **Coordinate-space inheritance:** materialization resolves a widget target through `WidgetOf` or reads a panel target directly, then builds the tooltip in that target panel's `World` or `Screen` coordinate space and layout unit. Cross-space placement is out of scope, so there is no independent world/screen flag to disagree with the relationship target. World tooltips lower to `hana_valence::AnchoredTo`; screen tooltips retain panel authoring and use the screen adapter over `resolve_attachments` (widget targets land in Phase 11).
- **Lifecycle and demand:** pre-materialized `TooltipFor` membership is not anchor demand. Materialization inserts the existing world/screen placement relationship, which starts demand; later hiding retains the already-built panel and placement. Retargeting updates `TooltipFor` immediately and updates `AnchoredToPanel` when materialized. Removing an associated declaration despawns its controller; target despawn does the same through `linked_spawn`. There is no inactivity eviction in the first API. World `AnchoredHere` and Phase 11's screen relationship keep geometry resident only while placement needs it. Eligibility reads target `PickingInteraction` plus `WidgetFocused` where applicable.
- **Approved panel template:** the tooltip entity carries public `TooltipTemplate`, a cloneable component wrapping an immutable concrete panel blueprint in an `Arc` alongside its policy values. Associated authoring carries the same value through the separate element declaration and `ComputedTooltipRecord`, cloning only the `Arc`; standalone authoring inserts it directly. Equality compares panel-blueprint pointer identity plus policy values. Reusing an identical clone preserves the controller and any materialized panel. A policy-only replacement updates future/current timers without rebuilding panel components. Replacing the panel blueprint hides and rebuilds the panel components on the same entity, then reveals only after fresh layout and placement readiness. It never respawns merely because the template changed.
- **Defaults and scope:** `show_after` defaults to 500 ms and `hide_after` defaults to zero; disabled policy, anchors, and offset also have defaults. Rich content is ordinary panel content; overflow avoidance is out of scope.

**Files:**
- `src/widgets/tooltip.rs` — template, controller, visibility events, materialization, state machine
- `src/widgets/relationship.rs` — `TooltipFor` / `Tooltips`
- `src/widgets/reify.rs` — associated tooltip controller reuse/removal after widget reify
- `src/layout/builder.rs`, `src/layout/element.rs`, `src/layout/engine/` — separate tooltip declaration and computed record
- `src/widgets/presets/tooltip.rs` — default tooltip panel presentation
- Read-only: `crates/hana_valence` resolver and attachment graph, `src/panel/anchoring.rs`, `src/screen_space/anchoring/`

**Constraints from prior phases:** Phase 4 publishes geometry for world demand; Phase 11 adds screen demand and ordering. Phase 3's `PickingInteraction` and Phase 5's `WidgetFocused` drive eligibility; Phase 2 supplies disabled state.

**Acceptance gate:** `cargo nextest run` green with new tests: associated tooltip authoring does not alter `Button`/`Slider` equality; one lightweight controller and exact reverse relationship exist before eligibility with no panel or geometry demand; unrelated tree replacement reuses that controller; omitted builders produce 500 ms show and zero hide delays plus `Suppress`; show wait cancels; hide grace cancels on renewed eligibility and `Duration::ZERO` hides immediately; first show materializes hidden and reveals only after layout+placement, including zero delay and `Fit`; every later hide/show preserves the same entity and resident panel/render state; a new blueprint rebuilds hidden on that same entity while an identical clone does nothing and a policy-only change does not rebuild; `Suppress` immediately hides an already-visible tooltip; `TooltipShown` and `TooltipHidden` target the tooltip entity exactly once per completed visibility edge, emit nothing for canceled waits or redundant states, expose ready transforms on show, and emit `TooltipHidden` before visible-controller cleanup; retargeting updates both semantic and materialized placement targets; associated declaration removal and target despawn each clean up exactly once; standalone and associated tooltips share the path; world and screen targets inherit the correct space; a world widget tooltip honors nonzero offset.

### Phase 11 — Screen-placer widget targets  · status: todo

#### Work Order

**Goal:** Screen-space tooltips and anchored panels can target widgets, not only panels.

**Spec:**
- The screen placer builds candidates from `PanelAttachmentAuthored` but accepts panel targets only today. Teach it to recognize a widget, resolve the owning screen panel/window through `WidgetOf`, and derive the target rectangle from current widget-local geometry plus the owning panel's screen rect/transform instead of `ScreenPanelRect` on the target. The screen source still must not carry `hana_valence::AnchoredTo`.
- Add a private source/target relationship for materialized screen widget attachments, analogous to `AnchoredTo`/`AnchoredHere` but without `linked_spawn`. Insert/replace/remove/despawn and retargeting keep the reverse target membership exact. Nonempty membership is screen geometry demand, supports multiple sources, and prevents geometry retirement until the last source detaches. `TooltipFor` owns semantic tooltip cleanup separately and does not count as geometry demand before materialization.
- Screen demand synchronization and widget geometry publication run in `Update` after `WidgetSystems::Reify`, followed by `ApplyDeferred`, and before `PanelSystems::ResolvePanelAttachments`. This is separate from the world `AnchorSystems::FillGeometry` provider in `PostUpdate`; no ordering claim crosses schedules.
- **Graph dependency proxy:** for every demanded widget, add a private resolver candidate `widget → owning panel`. Its placement action recomputes the widget's screen rect after the owning panel's current placement; real attachments still target the widget candidate. This gives `resolve_attachments` the required owner-panel→widget→tooltip order when the owner panel is itself attached, without exposing the proxy as authoring or mutating the widget hierarchy.
- Reuse Phase 4's `AnchorTargetMetrics::Widget` for screen layout-unit offsets. A missing owner, window, geometry, or transform yields the screen adapter's source/target/reason diagnostic instead of a panel-only fallback.
- Keep window and viewport projection in diegetic, but continue delegating graph ordering, cycles, fallback, and diagnostics to `hana_valence::resolve_attachments`. Missing widget geometry uses the screen adapter's `AttachmentResolveDiagnostics` source/target/reason key.

**Files:**
- `src/screen_space/anchoring/candidate.rs` — widget-target candidate rects
- `src/screen_space/anchoring/resolve.rs` — target resolution
- `src/screen_space/anchoring/projection.rs` — reuse/extend projection helpers
- `src/widgets/relationship.rs`, `src/widgets/reify.rs` — screen reverse relationship, demand, and geometry retirement

**Constraints from prior phases:** Phase 1 reifies widget identity/rects early in `Update`; Phase 4 owns geometry and target metrics; Phase 10 screen tooltips retain private `PanelAttachmentAuthored` and do not enter the world resolver.

**Acceptance gate:** `cargo nextest run` green with new tests: a new or relaid-out widget target resolves in the same frame; an attached-owner-panel→widget→tooltip chain follows graph order; tooltip placement uses the widget viewport rect and nonzero offset; two attachments maintain demand until both detach; retargeting moves reverse membership; final detach removes geometry; panel targets remain unchanged; missing owner/geometry warnings deduplicate per source, target, and reason.

### Phase 12 — Demonstration checkpoint (stop and discuss)  · status: todo

#### Work Order

**Goal:** Decide, with the project owner, how to demonstrate the widget system. This phase is a discussion checkpoint, not delegated implementation.

**Spec:**
- Stop after Phase 11 and design the demonstration plan together: which existing examples change, which new examples are added.
- The plan must prove the pieces work together in real diegetic UI: buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and existing IME/text input coexisting on one panel.

**Files:** none until the discussion lands.

**Constraints from prior phases:** All widget subsystems (Phases 1–11) complete and tested headlessly (`HeadlessLayoutPlugin`/minimal-app tests) before demonstration work begins.

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
- **F8 — Interactivity represented inheritance twice and exposed a first-frame input gap · accepted.** The value enum is concrete, `bevy_kana::Cascade` owns authored inheritance, the entityless virtual-layout fallback stays separate from runtime authoring, and the ordered resolver seeds disabled state before picking.
- **F9 — Focus policy, authority, scope, and traversal conflicted · accepted.** One per-window authority, panel-local computed preorder, retained disabled focus, typed transitions, IME gating, and adapter ordering replace the contradictory rules.
- **F10 — Widget id lookup had no entity-side owner · accepted.** `PanelWidget` plus `PanelWidgetReader` provide panel-qualified identity without changing `PanelElementId`.
- **F11 — Retained `.on_click(closure)` could not register a system · accepted.** A typed tracked callback template defers world registration to reify and owns cleanup while preserving the promised call.
- **F12 — Widget anchor offsets and lazy-demand teardown were missing · accepted.** Widget-aware target metrics, current transforms, multiplicity-aware reverse relations, retargeting, and last-demand retirement are required.
- **F13 — Tooltip timing and first reveal were contradictory · accepted.** One phase enum defines show/hide grace, zero duration, suppress behavior, hidden first panel materialization, and layout+placement readiness.
- **F14 — Consuming widget clicks bypassed IME blur · accepted.** Widget click handling classifies blur through the owning panel before stopping propagation.
- **F15 — Nested and precomposed interaction had no valid ownership/order model · accepted.** The first API rejects those forms while retaining arbitrary non-interactive child layout.
- **F16 — Authoring files and the Phase 4 screen gate were wrong · accepted.** Work moved to `layout/builder.rs`/`layout/element.rs`, and screen-demand acceptance moved to Phase 11.
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
- **Decision:** Widget entities participate directly in the shared engine and explicitly inherit from their owning panel. The already-approved entityless layout-subtree layer remains a private computed fallback between a widget-local runtime override and the shared panel/global result; it does not materialize private cascade-scope entities or overwrite live authored state during reify.
- **Public boundary:** Hana adds typed widget-interactivity builder and entity-command verbs but does not re-export raw `Cascade<T>` or `Resolved<T>` as widget API.
- **Status:** accepted — 2026-07-16

### Dropped proposals

- **DP1 — Remove the button authoring value entirely · dropped.** Public naming is now `.button(id, Button::new())`, but the second argument remains the home for optional retained button behavior such as `.on_click(...)`.
- **DP2 — Move `PanelElementId` out of the IME module · dropped.** The crate-root API is already stable and the move is unnecessary for widget correctness; `PanelWidget` supplies the missing entity ownership.
