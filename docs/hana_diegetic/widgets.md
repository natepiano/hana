# Headless Widgets

> **Status: IMPLEMENTATION PLAN — phased; ready for delegation.** Adds headless widgets (buttons, sliders, tooltips, focus, interactivity) to `hana_diegetic`: widgets own semantic behavior and typed events, visuals stay ordinary layout primitives, widgets reify as panel child entities targeted by Bevy picking, and anchoring comes from `hana_valence`. Phase 12 remains the required demonstration-design stop after the implementation work orders.

## Delegation Context

- **Project** — `hana_diegetic` (workspace member at `crates/hana_diegetic`). Diegetic UI layout engine for Bevy — in-world panels driven by a Clay-inspired layout algorithm. This plan adds a headless `widgets` module that reifies widgets as panel child entities.
- **Stack** — Rust (edition 2024). Bevy `0.19.0` is pinned in the root `Cargo.toml`. `thiserror` `2.0.18` is a workspace dependency and direct `hana_diegetic` dependency. `bevy_picking` + `mesh_picking` features are already enabled; widget presentation reads the all-pointer `bevy_picking::PickingInteraction` aggregate, and one diegetic picking backend owns the ordered panel+widget hit group. `bevy_enhanced_input` `0.26.0` is a workspace dependency and becomes a direct `hana_diegetic` dependency in Phase 5.5. `hana_valence` is a workspace path dependency declared in the root `Cargo.toml`. No bevy_ui.
- **Layout** (only phase-touched paths):
  - `crates/hana_diegetic/src/widgets/` — NEW module: `mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `id.rs`, `relationship.rs`, `interactivity.rs`, `focus.rs`, `input.rs`, `picking.rs`, `reify.rs`, and `visual.rs`.
  - `crates/hana_diegetic/src/layout/` — `builder.rs`, `element.rs`, and the engine output that produces widget records and visual-slot references.
  - `crates/hana_diegetic/src/ime/` — `activation.rs`, `field.rs`, `ids.rs`, `mod.rs` (`ImePlugin`).
  - `crates/hana_diegetic/src/panel/` — `builder.rs`, `anchoring.rs`, `anchor_geometry.rs`, `arrangement.rs`, `coordinate_space.rs`, `diegetic_panel.rs`, `valence_provider.rs`, `perf.rs`.
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
  - `src/panel/anchoring.rs` — same-space attachment authoring through opaque `PanelEntity<Space>` / `WidgetEntity<Space>` handles, private `PanelAttachmentAuthored`, world-only lowering to `hana_valence::AnchoredTo`, offset lowering, and attachment removal. Screen panels keep the shared private authoring without the world relation. Raw `Entity` remains available for unrelated ECS work but is not accepted by the public attach or retarget operations.
  - `src/panel/coordinate_space.rs` — required `PanelSpace` mirror synchronized with each live `DiegeticPanel`; coordinate-space branches and typed readers use it without borrowing the full panel component.
  - `src/panel/conversion/` — checked world↔screen conversion. Every public conversion entry point refuses a panel while it is an attachment source, a panel target, or the owner of a targeted widget; the caller detaches, converts the affected panels, then reattaches with handles for the new space.
  - `src/render/panel_geometry.rs` — current flat `PanelInteractionMesh`; Phase 3 moves it out of the generic mesh backend and makes the diegetic backend emit the panel and widget hits together.
  - `src/screen_space/anchoring/candidate.rs` + `resolve.rs` — screen placement builds candidates from private `PanelAttachmentAuthored` and delegates ordering and diagnostics to `hana_valence::resolve_attachments`; it accepts panel targets only today, and Phase 4.5 teaches it widget targets.
  - `src/panel/perf.rs` + `src/panel/constants.rs` — `DiegeticPerfStats` (`perf.rs:45`), `pub reify_ms: f32` (`perf.rs:54`), and `DIAG_PANEL_REIFY_MS` (`constants.rs:35`, published at `perf.rs:258`).
  - `src/render/mod.rs` — `PanelChildSystems` set enum (`:128`); `TextRunOf`/`PanelTextRuns` re-exports.
  - `src/widgets/focus.rs` + `src/widgets/input.rs` — shipped per-window focus authority, public focus events, six window-scoped semantic messages, private resolved widget intent, IME/disabled gating, and traversal over current computed preorder.
  - `src/panel/lifecycle.rs` + `src/panel/mod.rs` — panel-role cleanup plus the earlier full-entity `Despawn` observer required when terminal events must run before Bevy queues linked-child despawns.
  - `src/lib.rs` — curated re-exports, including `PanelBuildError`; widget public types re-export here.
- **Canonical runtime example:** `crates/hana_diegetic/examples/widgets.rs` follows `docs/fairy_dust/canonical-example.md` and is the cumulative widget integration target. Every later widget phase extends this example with its new public behavior and exercises that behavior in the required application smoke test. Preserve Phase 6's visible `Pointer`, `Focus`, and `Button` diagnostic rows and both pointer-originated and semantic button paths; later rows expand the measured readout bounds instead of replacing or clipping existing output.
- **Canonical input paths:** later edits to `examples/widgets.rs` preserve both Phase 5.5 integration proofs: Hana's built-in per-window `WidgetInputPlugin` bindings and the app-owned Bevy Kana action that sends the same Phase 5 message. Later behavior may extend either path but must not replace one with a second widget API.
- **Build:** `cargo check -p hana_diegetic` plus `cargo +nightly fmt --all -- --check`. If a Work Order changes another workspace package, check only that exact additional package; never compile the whole workspace for a package-scoped phase.
- **Test:** `cargo nextest run -p hana_diegetic --lib` (never `cargo test`). The library-only target avoids compiling every example as an implicit package test target. When a Work Order changes an example, compile only that example separately with `cargo check -p hana_diegetic --example <name>`; if another package's behavior changes, test only that exact additional package.
- **Lint:** run `cargo clippy -p hana_diegetic --lib --tests -- -D warnings` for a `hana_diegetic`-only phase, adding only another exact changed package when required. Do not invoke the shared `clippy` skill as a phase gate: its wrapper hardcodes `--workspace --all-targets --all-features` and therefore cannot honor this plan's package/target scope. Workspace lint policy still applies: `all`/`cargo`/`nursery`/`pedantic` denied, `unwrap_used`/`expect_used`/`panic`/`unreachable` denied, `missing_docs = "deny"`, `self_named_module_files` denied (use `module/mod.rs` directory form).
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/hana_diegetic_widgets`
- **Invariants:**
  - **Valence gate:** `hana_valence` exists at `crates/hana_valence`; its resolver, panel bridge, and screen-adapter integration are described in `docs/hana_valence/as-built/anchoring-and-arrangements.md`. Hana Valence types stay out of diegetic's public widget signatures. Diegetic authoring lowers to `hana_valence::AnchoredTo` only for world sources; screen sources retain `PanelAttachmentAuthored` and use the shared attachment graph without carrying the world relation.
  - No bevy_ui / bevy_a11y dependency. `WidgetDisabled`, `WidgetFocused`, the private focus-indicator marker, and pointer-capture state stay bespoke; `PickingInteraction` supplies all-pointer hover/press presentation and `bevy_enhanced_input` supplies the opt-in semantic-action adapter.
  - Deterministic pointer tests feed synthetic `PointerHits` plus raw `PointerInput` through Bevy's real hover and `pointer_events` dispatcher. They never move the operating-system pointer and do not treat directly triggered target events as sufficient integration coverage.
  - Widgets reify as panel child entities under `ChildOf(panel)`; the `WidgetOf`/`PanelWidgets` relationship is a traversal index only, no `linked_spawn` — `ChildOf` owns despawn.
  - Behavior modules never construct layout/render primitives (`El`, `LayoutTree`, `PanelDraw`, materials, `TextStyle`, `DrawZIndex`). Ordinary tree authoring and the direct widget builders supply the private retained slots and state values that behavior presents.
  - No relayout on hover/press/focus/disabled/value flips. Ordinary tree authoring supplies stable private visual-slot ids; changed widget state patches only those slots' retained batch records through widget-owned override components. It never regenerates the `LayoutTree` or writes `DiegeticPanel`/`ComputedDiegeticPanel` merely to restyle a widget. Runtime overrides can change appearance or translate a record in its panel-local XY plane, but preserve authored draw depth, widget hit rectangles, and cross-widget interaction rank.
  - Widget semantic reify runs in `Update`, after `PanelSystems::ComputeLayout` and before `PanelSystems::ResolvePanelAttachments`, with an explicit `ApplyDeferred` fence. Existing cascade propagation remains before layout so inherited layout attributes update geometry in the same frame. From Phase 2 onward, Bevy Kana's insertion observer seeds newly reified widget cascade state during the reify fence; later widget resolvers run after that fence. Render-child batching remains in `PostUpdate`.
  - Change-gated systems, never unconditional per-frame walks: reify is gated on `Changed<ComputedDiegeticPanel>` and reuses entities by id; interactivity writes `WidgetDisabled` only on diff; anchor geometry exists only while world or screen demand is nonempty and is removed after the last demand ends.
  - A newly reified widget receives one initial `GlobalTransform` composed from its owning panel's current global transform and its translation-only local transform; existing widgets receive no recurring global writes. World widget offset conversion reads the owning panel's propagated scale, while Hana ownership of widget geometry and the private world bridge is tracked and retired independently.
  - Public attachment mutation is same-space by construction. `PanelEntity<World>` can target only `PanelEntity<World>` or `WidgetEntity<World>`, and the equivalent rule holds for `Screen`; handles reuse the builder's existing `World` / `Screen` markers. Attachment and conversion methods share the existing `DiegeticPanelCommands` extension on one Bevy `Commands` value, preserving written order without more command-wrapper types. Conversion is allowed in both directions but is rejected while the panel or one of its widgets participates in placement. After the command fence, callers reacquire the destination-space handle through `PanelEntityReader`. Because ECS identity can outlive a handle, every queued mutation checks that its handle still matches the live panel when it applies.
  - Widget ids reuse `PanelElementId` and its `duplicate_named_element_id` → `DuplicateElementId` validation; event-emitting widgets require `Named` ids (auto ids reposition on structural edits and would fire spurious cancels).
  - Widget interactivity is one logical cascade across both storage domains: global/default and panel/explicit ancestors are ECS participants, while parent/child `El` authoring is folded during layout traversal. The folded layout-tree `Cascade<WidgetInteractivity>` is reified onto the widget entity, whose explicit `CascadeFrom(panel)` lets Bevy Kana produce the final `Resolved<WidgetInteractivity>`. The `LayoutTree` remains authoritative for the reified widget component, matching panel-text style reification; no second virtual-layout component or custom precedence resolver exists.
  - `commands.entity(panel).remove::<DiegeticPanel>()` removes the panel role but does not despawn `panel`. Hana finalizes and removes every entity and retained component recorded as owned by that panel; unrelated application components on `panel` remain. Hana never detaches or reparents application entities: anything an application parents beneath a Hana-owned runtime entity follows Bevy's normal recursive-despawn semantics when that runtime entity is removed. `commands.entity(panel).despawn()` instead removes `panel` and its hierarchy normally. Terminal events that require live widget relationships use both paths established by focus: ordinary `On<Remove, DiegeticPanel>` finalization for component-only role removal and an earlier `On<Despawn, DiegeticPanel>` finalizer for full entity despawn, before Bevy queues linked-child despawns. `set_tree` is not teardown because the `DiegeticPanel` component remains installed.
  - Widget events derive `EntityEvent` targeting the widget entity; the panel-local id is a payload convenience only, never the routing key. Owning panel resolves through `WidgetOf`, never duplicated on components or events.
  - Exported `hana_diegetic` error types derive `thiserror::Error`, declare messages beside their variants, and have exhaustive stable-message tests. Converting sources and intentionally lossy normalization mappings stay explicit.
  - Widget picking geometry stays in **panel-local space**. The first implementation uses the current flat interaction-mesh hit conversion. Curved-panel support is gated on Phase 5 of `surface-panels.md`, which replaces that one boundary with `PanelSurface::project()`; widget rectangle tests remain unchanged and never place geometry independently in world space.
  - The builder-provided initial `PanelPicking` is a Hana-owned sibling component installed only while absent. Unchanged Hana values leave with the panel role; explicit spawn-bundle values and ordinary later application writes survive. Competing application observers deliberately queued after Hana in the same command flush are application responsibility and do not add a second ownership mechanism.
  - The first API rejects interactive descendants inside a widget and widgets inside precomposed subtrees. Arbitrary non-interactive child layout remains valid; nested/precomposed interaction needs a later ownership and hit-order design.
  - Tooltip authoring is separate from `Button`/`Slider` authoring. Reify creates a lightweight tooltip entity with `TooltipFor(target)`; first eligibility materializes that same entity into a hidden anchored panel. The semantic relationship exists before the placement relationship and does not itself create anchor-geometry demand. Each controller also records its owning panel role privately so component-only panel teardown cannot strand a controller on a surviving target entity.
- **Public contract ledger (fixed before delegation except where a phase-local pending-decision block names the unresolved surface):**
  - Authoring methods are `El::button(self, id: impl Into<PanelElementId>, button: Button) -> Self` and `El::slider(self, id: impl Into<PanelElementId>, slider: Slider) -> Self`. Both assign the element id and crate-private widget variant atomically. `Button` is a private-field `Clone + Debug + PartialEq + Default` authoring builder with `new()` and Phase 7's `on_click(...)`; `Slider` is a private-field `Clone + Debug + PartialEq` validated authoring builder with no `Default` because range and initial value are required. Neither is an ECS component. Crate-private `WidgetSpec` is exactly `Button(Button) | Slider(Slider)`, while runtime slider data lives in `SliderState`, so no public `Spec` suffix is needed.
  - New validation variants are `PanelBuildError::WidgetRequiresNamedId(PanelElementId)`, `WidgetContainsInteractiveDescendant(PanelElementId)`, and `WidgetInsidePrecomposedSubtree(PanelElementId)`. Phase 1 adds them to the `thiserror::Error` enum established in Phase 0.5, with direct `#[error(...)]` messages and stable-message tests.
  - Runtime tree replacement has one public path: `DiegeticPanelCommands::set_tree(&mut self, entity: Entity, tree: LayoutTree) -> Result<(), PanelBuildError>`. It validates synchronously with the same validator as panel construction and queues the deferred replacement only for a valid tree; rejection queues nothing and preserves the current tree. `Ok(())` means validation succeeded and the replacement was queued, not that the deferred command later found a live panel entity. There is no `try_set_tree` companion; Phase 1 migrates every internal caller to handle the result explicitly.
  - Identity exports are `PanelWidget`, `PanelWidgetReader`, `WidgetOf`, and `PanelWidgets`. `PanelWidget` exposes only `id()`, and `WidgetOf` exposes only `panel()`; relationship mutation remains internal. `PanelWidgetReader` is the read-only `SystemParam` bridge from an authored `(panel, PanelElementId)` to the live reified widget entity for app-initiated entity-targeted control. Entity events already carry their widget target and never require this lookup.
  - Interactivity/focus exports are `WidgetInteractivity`, `WidgetDisabled`, `PanelWidgetWriter`, `WidgetFocusable`, `WidgetFocused`, `RequestWidgetFocus`, `ClearWidgetFocus`, `WidgetFocusChanged`, and `WidgetFocusChangeCause`. Element authoring is `El::widget_interactivity(self, value: WidgetInteractivity) -> Self`. `PanelWidgetWriter::override_interactivity(widget, value)` and `inherit_interactivity(widget)` update the authoritative widget `El` by following `PanelWidget` + `WidgetOf`; an event target can be passed directly, while `(panel, id)` callers resolve it through `PanelWidgetReader`. The existing `CascadeEntityCommandsExt` gains `override_widget_interactivity(value)` and `inherit_widget_interactivity()` for panels and other ECS-authored cascade ancestors; raw `Cascade<T>` remains owned by `bevy_kana` and is not re-exported. The focus request payload is `{ window, widget }`, clear is `{ window }`, and change is `{ window, previous, current, cause }`. Cause variants are `Pointer`, `Traversal`, `Semantic`, `Application`, `ExplicitClear`, `WidgetRemoved`, `FocusabilityRemoved`, and `ScopeLost`; disable is intentionally absent. Pointer presses retain semantic `WidgetFocused` state but hide the private focus indicator; traversal and direct application requests show it. No additional public focus-visible type is introduced.
  - The six exported semantic messages are `FocusNextWidget`, `FocusPreviousWidget`, `FocusFirstWidget`, `FocusLastWidget`, `ActivateFocusedWidget`, and `CancelFocusedWidget`; each carries `{ window: Entity }`. Phase 5 resolves the named window through private focus authority, applies IME and disabled gating, and emits one private entity-targeted `SemanticWidgetIntent`. The optional adapter exports `WidgetInputPlugin`, `WidgetInputMode`, `WidgetInputBindings`, `WidgetInputBindingsBuilder`, `WidgetInputBindingsError`, `WidgetInputDisabled`, and `WidgetControlSummary`. `WidgetInputMode` is configured independently on each `Window`: `Default` uses Tab / Shift+Tab / Home / End / Enter or Space / Escape, `Bindings(WidgetInputBindings)` uses that window's custom bindings, and `Manual` creates no adapter-owned input context so the application can use its own Bevy Enhanced Input actions and Bevy Kana macros to send the six core messages. `WidgetInputBindings` is reflected, replaceable authored data and is the adapter-facing value a future keymap may produce; enhanced-input action, modifier, and trigger entities remain private runtime installation. `WidgetInputBindings::builder()` returns the public builder; its six fluent action methods each accept `impl Into<Binding>` and add alternatives, while `build()` deduplicates same-action repeats and returns `WidgetInputBindingsError::NoneBinding` or `ConflictingBinding(Binding)` for invalid input. Adding `WidgetInputPlugin` automatically gives every existing and newly created window `WidgetInputMode::Default` unless that window already specifies another mode. Keyboard and built-in gamepad actions run only for the operating-system-focused window; if there is not exactly one focused live window, built-in gamepad input emits no widget action. Other windows retain their remembered widget focus without responding. Adding or removing `WidgetInputDisabled` temporarily tears down or restores only that window's adapter-owned input entities without changing its selected mode or remembered widget focus. `WidgetInputMode::control_summary()` returns a display-only `WidgetControlSummary` with `next`, `previous`, `first`, `last`, `activate`, and `cancel` label lists; it is not runtime input state, and `Manual` produces empty lists.
  - Button exports are `Button`, `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, `ButtonCanceled`, and `ButtonCancelCause`. Every event has `entity` as its `#[event_target]` plus `id: PanelElementId`; pressed/released add `pointer_id: PointerId`, clicked adds `pointer_id: Option<PointerId>` (`None` for semantic activation), and canceled adds `pointer_id: PointerId` plus `cause`. `ButtonCancelCause` is exactly `PointerCanceled | PointerRemoved | CaptureLost | Disabled | WidgetRemoved | WidgetKindChanged | Explicit`.
  - Slider exports are `Slider`, `SliderState`, `SliderRange`, `SliderStep`, `SliderDirection`, `SliderConfigError`, `SliderGrabbed`, `SliderChangeRequested`, `SliderReleased`, `SliderCanceled`, `SliderCancelCause`, `RequestSliderAdjustment`, `SliderAdjustment`, and `slider_self_update`. Phase 10 appends `TooltipTemplate`, `TooltipFor`, `Tooltips`, and `TooltipDisabledPolicy`; Phase 10.5 appends `TooltipShown` and `TooltipHidden`. Preset and theme APIs are deferred to [`widgets-deferred.md`](widgets-deferred.md).
  - Tooltip construction is `El::tooltip(self, template: TooltipTemplate) -> Self` for associated authoring and `commands.spawn((template, TooltipFor::new(target)))` for standalone authoring. `TooltipFor::new(target)`, `target()`, and `retargeted(target)` are public; `Tooltips::iter()` exposes reverse membership; mutation is maintained by Bevy relationship hooks; and `Tooltips` uses `linked_spawn` to despawn related tooltip controllers with their target. A private `PanelOwned` record follows the target's panel role and covers component-only role removal.
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
world-panel provider, panel-attachment lowering, and screen attachment adapter
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
- Disabled changes are visual/state-only: changing content or dimensions requires an explicit authoritative tree edit.

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
- Phase 5 owns a post-interactivity deferred fence and spawn-only `WidgetFocusable`; Phase 5.5 isolates the optional enhanced-input adapter. The formerly deferred routing and configuration surfaces are settled in their respective work orders.
- Same-id/same-kind authored or computed refresh preserves button/slider capture; only removal, kind change, disable, teardown, or an explicit terminal cause cancels behavior.
- `.on_click` registration and retained visual-slot infrastructure are separate Phases 7 and 7.5. Tooltip controller reify and lazy panel materialization are separate Phases 10 and 10.5, each with its own deferred-command visibility point.
- Phase 8 carries the propagated capture/fence constraints; Phases 9 and 12 need no direct Phase 2 correction.

### Phase 3 — Widget `Transform`, single rect source, custom picking backend  · status: done (`774ced5f`)

#### Work Order

**Goal:** Widgets are first-class Bevy picking targets via a custom backend testing panel-local rects; pointer hover works on widget entities.

**Spec:**
- **Transform:** widgets carry a real panel-local `Transform` — translation = the widget's panel-local offset; `GlobalTransform` propagates via `ChildOf(panel)`. This is deliberately unlike text runs (which carry no `Transform`; their placement is baked into run records) — copying the text-run shape would break the picking backend and collapse anchor geometry to the panel origin.
- **Single rect source:** layout writes the widget's panel-local rect, effective ancestor-clipped rect, current computed-tree preorder, and interaction rank into `ComputedWidgetRecord` once. The shipped Phase 2 record in `widgets/id.rs` contains `id`, `kind`, `preorder`, `authored`, and folded `interactivity`; Phase 3 extends that exact record rather than replacing or reconstructing it. `LayoutTree::computed_widget_records()` cannot see `LayoutResult::computed`, so replace or extend that construction so the full-layout commit joins computed bounds, clipping, and draw order into each record. Make `ComputedDiegeticPanel::regenerate_commands` use the same record path so visual-only updates preserve current geometry and folded cascade while refreshing authored snapshots and ranks. Picking bounds and Phase 4 anchor points project that record; no subsystem recomputes the rect with different invalidation triggers. Fully clipped widgets are not hit targets. Overlap order is deterministic: visual `DrawZIndex`, then source order, with a nested-interaction error from Phase 1 removing ambiguous ancestor/descendant targets.
- **One diegetic backend** (`widgets/picking.rs`): iterate Bevy's `(camera, pointer)` rays, apply the mesh backend's camera order, visibility, `RenderLayers`, `Pickable`, and render-target filters, and immediately raycast only `PanelInteractionMesh` entities. Test only `PanelWidgets` belonging to intersected panels. Emit the panel and all matching widgets in **one** ordered `PointerHits` group so widget depth is actually comparable with its panel; exclude panel interaction meshes from the generic mesh backend. Widget hits are slightly nearer than their panel and ordered against one another by the computed interaction rank.
- **Retained widgets, not widget meshes:** keep one interaction surface per panel and perform panel-local rectangle tests after that broad-phase hit. Do not add `Mesh3d`, materials, or render children per widget: widgets are semantic entities over retained batched panel content, their authoritative hit bounds already include layout clipping/order, and per-widget meshes would duplicate geometry plus require a second synchronization path for relayout, visual-only refresh, clipping, and future surface projection. The backend may reuse Bevy's mesh-ray intersection for the one `PanelInteractionMesh`; Hana-specific work begins at world-hit → panel-local conversion and widget-record testing.
- **Panel-level and two-sided interaction:** always report the owning `DiegeticPanel` entity itself, never its private mesh child, so panel background interaction and later whole-panel behaviors such as dragging retain a stable target alongside widget hits. The interaction surface is pickable from front and back: preserve the existing `RayCastBackfaces` ray-cast marker as well as the material's `double_sided = true` / disabled culling, and require front/back rays through the same panel-local point to resolve the same panel/widget identities. Widget-versus-panel gesture arbitration belongs to later behavior; Phase 3 preserves both targets and their ordering.
- **Flat-now/surface-later boundary:** extract the current affine hit→panel-local conversion from `ime/activation.rs` into one shared flat projection helper. Phase 3 supports the currently shipped flat interaction mesh. Phase 5 of `surface-panels.md` later replaces that helper and mesh with `PanelSurface::project()` plus the curved interaction mesh; until then this plan makes no curved-panel picking claim.
- **Pointer presentation:** use Bevy's `PickingInteraction` aggregate for hover/pressed/none presentation across mouse, touch, stylus, and custom pointers. Do not insert `Hovered`: Bevy 0.19 updates it from `PointerId::Mouse` only and performs a linear scan of every entity carrying it. Pointer-specific capture still uses `PointerId` in Phases 6 and 8.
- **Panel-role teardown:** `commands.entity(panel).remove::<DiegeticPanel>()` is supported; it removes the panel role while `panel` remains alive. Add one central `On<Remove, DiegeticPanel>` path that first finalizes diegetic-owned widget/controller behavior while those targets remain queryable, then despawns every entity recorded as owned by that panel and removes every relationship, required/private index (including `PanelWidgetIndex`), computed/runtime component, and retained render/interaction artifact owned by the panel runtime. Preserve unrelated application components on `panel` and do not select non-owned entities for teardown. Do not detach or reparent anything: if application code parents its own entity beneath a Hana-owned runtime entity, that child follows Bevy's normal recursive-despawn semantics and its lifetime is the application's responsibility. `commands.entity(panel).despawn()` instead removes `panel` and its hierarchy through normal Bevy cleanup. `DiegeticPanelCommands::set_tree` does not invoke teardown because it preserves the panel role. Later behavior phases extend this same choke point rather than adding competing owner-loss cleanup systems.

**Files:**
- `src/widgets/picking.rs` — new (backend)
- `src/widgets/reify.rs` — Transform + computed rect/rank writes
- `src/layout/element.rs` — extend the shipped `computed_widget_records` source-tree walk with computed-layout inputs
- `src/layout/engine/`, `src/render/clip.rs`, `src/render/draw_order.rs` — clipped bounds and interaction rank
- `src/panel/compute_layout.rs` — build geometry-bearing records during a full layout commit
- `src/panel/diegetic_panel.rs` — keep `regenerate_commands` on the same record construction path
- `src/widgets/id.rs`, `src/widgets/mod.rs` — preserve the Phase 2 record fields and integrate widget cleanup with the central owner-component removal lifecycle
- panel/text/render relationship and runtime-state modules named in Delegation Context — enumerate and tear down only artifacts owned by the diegetic panel role
- `src/render/panel_geometry.rs`, `src/ime/activation.rs` — owned panel raycast and shared flat conversion

**Constraints from prior phases:** Phase 1 reifies widgets under `ChildOf(panel)` and reuses entities by id. The shipped Phase 2 `ComputedWidgetRecord` stores id, kind, preorder, authored snapshot, and folded interactivity cascade through both full and visual-only panel updates; it does not yet carry computed geometry. Phase 2 supplies `WidgetDisabled` (the backend may still report hits on disabled widgets; behavior systems gate on the marker), the separate required `PanelWidgetIndex`, and the named `WidgetSystems::ReifyCommandsApplied` fence. Bevy does not remove required sibling components when `DiegeticPanel` alone is removed, so Phase 3 must explicitly dismantle the full library-owned panel role.

**Acceptance gate:** `cargo nextest run` green with new tests: pointer over a widget yields `Over`/`Out` on the widget; one hit group orders widget before panel and reports the panel root rather than its private mesh child; panel background remains a direct panel hit; front- and back-face rays through the same local point resolve the same panel/widget identities; no widget entity receives a per-widget mesh or material; partial/full ancestor clipping gates hits; overlapping widgets follow `DrawZIndex` then source order; hidden, layer-mismatched, and non-pickable panels do not hit; two cameras preserve the originating camera and order; mouse and a non-mouse pointer update `PickingInteraction`; an off-origin widget picks at its actual location; full-layout and visual-only record regeneration preserve the folded interactivity value while updating geometry/authored fields independently; removing only the `DiegeticPanel` component leaves its entity and unrelated application components alive but removes every entity recorded as owned by that panel plus every library-owned relationship, private/required index, and computed/runtime artifact, with no pickable or reader-resolvable orphan widget; teardown never detaches or reparents application entities; despawning the panel entity uses normal Bevy hierarchy cleanup; ordinary `set_tree` preserves the panel role and widget identity.

#### Retrospective

**What worked:**
- Extending the shipped `ComputedWidgetRecord` gave reify, picking, and later anchor work one solved rect, clipping, and ordering source.
- Injecting known rays into Bevy's real `RayMap` made pointer hits, `Over`/`Out`, and `PickingInteraction` deterministic without controlling the operating-system pointer.
- `PanelOwned` plus central component, cascade, and render-layer ownership records let panel-role teardown preserve the panel entity while removing Hana runtime state.

**What deviated from the plan:**
- Lifecycle work expanded into screen-space layers, panel anchoring and arrangements, text and shape children, and precomposition because removing only `DiegeticPanel` leaves required siblings and other runtime systems active.
- The canonical Fairy Dust example became a world panel attached to the cube, with visible interaction styling and a separate status panel.
- Arrangement placement needed an explicit runtime ownership marker and `Member`-removal cleanup after final review found that an arranged entity could remain active after losing its panel role.

**Surprises:**
- A same-frame `RenderLayers` removal and reinsertion leaves a removal event, so synchronization must confirm that the panel is still layerless before restoring layer 0.
- Valence arrangement systems mutate `Hinge` and `AnchorPose` every frame, so tick-based generic ownership cannot represent their lifetime; `PanelArrangementRuntime` records that placement ownership instead.
- Application children beneath Hana-owned runtime entities follow Bevy's recursive-despawn behavior; teardown does not detach or reparent them.

**Implications for remaining phases:**
- Geometry, behavior, and controller entities must consume the existing widget `Transform` and computed record, and register runtime ownership with the central panel-role teardown path.
- Later behavior must finalize capture, focus, and controller state before its owned entities are despawned; ordinary `set_tree` updates keep the panel role and stable widget identity.
- Phase 12 can extend the canonical world-space widget lab instead of creating another validation app.

#### Phase 3 Review

- Added a production-path visual-only `set_tree` test proving authored state and interaction rank refresh while rects, folded interactivity, entity identity, and layout-solve count remain unchanged.
- Phases 4 and 4.5 now join geometry, bridge, proxy, and reverse-demand cleanup to the central panel-role lifecycle; Phase 5 finalizes focus there before widget despawn.
- Phase 7.5 now keeps presentation-only appearance and panel-local XY translation patches inside the owning widget's retained records and explicitly preserves authored draw depth, Phase 3 hit bounds, and cross-widget order.
- Phases 10 and 10.5 now track each tooltip controller's owning panel role, clean it up when only that role is removed, and emit the hidden event before visible-controller teardown.
- Every remaining implementation phase names the canonical `examples/widgets.rs` integration target; Phase 12 treats the existing world-space Fairy Dust lab as its baseline.

### Phase 4 — Lazy anchor-geometry publication  · status: done (`9c5ad0ee`)

#### Work Order

**Goal:** Entities can anchor to widgets: diegetic publishes current `hana_valence` geometry and transforms only while a widget has attachment demand.

**Spec:**
- Publish `ResolvedAnchorGeometry` (the Hana Valence contract component) **lazily** for world attachments. World demand is nonempty `AnchoredHere`; fill on new demand or widget-rect change, and remove geometry after final world demand ends. Phase 4.5 generalizes retirement to combined world-or-screen demand. Never publish on every widget and never use `Changed<Transform>` as the refill trigger.
- World publication runs in `AnchorSystems::FillGeometry` after Phase 1 reify commands are flushed and before `Resolve`. Reify owns the rect in `Update`; the geometry provider projects it in `PostUpdate` without rewriting it.
- Geometry points are projections of the Phase 3 single rect, expressed in the **widget-local frame** matching the panel provider's centered convention; the resolver composes `global_transform * geometry[anchor]`, which is why the widget's own `Transform` must carry its panel-local offset.
- **World resolver bridge:** ordinary transform propagation runs after valence resolution, and a widget's owner panel may itself move inside that same resolver pass. While a world widget has demand, add a private internal `hana_valence::AnchoredTo` bridge from the widget to its owning panel using the widget rect's current panel-local offset. The widget becomes a real resolver candidate only while demanded: graph order resolves an anchored owner panel first, writes the widget's current transform/`resolved_globals` entry second, then resolves sources targeting that widget. Remove the bridge with final world demand. This covers first spawn, parented panels, same-frame panel motion, and anchored-panel→widget→tooltip chains without resolving every widget every frame; no valence type enters the public widget API.
- **Offsets:** generalize `write_panel_anchor_offsets` around a private `AnchorTargetMetrics::{Panel, Widget}`. A widget target resolves its owning panel through `WidgetOf`, uses that panel's layout-unit conversion, and keeps nonzero x/y/z offsets under translation, rotation, and scale. Do not let the existing `Query<(&DiegeticPanel, &GlobalTransform)>` silently remove widget offsets.
- **Diagnostics:** use `AttachmentResolveDiagnostics`' source/target/reason key when an attachment names missing geometry or a despawned target. World failures already flow through `ResolveDiagnostics`; the screen adapter keeps its coordinate-space-specific reason type over the same bounded diagnostic mechanism.
- **Panel-role teardown:** register demanded widget geometry and the private world bridge with Phase 3's central ownership lifecycle. Removing the owning panel's `DiegeticPanel` must clear `ResolvedAnchorGeometry`, the bridge, `AnchoredHere` reverse demand, and each dependent's widget-target relationship while the application-owned dependent entity survives.

**Files:**
- `src/widgets/reify.rs`, `src/widgets/relationship.rs` — rect ownership and demand transitions
- `src/widgets/mod.rs` — `AnchorSystems::FillGeometry` set membership
- `src/panel/anchoring.rs` — widget-aware offset lowering
- `src/panel/lifecycle.rs` — finalize world geometry demand and bridges before panel-owned widgets are despawned
- `crates/hana_diegetic/examples/widgets.rs` — canonical world-anchor integration exercise
- Read-only: `src/panel/valence_provider.rs` (centered provider convention), `crates/hana_valence` (contract types), `docs/hana_valence/as-built/anchoring-and-arrangements.md`

**Constraints from prior phases:** Phase 3 built the single panel-local rect source and gave widgets a real panel-local `Transform`. Phase 1 reify runs during `Update` and flushes before either screen or world attachment work.

**Acceptance gate:** `cargo nextest run` green with new tests: first-frame and same-frame panel motion place an ordinary world attachment at an off-origin widget corner; an anchored owner panel → widget → dependent chain resolves in graph order in one pass; geometry and the private bridge are absent without world demand, refill on rect change, and are removed after final world demand; two world dependents keep them resident until both detach; removing the owning `DiegeticPanel` under active demand removes geometry, bridge, reverse membership, and dependent relationship while preserving the dependent entity; nonzero pixel and physical-unit offsets survive transformed owning panels; missing-geometry diagnostics deduplicate by source, target, and reason. Screen demand and combined retirement belong to Phase 4.5. Extend and smoke-test the public path in `examples/widgets.rs`.

#### Retrospective

**What worked:**
- `AnchoredHere` demand, centered `WidgetAnchorRect` geometry, and a private widget-to-panel Valence bridge kept publication lazy while preserving graph-ordered same-frame placement.
- Independent ownership records let final demand and panel-role teardown remove Hana geometry and bridge state without deleting an application-replaced relation.
- The canonical world-panel lab exercised the public widget-target path; BRP screenshots and interaction logs confirmed the readout stays below the slider while hover and press status remains visible.

**What deviated from the plan:**
- Newly spawned widgets needed one initial `GlobalTransform` composed from the owner panel and authored local transform so Valence sees inherited scale before ordinary child propagation.
- Widget offset conversion uses the owning panel's propagated global scale rather than the new child widget's temporarily stale `GlobalTransform`.
- The first example placement followed the primary button and covered the secondary button; the final example follows the bottom slider with the same positive gap.

**Surprises:**
- Valence preserves a source's current global scale; an unpropagated scaled child could therefore acquire a compensating inverse local scale and make a self-referential corner test pass while remaining wrong in world space.
- Owner-scale changes refresh only the bridge after transform propagation becomes visible; they must not refill or rewrite unchanged widget geometry.
- Correct lifetime handling requires retiring Hana-owned geometry and the private bridge independently because application code may replace either component.

**Implications for remaining phases:**
- Screen widget-target work must combine screen and world demand without weakening independent component ownership or final-demand retirement.
- Same-frame screen proxy placement must derive scale and target bounds from authoritative owner-panel state rather than assuming a newly reified child's global transform has propagated.
- Tooltip materialization can use the shipped world widget-target path, including nonzero offsets, without adding a second anchoring mechanism.

#### Phase 4 Review

- Phase 4.5 replaces raw-entity attachment authoring with same-space typed handles and rejects panel conversion while any attachment involves that panel or one of its widgets. Cross-space attachment fallback and overlapping world/screen demand are no longer public behaviors to support.
- Phase 4.5 now retires the world bridge, shared geometry, and screen proxy independently; its synchronization runs after both widget reify and screen observer-command fences.
- Phase 4.5's file list now covers the typed attachment boundary, guarded conversions and migrated callers as well as screen scheduling, placement, rects, world target metrics, widget registration, lifecycle, and anchoring documentation.
- Phases 5, 6, 8, and 10.5 now finalize focus, capture, and visible tooltips before anchor cleanup and owned-entity despawn; missing registration/export files were added across later Work Orders.
- Phase 10 carries a deferred owner decision for tooltip anchors, offset builders, defaults, and placement-only diff behavior; Phase 12 now treats the cumulative dual-space widget lab as its baseline.

### Phase 4.5 — Screen-placer widget targets  · status: done (`33229542`)

#### Work Order

**Goal:** Panel attachments are authored through same-space typed handles, coordinate-space conversion refuses active attachment graphs, and screen-space anchored panels can target widgets rather than only panels.

**Spec:**
- **Minimal typed identity:** publicly export the builder's existing `World` and `Screen` marker types and add only two opaque generic entity handles: `PanelEntity<Space>` and `WidgetEntity<Space>`. Each exposes its raw Bevy `Entity` for unrelated ECS work, but has no public unchecked constructor. A handle is minted only after checking the live `DiegeticPanel`, or for a widget after checking `WidgetOf` and its owning panel. Extend `PanelWidgetReader` with typed lookup from a typed owner panel; keep its existing raw entity lookup for non-placement use. Do not add four concrete world/screen panel/widget wrapper types or another public target-handle family.
- **Same-space attachment API:** replace public `AnchoredToPanel::new(target: Entity, ...)` and `retargeted(target: Entity)` authoring with checked methods on the existing `DiegeticPanelCommands` extension for Bevy `Commands`. The source is `PanelEntity<S>` and its panel or widget target carries that same `S`; raw `Entity` is not accepted for attach or retarget. Detach goes through the same extension. Using one ordinary `Commands` parameter for attachment and conversion preserves the order in which calls are written without another public command-wrapper type. The stored `PanelAttachmentAuthored`, screen reverse membership, and Hana Valence relation remain private lowerings. Add compile-fail API coverage showing that a world source cannot attach or retarget to a screen panel/widget and vice versa. Direct insertion or replacement of private lowering components is outside the supported contract and receives no automatic cross-space repair.
- **Checked conversion:** public begin/finish/direct world↔screen methods live on `DiegeticPanelCommands`. They return `Result` only for immediate conversion-recipe validation, then queue one complete mutation. When that queued operation runs, it validates the typed handle against the live panel space and rejects conversion if the panel has an outgoing placement, another panel targets it, or another panel targets any widget it owns. A rejected live operation changes nothing and emits `warn!`. Callers may queue detach and conversion in that order on one `Commands` value; after the command fence they reacquire destination-space handles through `PanelEntityReader` and reattach. No conversion returns a destination handle before the entity actually occupies that space. Make unchecked command-level conversion helpers crate-private. Use `thiserror` for new public preparation errors and pin their messages. Callers that deliberately bypass the checked surface own any inconsistent ECS state they create.
- The screen placer builds candidates from private `PanelAttachmentAuthored` but accepts panel targets only today. Teach it to recognize a widget, resolve the owning screen panel/window through `WidgetOf`, and derive the target rectangle from current widget-local geometry plus the owning panel's screen rect/transform instead of `ScreenPanelRect` on the target. The typed authoring boundary guarantees that source and target share `Screen`; screen-window compatibility remains a runtime placement check. A screen source never carries Hana's world relation.
- Add a private source/target relationship for screen widget attachments, analogous to `AnchoredTo`/`AnchoredHere` but without `linked_spawn`. The same queued attach, retarget, or detach operation updates `PanelAttachmentAuthored` and the reverse target membership together: the old widget loses the source and the new widget gains it before the next queued panel operation runs. Nonempty membership is screen geometry demand, supports multiple sources, and prevents geometry retirement until the last source detaches. Future `TooltipFor` semantic membership does not count as geometry demand before materialization.
- Screen demand synchronization and widget geometry publication run in `Update` after both `WidgetSystems::ReifyCommandsApplied` and `ScreenSpaceSystems::FlushObserverCommands`. Follow synchronization with a named `ScreenSpaceSystems::WidgetDemandCommandsApplied` `ApplyDeferred` set before `PanelSystems::ResolvePanelAttachments`, so widget creation plus command-queued insertion or retargeting is visible in the same frame. This is separate from the world `AnchorSystems::FillGeometry` provider in `PostUpdate`; no ordering claim crosses schedules.
- **Graph dependency proxy:** for every demanded widget, add a private resolver candidate `widget → owning panel`. Its placement action derives the widget rect from `WidgetAnchorRect` plus the owning panel's current resolved `ScreenPanelRect` and transform after owner placement; it never reads the child widget's potentially stale `GlobalTransform`. Real attachments still target the widget candidate. This gives `resolve_attachments` the required owner-panel→widget→dependent order when the owner panel is itself attached, without exposing the proxy as authoring or mutating the widget hierarchy.
- Factor coordinate-neutral owner layout-unit conversion from Phase 4's world-only `AnchorTargetMetrics::Widget`; do not reuse its `world_per_layout_unit` or world-space gate for screen placement. Screen offsets and proxy bounds use the owning screen panel's current resolved scale/rect. A missing owner, window, geometry, or transform yields the screen adapter's source/target/reason diagnostic instead of a panel-only fallback.
- Generalize Phase 4 retirement without coupling independently owned components. Losing final world demand always retires only Hana's private world bridge. Shared geometry retires after the final demand in the widget's current space; valid authoring and guarded conversion mean one widget never needs simultaneous world and screen demand. Losing final screen demand retires only the screen proxy/reverse state unless geometry also has no remaining demand. Application replacement of either the world bridge or geometry is preserved independently.
- Register screen dependency proxies and reverse-demand state with Phase 3's central panel-role ownership lifecycle. Teardown removes each dependent relationship while its target is queryable, then retires Hana-owned bridge/geometry/proxy state before owned widget despawn. It never marks or despawns an application-owned dependent panel. Missing `WidgetOf` after application mutation is outside the attachment contract; cleanup may use Hana ownership records for its own components but does not detach, reparent, or repair application-authored state.
- Keep window and viewport projection in diegetic, but continue delegating graph ordering, cycles, fallback, and diagnostics to `hana_valence::resolve_attachments`. Missing widget geometry uses the screen adapter's `AttachmentResolveDiagnostics` source/target/reason key.

**Files:**
- `src/panel/builder.rs`, `src/panel/mod.rs`, `src/lib.rs` — public space markers, opaque typed handles, and curated exports
- `src/panel/conversion/error.rs`, `src/panel/conversion/screen.rs`, `src/panel/conversion/world.rs`, `src/panel/diegetic_panel.rs` — attachment guard, checked handle transitions, and crate-private raw conversion helpers
- `src/widgets/id.rs` — typed widget lookup through the existing reader
- `src/screen_space/mod.rs` — register the observer, synchronization, deferred-command, and resolve ordering sets
- `src/screen_space/anchoring/mod.rs` — order demand publication after `WidgetSystems::ReifyCommandsApplied` and before attachment resolution
- `src/screen_space/anchoring/candidate.rs` — widget-target candidate rects
- `src/screen_space/anchoring/resolve.rs` — target resolution
- `src/screen_space/anchoring/placement.rs`, `src/screen_space/anchoring/rect.rs`, `src/screen_space/anchoring/projection.rs` — current owner placement, widget bounds, and projection helpers
- `src/widgets/relationship.rs`, `src/widgets/reify.rs` — screen reverse relationship, combined demand, and geometry retirement
- `src/widgets/mod.rs` — screen-demand relation/system registration
- `src/panel/anchoring.rs` — typed public attachment mutation plus private coordinate-neutral lowering and owner-unit conversion
- `src/panel/lifecycle.rs` — finalize screen proxies and combined demand before panel-owned widgets are despawned
- `crates/hana_diegetic/examples/widgets.rs` — canonical screen-target integration exercise
- `crates/hana_diegetic/examples/screen_world_panel_conversion.rs` and existing anchoring callers — migrate internal examples to checked conversions and typed attachments
- `docs/hana_diegetic/as-built/panel-anchoring.md` — replace the screen-widget unsupported notes with the shipped dual-space contract

**Constraints from prior phases:** Phase 1 reifies widget identity early in `Update`; Phase 2 names the post-reify command fence `WidgetSystems::ReifyCommandsApplied`; Phase 3 adds the widget rects and panel-local `Transform`; Phase 4 owns centered `WidgetAnchorRect` geometry, a private world bridge, owner-based world offset conversion, and independent ownership records for bridge and geometry. Phase 4 seeds a new widget's initial global transform once, but screen placement must still use the owning panel's current resolved screen state rather than the child global. Phase 4's raw-entity `AnchoredToPanel` constructor may be broken and all internal callers migrated because `hana_diegetic` has not been published. Screen sources retain private `PanelAttachmentAuthored` and do not enter the world resolver. The existing canonical lab's world readout follows the bottom slider with a positive pixel gap and remains part of the cumulative example.

**Acceptance gate:** `cargo nextest run` green with new tests: the public API has compile-fail coverage for world↔screen attach and retarget mismatches; stale typed handles are rejected before mutation; outgoing, incoming panel-target, and incoming owned-widget attachment cases each reject conversion without changing panel space; one `Commands` buffer preserves attach → conversion, conversion → attach, and detach → conversion call order; detach → conversion → destination-handle reacquisition → reattach succeeds in both directions; command-queued attachment to a new or relaid-out widget target resolves in the same frame after both required fences; an attached, non-unit-scaled, rotated, off-origin owner panel → widget → dependent chain follows graph order using owner-derived bounds and never the child global; placement uses the widget viewport rect and nonzero offset; two same-space screen attachments maintain demand until both detach; retargeting moves reverse membership in the same queued operation; world-demand loss removes only the Hana-owned bridge, screen-demand loss removes only screen state, final no-demand removes geometry, and application replacement of bridge or geometry is preserved independently; removing the owning `DiegeticPanel` under active screen demand removes every proxy, Hana-owned geometry/bridge component, reverse membership, and dependent relationship while preserving application-owned dependent panels; panel targets remain unchanged; missing owner/geometry and same-window warnings deduplicate per source, target, and reason. Migrate every internal attachment/conversion caller. Extend `examples/widgets.rs` with a distinct typed screen-widget target while preserving and smoke-testing the existing typed world readout below the slider; smoke `screen_world_panel_conversion` through one successful round trip with no attachments.

#### Phase 4.5 implementation repair decisions

- Attachment, retarget, detach, and coordinate-space conversion each queue one
  complete operation on the caller's existing Bevy `Commands` value; no
  attachment- or conversion-specific command parameter exists. The operation validates the live ECS state when Bevy
  applies it, then either performs its whole mutation or performs no mutation
  and emits `warn!` with the operation, affected entities, and reason. Runtime
  conflicts such as a missing attachment, stale typed handle, or active
  attachment blocking conversion are programming mistakes rather than public
  result events or receipt values. Calls from one system retain command order;
  ordered application systems use the existing panel system-set boundary when
  they require a particular winner or same-frame visibility. Correctness does
  not depend on application system ordering.
- Widget-target validation includes the owner's authoritative
  `PanelWidgetIndex`, not only the old widget entity's `PanelWidget` and
  `WidgetOf` components. A non-identical `set_tree` clears that index before
  reification, so a widget handle invalidated by an earlier queued tree
  replacement is rejected even while its old entity still exists. Rejected
  attach and retarget operations leave authored and reverse attachment state
  unchanged.
- A screen-widget attach, retarget, or detach updates its private forward and
  reverse relationship in the same queued operation as
  `PanelAttachmentAuthored`. A rejected retarget or detach leaves both records
  unchanged; a successful retarget removes the source from the old widget before
  a later queued panel operation can inspect the graph.
- Conversion preparation is separate from conversion mutation. Constructing a
  resolved conversion or projecting a panel through a camera remains an
  immediate, read-only `Result`, so recoverable missing camera/window data and
  invalid dimensions can be handled or retried by the caller. Only an already
  valid conversion is queued for mutation; an unexpected live-state conflict
  at execution performs no mutation and emits `warn!`. Do not keep a combined
  convenience whose `Ok` reports successful preparation while the queued
  conversion can still fail.
- A queued conversion returns no destination-space handle. After
  `PanelSystems::ApplyConversions`, the caller reacquires the live
  `PanelEntity<Screen>` or `PanelEntity<World>` through `PanelEntityReader`.
  Returning the destination handle before execution would claim a coordinate
  space the entity does not yet occupy and might never occupy; do not add a
  speculative or pending-handle type.
- `PanelWidgetReader::typed_entity` validates the typed owner against
  `PanelSpace` so the reader remains usable in the same system as the
  `PanelWidgetWriter` that mutably accesses `DiegeticPanel`. Make `PanelSpace` a
  required panel component and synchronize it both when a whole
  `DiegeticPanel` is inserted or replaced and at in-place conversion apply
  points. The regression first proves that a world owner resolves its widget,
  then replaces that owner with a screen panel and proves the mirror updates and
  the old world handle returns `None`.

#### Retrospective

**What worked:**
- Methods on one ordinary Bevy `Commands` value preserve written operation order while typed panel and widget handles prevent cross-space attachment calls.
- One private screen-widget reverse relationship supplies both graph dependency and geometry demand without exposing a second public target abstraction.
- The canonical widget lab and conversion example made both attachment spaces and a complete world-to-screen-to-world round trip observable through BRP.

**What deviated from the plan:**
- Conversion preparation remains an immediate `Result`, but the validated mutation is queued separately and returns no speculative destination handle.
- `PanelWidgetReader` needed a synchronized `PanelSpace` component so typed lookup can coexist with mutable panel access in the same system.
- The canonical screen demonstration needed a narrower control and shorter attached-panel label so both panels remain visible at the top-right window edge.

**Surprises:**
- A stale widget entity can remain alive until reification even after `set_tree`; checking the owner's live `PanelWidgetIndex` is required to reject its old typed handle before mutation.
- A queued conversion's new coordinate space may not be visible to an immediate BRP query until the next application frame, even though operation order remains deterministic.

**Implications for remaining phases:**
- Later widget systems should use the synchronized `PanelSpace` mirror and existing typed readers rather than borrowing `DiegeticPanel` directly merely to determine coordinate space.
- Later queued widget operations should expose immediate `Result` values only for preparation that has actually completed; applied ECS mutations remain observable after their documented command fence.
- Tooltip materialization can use the single typed attachment path for both screen and world targets and must reacquire handles after any conversion.

#### Phase 4.5 Review

- Delegation Context and later Work Orders now name the synchronized `PanelSpace` mirror, typed handle reacquisition, and the actual combined world/screen anchor cleanup point.
- Phase 10.5 now stages lazy materialization through panel insertion, handle acquisition, checked attachment, coordinate-specific placement readiness, and final transform propagation.
- Phase 10.5 now inherits a screen target's window, camera order, and render layers and rematerializes the same tooltip controller when retargeting changes coordinate space or layout unit.
- Phase 5's pointer-focus and semantic-routing decisions and Phase 5.5's adapter decisions are now settled in their respective work orders; Phase 10 decisions remain deferred to that phase gate.

### Phase 5 — Focus subsystem and semantic routing  · status: done (`6c59c602`)

#### Work Order

**Goal:** Window-scoped focus and binding-library-independent semantic requests work across all widgets with deterministic panel-local traversal.

**Spec:**
- `widgets/focus.rs`. Focus is shared, not button-local. One crate-private authoritative resource maps each window to its active panel, focused widget, and focus-indicator visibility; marker state is never an independent authority. The exact public request, clear, change, cause, and semantic-action types are fixed in the public contract ledger.
- `WidgetFocusable` participation component, inserted only when a widget entity is first spawned; removing it opts that live widget out of keyboard traversal without changing pointer picking. Same-id reify, reorder, authored refresh, and kind-preserving updates must not restore a deliberately removed marker.
- `WidgetFocused(())` is a public read-only presence marker with a private field, synchronized only by one focus-transition function. It records the retained keyboard-input target, independently of whether that focus should currently be drawn. A crate-private `WidgetFocusVisible(())` marker is present when at least one window focuses that widget with a visible indicator. Pointer focus retains `WidgetFocused` but hides the indicator; traversal and direct application requests show it, including when traversal selects the already-focused widget. Indicator-only changes emit no `WidgetFocusChanged`, because the semantic target did not change. Public app control uses typed request/clear events carrying the window and target; `WidgetFocusChanged` reports old/new entities and a cause. `RequestWidgetFocus` remains entity-targeted: callers starting from an authored `(panel, id)` resolve it through `PanelWidgetReader`, while pointer and existing event flows already have the entity. There is no parallel id-targeted focus request or public focus-visible type.
- Focus is gained by pointer focus, traversal, semantic routing, or app request. It is lost by transfer, despawn/removal, `WidgetFocusable` removal, panel/window input-scope loss, or explicit clear — **not** by disable. Disabled focusable widgets may retain or receive focus and participate in traversal; behavior modules ignore activate/change input while disabled.
- Traversal order is the current `ComputedWidgetRecord` preorder rebuilt in Phase 1, never `PanelWidgets` relationship insertion order. Next/previous/first/last stay within the active panel for that window and wrap deterministically; focusing a widget on another panel transfers the active panel. Structural reorder changes traversal without respawning entities.
- Define the six semantic action types: next, previous, first, last, activate-focused, cancel-focused. Core focus requests/events and their observable routing do not depend on a binding library; Phase 5.5 only translates enhanced-input action edges into this core path.
- Add `WidgetSystems::InteractivityCommandsApplied`: an `ApplyDeferred` fence after Phase 2's `ResolveInteractivity`. Focus/semantic behavior and every later button, slider, and tooltip behavior system that reads `WidgetDisabled` orders after this fence, so same-frame marker insert/remove commands are visible before behavior runs.
- **Ordering and IME:** pointer focus is visible before same-frame activate handling. Semantic widget input runs after `ImeSystemSet::PublishInputBlockers` and ignores a window while `ImeInputBlocker::blocks_window(window)` is true.
- **Panel-role teardown:** Phase 3's central lifecycle calls the focus transition before Phase 4.5's combined `finalize_widget_anchor_state` cleanup and before despawning panel-owned widgets. It removes `WidgetFocused` and the private indicator marker, clears that window's active-panel/widget entry, and emits exactly one `WidgetFocusChanged` with `WidgetRemoved` while the target, `WidgetOf`, and both world and screen attachment relations remain queryable.
- Design with accessibility in mind (structure the traversal so an a11y layer can attach later), without adding bevy_a11y.

**Approved decision: core semantic-action routing contract**

Approved foundation:
Focus is per-window. Each window remembers its own active panel and focused widget. Direct pointer interaction still targets a widget directly; only operations that act on remembered focus need to identify the window whose focus they use.

Actual problem:
Given the approved per-window focus model, the public ledger names six semantic action types but does not say whether they are binding-library action markers, app-sendable window-scoped requests, or entity-targeted events. Without that contract, Phase 5 cannot prove how `ActivateFocusedWidget` reaches the currently focused widget, and Phase 5.5 cannot be a replaceable adapter.

What exists now:
- Focus authority is window-scoped and the focused widget is crate-private state.
- Pointer/app focus requests already use typed public events, while `bevy_enhanced_input` is intended to remain optional.
- Button and slider behavior need one entity-targeted routed intent, not knowledge of input contexts or bindings.

What should change:
- Freeze one library-independent public request shape carrying the window for next/previous/first/last/activate/cancel.
- Define one private routed intent carrying the resolved widget entity that later behavior modules consume after focus and IME gating.
- Make the enhanced-input adapter translate action edges into the public/core request path rather than becoming a second authority.

Decision:
The six exported semantic types are app-sendable window-scoped messages. Focus authority resolves the named window to its focused widget and emits one private entity-targeted semantic intent. Applications and headless tests use the same public path, while `bevy_enhanced_input` remains an optional Phase 5.5 translator rather than a second focus or routing authority.

**Approved decision: pointer focus from non-window render targets**

Actual problem:
Focus authority is keyed by `Window`, but a picked widget can come from a camera that renders to an image or another non-window target. Phase 5 must define whether that pointer interaction can establish window-scoped focus.

What exists now:
- Diegetic picking retains the camera and its normalized render target for each hit.
- A camera targeting a real window identifies the exact focus scope; a non-window target does not.

What should change:
- Derive pointer-focus scope from the hit camera's normalized window target.
- Ignore pointer focus for a non-window render target while leaving explicit app focus requests available when the application supplies a window.

Decision:
Automatic pointer focus is available when the picked camera resolves to a window. A hit through an image, texture view, or other non-window render target can still interact directly with the exact widget, but it leaves remembered keyboard focus unchanged. The library does not assume the primary window or require a render-target mapping system. An application that later needs keyboard focus for such a surface can use the ordinary explicit focus request and name the intended window.

**Files:**
- `src/widgets/focus.rs` — new
- `src/widgets/input.rs` — core semantic request types and routing only
- `src/widgets/mod.rs` — systems in `WidgetSystems` after picking; post-interactivity deferred fence
- `src/widgets/reify.rs` — default `WidgetFocusable` insertion
- `src/panel/lifecycle.rs` — invoke focus finalization before owned widget teardown
- `src/lib.rs` — curated focus, request, and change-event exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical focus and semantic-routing exercise

**Constraints from prior phases:** Phase 2 supplies `WidgetDisabled`; Phase 3 supplies pick targets and `PickingInteraction`; Phase 1 supplies current traversal order. Phase 4.5's central panel-role path calls `finalize_widget_anchor_state` for combined world/screen attachment cleanup before owned widget despawn, so focus finalization must run first while both attachment relation forms remain queryable. Activation of a focused button lands in Phase 6; this phase routes the action to that later behavior hook.

**Acceptance gate:** `cargo nextest run` green with new tests: next/previous/first/last and wrap order; structural reorder updates order while preserving entities; two windows hold isolated focus; an app focus request can target a widget resolved from `(panel, id)`, while pointer focus uses its existing entity; pointer focus retains `WidgetFocused` without the private indicator, and traversal restores the indicator even when the target remains unchanged; app clear and change causes; focus loss on despawn, `WidgetFocusable` removal, and explicit clear; removing an owning panel role clears authority and both focus markers while emitting exactly one `WidgetRemoved` transition before anchor relationships or `WidgetOf` are removed and before the target is gone; removing `WidgetFocusable` survives same-id reify and reorder; disabled widgets retain and can receive focus but activate is a no-op; a same-frame interactivity edge is applied before semantic behavior; pointer-focus plus activate works in one frame; IME blocks semantic actions only in its leased window; the resolved semantic intent names the focused entity exactly once. Extend and smoke-test the public path in `examples/widgets.rs`.

#### Retrospective

**What worked:**
- `WidgetFocusAuthority` now keeps independent active-panel, focused-widget, and indicator-visibility state per window. `WidgetFocused` remains the public derived “focused somewhere” marker, while a private marker draws focus only for traversal or application focus.
- The six public window-scoped messages route through one private entity-targeted `SemanticWidgetIntent`; disabled and IME-blocked input is stopped before later behavior modules see it.
- Current computed preorder drives deterministic traversal without respawning widgets, and removing `WidgetFocusable` remains an intentional opt-out across later reify passes.

**What deviated from the plan:**
- Full-entity panel despawn required an additional `On<Despawn, DiegeticPanel>` observer registered from `panel/mod.rs`; `On<Remove, DiegeticPanel>` alone is sufficient only for component-only role removal.
- Review repairs changed pointer focus to resolve the winning hit camera's render target and changed marker cleanup to tolerate an already-despawned widget without an invalid-entity warning.

**Surprises:**
- A pointer's own render target is not sufficient evidence for focus: custom ray routing can produce a widget hit from a camera targeting a different window or an offscreen texture.
- Bevy queues linked-child despawns from the parent's `on_despawn` hook before deferred commands from `Remove` observers, so the focus event must be queued from an earlier `Despawn` observer to preserve queryable widget relationships.

**Implications for remaining phases:**
- Phase 5.5 translates optional input bindings only into the six shipped window-scoped messages; it does not read or mutate focus authority.
- Button and slider behavior consume the shipped private `SemanticWidgetIntent` after `WidgetSystems::InteractivityCommandsApplied`; they do not duplicate window, IME, disabled, or focus lookup logic.
- Any later terminal event that must inspect widget ownership or attachment relationships must run before both component-only panel cleanup and full linked-child despawn, following the two-path lifecycle established here.

#### Phase 5 Review

- Phase 5.5 targets the six shipped window-scoped messages exactly. Its per-window modes, automatic defaults, focused-window routing, default bindings, controller rule, Bevy Kana manual path, and display-only control summary are now owner-approved.
- Phase 6 consumes only the resolved focused-widget `SemanticWidgetIntent::Activate`; the plan no longer implies an unshipped arbitrary entity-targeted semantic activation API.
- Phase 7.5 adds `WidgetSystems::FocusCommandsApplied` after semantic input so same-frame marker changes are visible to presentation and tooltip eligibility.
- Phases 6, 8, 10, and 10.5 now extend both lifecycle paths established by Phase 5: ordinary component-role removal and early full-entity despawn before linked-child cleanup. Tooltip target despawn has the same early-finalization requirement.
- Phase 10 records the missing synchronous validation contract for `.tooltip(...)` on a non-widget element as an owner decision rather than leaving failure behavior implicit.
- The former Phase 10.5 was split without changing behavior: Phase 10.5 owns initial timing, materialization, readiness, and teardown; Phase 11 owns replacement and retargeting.

### Phase 5.5 — Enhanced-input adapter  · status: done (`4b6e8866`)

#### Work Order

**Goal:** An opt-in `bevy_enhanced_input` adapter gives each window an independent binding mode while feeding only Phase 5's core semantic path. Applications may instead keep their own enhanced-input contexts and Bevy Kana action macros and send the same six core messages directly.

**Spec:**
- Add the direct workspace dependency and expose `WidgetInputPlugin`, `WidgetInputMode`, `WidgetInputBindings`, `WidgetInputBindingsBuilder`, `WidgetInputBindingsError`, `WidgetInputDisabled`, and display-only `WidgetControlSummary` from the public contract ledger. No raw key handling lives in widgets.
- `WidgetInputMode` is a component on `Window` with three choices: `Default`, `Bindings(WidgetInputBindings)`, and `Manual`. `Default` installs Hana's standard controls for that window. `Bindings` installs that window's supplied controls. `Manual` installs nothing: the application may use ordinary Bevy Enhanced Input contexts and Bevy Kana's `action!`, `event!`, `bind_action_system!`, and `Keybindings` helpers to send Phase 5's six window-scoped messages.
- `WidgetInputDisabled` is a presence component on `Window`. Adding it removes that window's adapter-owned context/action entities while preserving `WidgetInputMode` and remembered widget focus; removing it restores the selected mode. Removing `WidgetInputMode` or the window removes every adapter-owned input entity for that window and also leaves core focus state untouched unless the window itself was removed.
- Mode and binding changes reconcile that window's action/context entities by diff. Repeating install/rebind/disable/remove is a no-op.
- Lower each authored shortcut through Bevy Enhanced Input's own action conditions. Do not keep a widget-owned completion observer or suppression table. Bevy Kana supplies the shared one-shot shortcut installer used by the built-in adapter and available to application-owned keymaps: it tracks the unmodified physical input edge, represents required modifiers with `Chord`, excludes additional modifiers with `BlockBy`, and applies `Press` so releasing a modifier while the main key remains held cannot become a new shortcut press. Modifier actions continue reflecting physically held modifiers when a window context becomes active, while the main physical edge still requires a fresh press. A modifier key used as the shortcut's primary input never blocks itself. The winning semantic action may consume its unmodified physical input for lower-priority contexts. Keep one semantic action entity per authored binding and deduplicate simultaneous alternatives only when emitting the six core messages.
- Adapter action processing runs after enhanced-input action resolution and `ImeSystemSet::PublishInputBlockers`, but before Phase 5's `WidgetSystems::SemanticInput`. It only writes Phase 5's six public messages; core routing remains the final IME/disabled/focus gate and the adapter never emits `SemanticWidgetIntent` itself.

**Approved decision: per-window binding and adapter-disable API**

Actual problem:
The earlier `WidgetInputPlugin::new(bindings)` draft defined only initial installation while promising later rebind/remove/disable behavior without naming any public mutation surface. A delegate could not implement or test idempotent reconciliation until that surface was fixed.

What exists now:
- `WidgetInputBindings` and `WidgetControlSummary` are promised exports.
- The plugin owns per-window enhanced-input context/action entities.
- Phase 5 owns the semantic request contract and remains usable without this plugin.

What should change:
- Name one public source of desired adapter configuration and enabled state.
- Specify whether removing/disabling the adapter preserves focus state (recommended) while only deleting adapter-owned context/action entities.
- Define how apps perform a runtime rebind and how invalid/no-op updates are reported.

Decision:
Use `WidgetInputMode` and `WidgetInputDisabled` components on each `Window`, following the established per-camera input-mode pattern in `bevy_lagrange`. There is no global mutable widget-input settings resource. Runtime rebind changes only that window's `WidgetInputMode`; temporary disable adds `WidgetInputDisabled`; both preserve the window's remembered widget focus. `Manual` is the explicit integration path for applications that already own Bevy Enhanced Input contexts and Bevy Kana action bindings. Those applications send Phase 5's existing window-scoped semantic messages, so custom input and Hana's built-in adapter do not create separate widget behavior paths.

**Approved decision: automatic default-mode installation**

Actual problem:
The per-window mode fixes where customization lives, but the plan still must say whether adding `WidgetInputPlugin` automatically gives every window `WidgetInputMode::Default` or whether each window must opt in by adding that component itself.

What exists now:
- The application has already opted into Hana's adapter by adding `WidgetInputPlugin`.
- Per-window `Bindings` and `Manual` overrides remain available either way.
- Core focus and semantic messages work without the adapter.

Decision:
When `WidgetInputPlugin` is present, insert `WidgetInputMode::Default` for each existing or newly created window that does not already specify a mode. This makes the common one-window case work immediately while preserving per-window custom bindings and the explicit `Manual` escape hatch.

**Approved decision: keyboard window routing**

Actual problem:
Enhanced-input key edges are application-global, while Phase 5 remembers widget focus separately for every window. If every per-window context remains active, pressing Tab once can move widget focus in every window.

What exists now:
- Phase 5's six messages each require one exact window and keep that window's focus independent.
- Bevy reports which operating-system window currently has keyboard focus.
- `WidgetInputMode` controls which bindings a particular window uses; it does not by itself choose which window receives a particular key press.

What should change:
- Activate keyboard bindings only for the operating-system-focused window.
- Changing operating-system window focus must not clear either window's remembered widget focus.

Decision:
Route keyboard input only to the operating-system-focused window. When the user switches windows, the newly active window resumes from the widget it previously remembered; the inactive window retains its widget focus but does not respond to keys.

**Approved decision: default controls**

Actual problem:
`WidgetInputMode::Default` promises working controls without application setup, so each of the six Phase 5 actions needs a concrete standard binding.

Decision:
Use Tab for next, Shift+Tab for previous, Home for first, End for last, Enter and Space for activate, and Escape for cancel. `WidgetInputMode::Bindings` may replace any of these per window, while `Manual` remains entirely application-owned.

**Approved decision: controller window routing**

Actual problem:
A gamepad action does not identify a window. The adapter therefore cannot send Phase 5's `{ window }` message until it selects one.

Decision:
Send built-in gamepad actions to the operating-system-focused window, matching keyboard behavior. If there is not exactly one focused live window, emit no widget action because the adapter cannot choose safely. An application that wants a different rule can use `WidgetInputMode::Manual` and send the six core messages with its own selected window.

**Approved decision: display-only control summary**

Actual problem:
An application's help panel may need to say “Tab: next” and “Enter / Space: activate.” If that window is rebound, hard-coded help text becomes wrong. The help panel should not have to inspect Bevy Enhanced Input action/context entities to discover the effective labels.

Decision:
Keep this deliberately smaller than Lagrange's camera summary. `WidgetInputMode::control_summary()` returns a point-in-time `WidgetControlSummary` with exactly six public fields: `next`, `previous`, `first`, `last`, `activate`, and `cancel`, each a `Vec<String>` of ready-to-display binding labels. It is an ordinary derived value, not a component, resource, or runtime authority; widget input never reads it. `Default` and `Bindings` describe their effective controls, while `Manual` returns empty fields because the application owns both input and help text. Calling the method again after a rebind produces the updated labels. Do not add public action-row, source, or enhanced-input entity types for this display-only use case.

**Approved decision: custom-binding construction and conflicts**

Actual problem:
The plan shows `WidgetInputBindings` as a value but does not define how applications construct it or what happens when one physical binding is assigned to two widget actions. Allowing Tab to mean both “next” and “activate,” for example, could move focus and activate the newly focused widget from one press.

Decision:
Expose `WidgetInputBindings::builder()` returning public `WidgetInputBindingsBuilder`, with consuming fluent `next`, `previous`, `first`, `last`, `activate`, and `cancel` methods accepting `impl Into<bevy_enhanced_input::Binding>`. Repeating a method adds an alternate binding, so Enter and Space can both activate. Omitted actions remain unbound. `build()` returns `Result<WidgetInputBindings, WidgetInputBindingsError>`; it deduplicates repeats within one action, rejects `Binding::None` with `WidgetInputBindingsError::NoneBinding`, and rejects an exact binding assigned to different actions with `WidgetInputBindingsError::ConflictingBinding(Binding)`. The error derives `thiserror::Error`. Bevy Enhanced Input's own `Binding` display implementation supplies the control-summary labels, so no parallel label API is needed. `WidgetInputBindings::default()` directly returns the approved standard bindings. Enhanced Input conditions select the exact declared modifier set and consume the winning shortcut, preserving Shift+Tab as previous rather than also firing Tab as next or manufacturing a later Tab press when Shift is released.

**Approved decision: native modifier handling and future keymap boundary**

Actual problem:
The first implementation tried to repair modifier-release handoff after Bevy Enhanced Input had already emitted action transitions. It recorded completed modified actions in a widget-owned suppression table. That table could suppress an unrelated alternative assigned to the same semantic action, and it could not reliably distinguish two alternatives that both produce the same semantic action.

What exists now:
- `../hana/crates/hana` already owns Bevy Enhanced Input contexts and uses Bevy Kana's action/event macros and `Keybindings` helper for application shortcuts.
- `bevy_lagrange` already keeps authored binding configuration separate from its private Bevy Enhanced Input installation.
- `WidgetInputBindings` already stores native `bevy_enhanced_input::Binding` values, derives `Reflect`, and can be replaced per window without changing Phase 5's semantic messages.
- Bevy Enhanced Input provides `Press`, `Chord`, `BlockBy`, and input consumption. Its plain `consume_input` ordering prevents a modified chord and its bare counterpart from firing together while the modifier is held, but does not by itself make releasing the modifier a new physical-key edge.

Decision:
Keep the same separation used elsewhere in Hana. `WidgetInputBindings` is authored, reflected keymap data; private Bevy Enhanced Input entities are only its current runtime installation; the six Phase 5 messages remain the device-independent widget boundary. A future Hana keymap may either replace a window's `WidgetInputMode::Bindings` value or select `Manual` and own the Enhanced Input contexts itself.

Put reusable one-shot shortcut lowering in `bevy_kana::Keybindings`, not in a widget-specific suppression system. Add `spawn_shortcut` for one-shot runtime keymap data, while preserving the existing held-input behavior of `spawn_key`, `spawn_shift_key`, `spawn_platform_key`, and `spawn_binding`; callers continue choosing `Start` for a discrete response or `Fire` for a continuous response. For each one-shot physical shortcut, spawn a non-consuming edge action for the binding with its modifier keys removed; the semantic action requires that fresh edge plus its declared modifier actions, is blocked by every undeclared modifier action except a modifier key that is itself the primary input, and consumes the winning physical input according to its `ActionSettings`. Modifier actions must report the currently held state immediately after a context becomes active, but an already-held main input must not manufacture a fresh shortcut press. This keeps all modifier matching and input consumption inside Bevy Enhanced Input while ensuring that releasing Shift from Shift+Tab does not manufacture a fresh Tab press. The widget adapter uses `spawn_shortcut` and only deduplicates multiple genuine starts that map to the same semantic message in one frame.

A single Shift, Control, Alt, or Super press is a valid primary shortcut. A timed sequence such as Control followed by Control is also a valid future keymap gesture, but it is not one `Binding`; a future keymap layer can lower that sequence through Bevy Enhanced Input's `Combo` condition without changing the six widget messages or making widgets own timing machinery.

**Files:**
- `src/widgets/input.rs` — enhanced-input action/context adapter and public per-window mode/disable surface
- `src/widgets/mod.rs` — adapter ordering and registration
- `crates/hana_diegetic/Cargo.toml` — direct `bevy_enhanced_input` dependency
- `crates/bevy_kana/src/input/keybindings.rs` — shared one-shot binding lowering for built-in and future keymap-created shortcuts
- `crates/bevy_kana/src/input/` tests — exact-modifier, modifier-release, and consumption regressions
- `src/lib.rs` — curated adapter exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical enhanced-input exercise
- Read-only references: `crates/bevy_lagrange/src/input/`, `../hana/crates/hana/src/input/`, and `../hana/crates/hana/tests/bei_chord_overlap.rs`

**Constraints from prior phases:** Phase 5 ships private `WidgetFocusAuthority`, `WidgetSystems::SemanticInput`, six public `{ window }` messages, and private entity-targeted `SemanticWidgetIntent`. The adapter writes only those public messages after enhanced-input resolution and before `WidgetSystems::SemanticInput`; it never reads or mutates focus authority, performs widget lookup, emits the private intent, or exposes enhanced-input types through core behavior APIs. Keep the direct `bevy_enhanced_input` dependency because the adapter owns Enhanced Input contexts and accepts its `Binding` values. Bevy Kana remains the shared convenience layer for both held actions and one-shot shortcuts; it does not become widget focus or behavior authority. The canonical example must exercise both the built-in per-window adapter and an app-owned Bevy Kana action that sends one of the same core messages, demonstrating that a future app keymap does not require a second widget API.

**Acceptance gate:** `cargo nextest run` green with new tests: automatic default installation creates one context per live window; `Default`, `Bindings`, and `Manual` reconcile only the selected window; the builder adds alternatives, permits omitted actions, deduplicates same-action repeats, rejects `Binding::None`, and rejects cross-action conflicts with the exact approved error variants; Shift+Tab emits previous without also emitting next, and releasing Shift while Tab remains held emits nothing; a semantic action bound to both Shift+Enter and Enter emits once for the modified press and nothing on modifier release; releasing Shift while pressing an unrelated alternative such as N still emits N's semantic action; Ctrl+Shift+Enter does not hand off to Ctrl+Enter when Shift is released; holding Shift while focus moves to another window and then pressing Tab emits previous in the newly focused window, while moving focus with Shift+Tab already held emits nothing until a fresh Tab press; each modifier key works as a primary one-shot shortcut without blocking itself; existing `spawn_key` held behavior still emits `Fire` on successive held frames; a consumed winning shortcut is unavailable to a lower-priority Enhanced Input context; simultaneous alternatives for one semantic action emit one core message; one action edge emits the corresponding Phase 5 message for exactly the operating-system-focused window; an app-owned Bevy Kana action can send the same core message without adapter-owned input entities; IME blocks routed behavior only in its leased window; adding/removing/focusing windows reconciles context activation without clearing remembered widget focus; runtime rebind replaces reflected binding data and updates contexts without duplicates; repeated equal configuration is a no-op; `WidgetInputDisabled` removes plugin-owned context/action entities without clearing focus or mode; re-enable restores exactly one installation per window; built-in gamepad input targets the single operating-system-focused window and emits nothing when no unique focused window exists; `WidgetControlSummary` has exactly the six approved display fields, reflects defaults and runtime rebinds, is empty for `Manual`, and exposes no enhanced-input entities; removing `WidgetInputMode` follows the settled public contract. Extend and smoke-test both the built-in and app-owned Bevy Kana paths in `examples/widgets.rs`.

#### Retrospective

**What worked:**
- `WidgetInputPlugin` keeps adapter-owned contexts per window and translates only into Phase 5's six public messages; `Manual` leaves the same behavior path available to application-owned input.
- `bevy_kana::Keybindings::spawn_shortcut` centralizes one-shot runtime shortcuts, while the existing four installation helpers retain their held-input behavior.
- The canonical example proves both paths: Hana's Tab controls and an app-owned Bevy Kana `P` action move the same per-window widget focus.

**What deviated from the plan:**
- Exact one-shot behavior required a shared physical-edge action per unmodified binding rather than relying on semantic-action completion state.
- Primary modifier shortcuts required private left/right modifier actions for blocking in addition to family-wide actions used to satisfy declared modifiers.

**Surprises:**
- Modifier actions must remain live across context activation so an already-held Shift affects a fresh Tab press, while the unmodified main-key edge must require reset so an already-held Tab does not retrigger.
- A family-wide modifier action cannot distinguish the exact primary key from the opposite-side key; exempting the whole family made Left Shift fire while Right Shift was held.

**Implications for remaining phases:**
- Button, slider, and tooltip behavior consume the existing private `SemanticWidgetIntent`; they do not inspect Enhanced Input contexts, bindings, or focus authority.
- Future Hana keymaps should use `spawn_shortcut` for discrete runtime shortcuts, retain the existing helpers for held controls, and lower timed sequences through Enhanced Input conditions such as `Combo` outside widgets.
- Later canonical-example work can add behavior behind both built-in and application-owned bindings without adding another widget input API.

#### Phase 5.5 Review

- Phases 6–12 remain distinct and keep their existing order; Phase 5.5 stops at translating input into Phase 5's six messages.
- Phase 6 and Phase 8 now state exactly how `SemanticWidgetIntent::Cancel`, including the built-in Escape binding, ends an active capture.
- Phase 8 now demonstrates continuous app-owned held input by repeatedly sending `RequestSliderAdjustment` through Bevy Kana's retained held-action path.
- Phase 7.5 and Phase 10.5 now state that `WidgetSystems::FocusCommandsApplied` already includes both built-in and app-owned message effects; no adapter-specific fence is added.
- Delegation Context and Phase 12 now preserve the canonical example's built-in adapter and app-owned Bevy Kana paths through every later phase.

### Phase 6 — Button behavior  · status: done (`8e0cd250`)

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
- **Emulated capture:** a private resource maps each occupied `PointerId` to one capture entry containing the widget, id, private press sequence, and typed terminal state (`Pending`, `Release(outcome)`, `Cancel(cause)`). A second press for an occupied pointer or on an already-captured widget is ignored. Every global release/cancel/drag-end path matches both the captured pointer and widget before acting. The private `ButtonPress` component is the lifecycle marker for the entry. Every valid primary `Pointer<Press>` target increments a private sequence before capture checks, and the accepted capture stores that sequence so reconciliation can distinguish it from a later rejected press.
- **Terminal choke point:** set the capture entry's terminal state before removing `ButtonPress`; its remove/despawn hooks consume the entry and emit `Released` plus optional `Clicked`, or `Canceled`. `Pending` removal is cancellation. Widget/kind removal runs finalization and targeted event dispatch while the entity still exists, then despawns/removes the behavior bundle. Do not queue an entity-targeted terminal event after its target is gone.
- **Panel teardown order:** extend both Phase 5 lifecycle paths. Component-only role removal finalizes button capture from the ordinary `On<Remove, DiegeticPanel>` teardown before Phase 4.5's combined `finalize_widget_anchor_state`; full panel despawn finalizes from the earlier `On<Despawn, DiegeticPanel>` observer before Bevy queues linked-child despawns. Emit exactly one terminal event while `WidgetOf` and both world and screen attachment relations remain queryable, with duplicate suppression when full despawn subsequently reaches the remove observer.
- **Pointer loss:** Bevy supplies the normal targeted lifecycle: `Pointer<Click>` and `Pointer<Release>` use `PreviousHoverMap`, while `Pointer<DragEnd>` uses Bevy's dragging state. Those observers are authoritative and remove Hana's capture before the fallback runs. `WidgetsPlugin` schedules the fallback only when `Messages<PointerInput>`, `PointerState`, `HoverMap`, and `PickingSettings` exist, so `PickingPlugin` without `InteractionPlugin` is valid composition. In `PickingSystems::Last`, Hana reads the frame's primary raw actions in global order. Surviving release/cancel captures are marked terminal first. When hover processing ran, a raw release over the captured widget emits released and clicked; a release elsewhere emits `ButtonCanceled(CaptureLost)`. When picking or hover processing was disabled, a raw release treats the stale capture as lost and emits `ButtonCanceled(CaptureLost)` without consulting stale hover or press state. After all terminal removals, final Bevy presses are considered in raw order and may establish only Hana's private capture plus `ButtonPressed`; a press can claim a widget only if its raw action occurred after the release/cancel that freed both its pointer and widget. Raw `PointerAction::Cancel` and pointer removal remain exact-once terminal fallbacks because targeted `Pointer<Cancel>` can have no target over empty space. Because Bevy documents `PointerAction::Cancel` as terminal, Hana warns about and ignores every later raw action for that pointer while preserving the first cancel.
- **Disable-while-pressed:** inserting `WidgetDisabled` on a pressed button must actively remove the live `ButtonPress` with a Canceled cause — a flag alone lets the pending Release/DragEnd resolve as Clicked. Disabled buttons ignore pointer and semantic activation and cannot keep capture.
- **Semantic activation:** observe Phase 5's private `SemanticWidgetIntent::Activate`, which already names the focused, enabled widget after window, IME, disabled, and focus routing. When that target is a button, emit `ButtonClicked` directly with no fabricated pointer events. Phase 6 does not invent a second public arbitrary-entity activation request; applications that already know a button entity use the public button event/API surface settled by this phase.
- **Semantic cancel:** `SemanticWidgetIntent::Cancel` terminates an active `ButtonPress` on its resolved target exactly once with the stored `PointerId` and `Explicit` cause. It is a no-op when that button has no active capture; the built-in Escape binding and an application-written `CancelFocusedWidget` message use this same path.
- **IME coexistence:** the Phase 3 ordered hit group makes the widget the event target. Before the widget stops click propagation, call a factored IME blur-classification helper with `WidgetOf::panel()` so clicking a button commits an editor outside that focus scope. Then stop propagation so the panel's double-click field activator cannot open a field underneath the button.
- Presentation state comes from `PickingInteraction`; no bespoke hover events in the first API.

**Files:**
- `src/widgets/button.rs` — extend the Phase 1 authoring module with button behavior
- `src/widgets/mod.rs` — observers + systems registration
- `src/panel/lifecycle.rs` — finalize active button capture before anchor and owned-entity cleanup
- `src/panel/mod.rs` — extend the early full-panel-despawn finalizer registration
- `src/ime/editor.rs` — shared widget-aware blur classification
- `src/lib.rs` — curated button event and cancellation exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical button-lifecycle exercise
- Read-only: `src/ime/activation.rs`, `/Users/natemccoy/rust/bevy/crates/bevy_ui_widgets/src/button.rs`

**Constraints from prior phases:** Phase 1 owns `Button` authoring, same-kind entity reuse, and Phase 1-owned kind replacement/removal. Extend those transitions here so button behavior is finalized before a kind change or removal. Phase 2 supplies `WidgetDisabled`; Phase 3 supplies ordered hits and `PickingInteraction`; Phase 4.5 supplies combined world/screen attachment cleanup through `finalize_widget_anchor_state`. Phase 5 supplies `WidgetSystems::InteractivityCommandsApplied`, private `SemanticWidgetIntent::{Activate, Cancel}`, and the paired panel lifecycle paths: ordinary `On<Remove, DiegeticPanel>` for component-only role removal plus earlier `On<Despawn, DiegeticPanel>` finalization before linked children. Button behavior consumes the resolved intent without repeating window/focus/IME/disabled lookup, and capture finalization extends both lifecycle paths while both attachment relation forms remain queryable. The optional Phase 5.5 adapter feeds the public message route but is not a behavior dependency.

**Acceptance gate:** `cargo nextest run` green with new tests: press→release→click and release-without-click; pointer ids match every terminal path; a second pointer cannot terminate the first; cancel over empty space, raw pointer removal, drag-off release, disable-while-pressed, widget removal/despawn, same-id kind change, component-only owner-role removal, full owner-panel despawn, and explicit cancel each emit exactly one `ButtonCanceled`; built-in Escape reaches the same explicit-cancel path, preserves the captured pointer id, emits exactly once, and does nothing without active capture; both owner teardown paths emit that terminal event while `WidgetOf` and world/screen attachment state remain queryable, before anchor cleanup or linked widget despawn, with no duplicate on the later remove observer; same-id/same-kind tree refresh and interactivity writes that remain enabled preserve capture; Phase 5 focused semantic activation emits `ButtonClicked` alone; same-panel and other-panel button clicks classify IME blur correctly while a button over a field blocks field activation. Extend and smoke-test the public path in `examples/widgets.rs` while preserving the built-in and app-owned input paths.

#### Retrospective

**What worked:**
- `ButtonCaptures` owns immediate capture authority while `ButtonPress` remove/despawn hooks emit one terminal entity event before teardown removes widget relationships.
- The canonical example's persistent pointer/focus/button readout and tests through Bevy's real `pointer_events` made pointer and semantic behavior independently observable.

**What deviated from the plan:**
- Bevy's targeted `Release` and `DragEnd` paths do not cover every valid release, so Phase 6 added a post-event reconciliation pass over raw action order, `PointerState`, and `HoverMap`.
- The capture implementation remained button-specific; Phase 6 did not create the generic registry that earlier Phase 8 wording assumed.

**Surprises:**
- A newly hovered press/release in one frame and a stationary pointer whose panel moves away can receive neither targeted `Release` nor `DragEnd`.
- Bevy's `Press.count` resets after the multi-click interval and saturates at 255, so it cannot identify a press; Hana uses a checked private sequence instead.
- Partial picking-plugin composition and disabled hover processing require explicit resource gates and raw-release capture-loss handling.

**Implications for remaining phases:**
- Phase 8 must reuse the proven ordering and terminal-hook pattern or extract shared private capture machinery when slider behavior is implemented.
- Later pointer tests should drive Bevy's dispatcher and preserve Bevy message-maintenance timing rather than manually triggering only the expected target events.

#### Phase 6 Review

- Phase 6.5 now completes per-face hit filtering before the remaining pointer consumers build on the backend, with button regressions protecting the real pointer lifecycle.
- Phase 7 callbacks remain consumers of `ButtonClicked`; callback code does not inspect or mutate private capture state.
- Phase 7.5 reads the private `ButtonPress` presentation marker rather than the `ButtonCaptures` lifecycle authority.
- Slider state/request behavior and slider pointer lifecycle are split into Phases 8 and 8.5. Phase 8.5 must either extract shared private capture ownership or otherwise enforce one-pointer/one-widget ownership across buttons and sliders, and its tests use Bevy's real dispatcher and raw-message timing.
- The canonical example keeps the existing pointer, focus, and button diagnostics plus both button input paths; later phases expand its measured readout instead of replacing or clipping it.
- Phase 10 still has the same two owner decisions and remains intentionally non-delegate-ready until they are settled.

### Phase 6.5 — Per-face panel picking and stock-picker exclusion  · status: done (`7afab59d`)

#### Work Order

**Goal:** Finish the panel-picking work reconciled from `hanadocs/issues/panel picking design proposal.md` (decisions recorded there 2026-07-22): panel-internal render meshes stop competing with an application's generic mesh picking, and per-panel, per-face pickability becomes a builder choice.

**Spec:**
- **Stock-picker exclusion:** every panel-spawned render mesh — fill, image, text, and SDF batch entities — carries `Pickable::IGNORE` at spawn, matching the interaction mesh. In an app running `MeshPickingPlugin` (the hana editor), panel content no longer receives or blocks generic mesh picks. The diegetic backend is unaffected: it raycasts the interaction mesh directly and ignores that mesh's `Pickable`.
- **`FacePicking` / `PanelPicking`:** public enum `FacePicking { Interactive, PanelOnly, WidgetsOnly, PassThrough }` and component `PanelPicking { front: FacePicking, back: FacePicking }` with `Default` = both `Interactive` and consts `INTERACTIVE` / `PASS_THROUGH` for the symmetric cases. Absent component behaves as default. `Interactive`: panel and widgets respond. `PanelOnly`: panel is a grab/move target; widgets on that face don't respond (intended for back faces — a visible front widget should be usable; per-widget availability stays `WidgetInteractivity`'s job). `WidgetsOnly`: widget rects respond; panel background passes through to lower hits. `PassThrough`: face invisible to picking and never blocks.
- **Builder:** `.picking(PanelPicking)` on both the world and screen `DiegeticPanelBuilder` paths; the built panel entity carries the component. `PanelPicking::PASS_THROUGH` is the decoration/pass-through opt-out (hana usage path A).
- **Backend face resolution:** the diegetic backend classifies each interaction-mesh hit as front or back from the ray direction against the panel's plane normal, then composes the `PointerHits` group per that face's variant: which of panel entity and widget entities appear, and whether the ray early-exits (blocks lower). A face that emits no hits does not block. `PanelPicking` replaces the current panel-entity `Pickable` read (`is_hoverable` / `should_block_lower`) as the backend's per-panel control; per-widget `Pickable` filtering in widget matching is unchanged.
- **Two-sided invariant, restated:** identity resolution stays two-sided — front and back rays resolve the same panel and widget identities — and per-face filtering applies only to which of those identities are emitted. Panels with symmetric config keep the Phase 3 invariant test verbatim.
- **Cutout limitation, documented:** hit areas are rectangles. Transparent regions inside a panel catch clicks; content overhanging the panel edge does not hit-test. Rustdoc on `PanelPicking`/`FacePicking` records this and names `WidgetsOnly` as the transparent-panel path. Non-rect hit regions are out of scope.

**Files:**
- `src/widgets/picking.rs` — `FacePicking`, `PanelPicking`, face classification, per-face group composition
- `src/panel/builder.rs` — `.picking()` on world and screen builders
- `src/render/fill_batch.rs`, `src/render/image_batch.rs`, `src/render/panel_text/batching.rs`, `src/render/panel_shapes/batching.rs` — `Pickable::IGNORE` at batch spawn sites
- `src/lib.rs` — curated `PanelPicking` / `FacePicking` exports
- `crates/hana_diegetic/examples/widgets.rs` — exercise at least one non-default config

**Constraints from prior phases:** Depends on Phase 3's backend and Phase 1's widget records. It runs now because it changes the hit stream consumed by later button presentation, slider capture, and tooltip eligibility. Phase 3's acceptance test "non-pickable panels do not hit" migrates from panel-entity `Pickable::IGNORE` to `PanelPicking::PASS_THROUGH`. Phase 6's button capture and reconciliation remain authoritative consumers of the resulting Bevy pointer lifecycle; this phase must not add a second behavior path.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: batch-spawned fill/image/text/SDF entities carry `Pickable::IGNORE`; absent `PanelPicking` behaves as both-faces `Interactive`; per variant and face — `PanelOnly` back face reports the panel but no widgets, `WidgetsOnly` reports widgets while the panel background passes through to a mesh behind, `PassThrough` faces produce no hits and never block; symmetric configs preserve the Phase 3 two-sided identity test; the migrated pass-through test covers the old opt-out path; real pointer dispatch proves `Interactive` and `WidgetsOnly` still produce the Phase 6 button lifecycle while `PanelOnly` and `PassThrough` do not target the button. Extend and smoke-test the public path in `examples/widgets.rs` without replacing or clipping the existing diagnostic rows. On completion, run `close_issue` on `hanadocs/issues/panel picking design proposal.md`.

#### Retrospective

**What worked:**
- `FacePicking` and `PanelPicking` now filter one two-sided interaction mesh while resolved-hover stacking and real `PointerInput` tests protect button delivery.
- Fill, image, text, and shape batch entities carry `Pickable::IGNORE`, so Bevy's stock picker no longer sees retained panel rendering.
- The builder-provided initial `PanelPicking` uses the existing component-ownership ledger: role removal deletes Hana's untouched value, while explicit spawn-bundle values and later application writes survive.

**What deviated from the plan:**
- Fairy Dust screen overlays and six decorative `hana_conduit` labels also needed `PanelPicking::PASS_THROUGH`; their existing root `Pickable::IGNORE` no longer controlled the custom backend.
- The canonical non-default example moved from a Fairy Dust-overridden screen panel to the world widget panel: front widgets stay interactive and the back remains a panel-only grab surface.
- Post-review lifecycle work added component-only teardown and re-add coverage for the builder-provided initial picking value.

**Surprises:**
- A required default `PanelPicking` could not distinguish an explicit `INTERACTIVE` component from Bevy's automatically inserted default. The panel observer now installs the builder-provided initial value only when the sibling component is absent and rechecks before the deferred insert.
- Decorative callers must opt out of the diegetic backend through `PanelPicking`; `Pickable::IGNORE` remains the separate stock-mesh-picker control.
- An application observer can deliberately replace the picking component later in the same command flush and share Hana's change tick. That competing observer order is application responsibility; the library does not add discard hooks or extra ownership state for it.

**Implications for remaining phases:**
- Every later pointer consumer receives the final per-face hit stream; callbacks, visual state, sliders, and tooltips must not recreate face filtering.
- Phase 7 remains a consumer of `ButtonClicked` and does not need access to private capture or picking state.
- The canonical example must preserve the world panel's front-interactive/back-panel-only policy and Fairy Dust's pass-through screen overlays.
- Later panel resets and tooltip materialization use the shipped `PanelPicking` ownership/teardown path rather than maintaining a second cleanup mechanism.

#### Phase 6.5 Review

- Remaining verification commands are package-scoped to `hana_diegetic`; another package is added only when a phase changes it.
- Phase 7 now requires exactly one plugin-installed global callback observer, while Phase 7.25 separately owns retained visual slots, batch re-keying, empty-batch retirement, and `Pickable::IGNORE` on replacement batches.
- The Phase 7.5 public-preset question is resolved: widgets v1 uses direct state-presentation builders, while presets and themes move to [`widgets-deferred.md`](widgets-deferred.md).
- Phase 8.25 now isolates shared private pointer-capture occupancy before Phase 8.5 adds slider dragging, and flat ray projection extends the existing renderer boundary.
- Phase 10.5 now carries an explicit owner decision for materialized-tooltip picking; Phase 11 reuses Phase 6.5 ownership during ordered panel reset instead of maintaining a second picking cleanup path.
- Phase 12 preserves the canonical world panel's front `Interactive`/back `PanelOnly` policy and Fairy Dust's `PASS_THROUGH` overlays.
- Same-command-flush application observers that deliberately compete with Hana's absent-only initial `PanelPicking` install remain application responsibility; the plan rejects discard hooks or another ownership mechanism for that edge case.

### Phase 7 — `.on_click` sugar  · status: done (`3309610b`)

#### Work Order

**Goal:** Retained button authoring can install ergonomic typed click callbacks without giving `LayoutBuilder` access to `World`.

**Spec:**
- **Event consumption, base path:** app code observes `ButtonClicked` globally or through an entity-scoped observer and reads the widget entity directly from the event target; the payload id is a convenience for logging or panel-local application logic. An event handler never re-resolves its target through `PanelWidgetReader`. The reader is only a pre-event bridge when app code starts from authored `(panel, id)` and needs the entity to install a scoped observer/effect or issue other entity-targeted control. This base path ships alongside the sugar, not instead of it; id alone is never globally unique.
- **`.on_click` sugar:** preserve `.on_click(closure)` without requiring `LayoutBuilder` to access a `World`. `Button` stores a private, cloneable callback template: an `Arc`-owned wrapper around a typed `SystemHandleTemplate<In<ButtonClicked>, ()>`, compared by `Arc` identity so `WidgetSpec` stays comparable. World-aware reify builds one tracked `SystemHandle` and stores it on the widget. `WidgetsPlugin` installs exactly one global `ButtonClicked` observer; it reads the event target's tracked handle and calls `run_system_with` using the clicked event. Reify never installs a per-widget observer. Reuse never registers the system again; callback replacement drops the old strong handle, and final-handle drop lets Bevy clean up the registered system. Cost: one allocation per authored callback and reference-count operations when a tree clones. Each dispatch clones the owned `ButtonClicked` payload, including its string-backed panel-local id, but performs no callback registration or callback-system allocation.
- **Behavior boundary:** the uniform callback observer forwards the completed `ButtonClicked` event only. It never reads or mutates the private `ButtonPress` marker or `ButtonCaptures` resource; pointer lifecycle and semantic activation are already resolved before callback dispatch.

**Files:**
- `src/widgets/button.rs`, `src/layout/builder.rs` — typed callback template and `.on_click`
- `src/widgets/reify.rs` — tracked callback handle lifecycle
- `src/widgets/mod.rs` — the one plugin-installed global `ButtonClicked` observer
- `src/lib.rs` — `.on_click`-related curated exports if required
- `crates/hana_diegetic/examples/widgets.rs` — canonical callback exercise

**Constraints from prior phases:** Phase 6 defined typed click input and lifecycle; Phase 1 requires `WidgetSpec: Clone + PartialEq`. Callback replacement is a same-id/same-kind authored refresh and must not disturb a live press except when the `Button` kind is removed. Phase 6.5 preserves real button pointer delivery through the per-face backend; this phase consumes the resulting click event without depending on hit-filter or capture internals.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: `.on_click` receives both a pointer-originated `ButtonClicked` carrying `Some(pointer_id)` and a semantic `ButtonClicked` carrying `None`; exactly one plugin observer dispatches callbacks and no widget owns an observer; reify reuse does not re-register; callback replacement during a live press releases only the prior tracked handle, installs exactly one replacement, and leaves the press intact; widget removal releases the final tracked callback; global observation reads the event target directly, and an entity-scoped observer installed on a previously reader-resolved widget receives the click without re-resolving it. Extend and smoke-test the public path in `examples/widgets.rs` while preserving and expanding the existing diagnostic readout.

#### Retrospective

**What worked:**
- `Button::on_click` stores an `Arc`-backed `SystemHandleTemplate`; reify owns one tracked `SystemHandle` on the widget, and the plugin's single global observer dispatches completed `ButtonClicked` events.
- Same-tree reuse, live-press callback replacement, final-handle cleanup, direct global observation, and reader-resolved entity-scoped observation are covered independently.
- The canonical example exposes callback count as a fourth persistent readout row. Keyboard-driven BRP smoke produced the normal click and callback logs, and the measured 103.44 px content fits below its 116 px cap.

**What deviated from the plan:**
- Dispatch must clone the owned `ButtonClicked` from Bevy's borrowed observer event before passing it to `run_system_with`; a named `PanelElementId` therefore clones its string. Dispatch still performs no callback registration or callback-system allocation.
- Strict clippy required extracting `resolve_face_hit` from the already-shipped picking backend. That edit is behavior-preserving and does not move face filtering into callback code.
- Verification now runs `cargo nextest run -p hana_diegetic --lib`; an edited example is compiled separately with its exact `cargo check -p hana_diegetic --example <name>` command so Cargo does not implicitly link every example.

**Surprises:**
- The added readout row made the old 88 px cap exactly full. Raising the cap to 116 px and world height to 0.12 left measured pixel content below the cap instead of clipping it.
- `cargo nextest run -p hana_diegetic` selects the package's examples as test targets even though the command appears package-scoped; package scope alone is not target scope.

**Implications for remaining phases:**
- Later presentation, slider, and tooltip work consumes `ButtonClicked` and the retained widget entity; it must not add per-widget callback observers or inspect callback system handles.
- Later example changes expand the measured readout and verify only the named example in addition to library tests.

#### Phase 7 Review

- No remaining phase is redundant, reordered, or blocked by the callback implementation, and no new owner decision is required before Phase 7.25.
- Phase 7.25 and Phase 8 preserve an unchanged button's authored `WidgetSpec` and tracked callback handle while editing shared reify state.
- Phase 7.5 remains callback-transparent, Phase 8.25 retains Phase 7's callback regressions, and Phase 12 preserves the visible callback count.
- Every remaining executable Work Order uses library-only tests plus the exact changed-example check from Delegation Context; none selects every example or uses `--all-targets`.

### Phase 7.25 — Retained widget visual slots and batch re-keying  · status: done (b86e291d)

#### Work Order

**Goal:** Widget state can update retained fill, image, text, and shape records without relayout, including moving a record when its batch key changes.

**Spec:**
- **Stable visual slots** (`widgets/visual.rs` and layout output): ordinary fills, borders, images, text, and shape parts remain retained render records rather than ECS children. Layout authoring may attach stable private slot ids to ordinary `El`/`PanelDraw` primitives; `ComputedWidgetRecord` carries slot-to-record references. Widget entities own changed-only override components. A state-only override never rewrites `DiegeticPanel`, regenerates the `LayoutTree`, changes `ComputedDiegeticPanel`, or runs geometry solving.
- **Four renderer entry points:** route overrides through the existing record writers in `render/fill_batch.rs`, `render/image_batch.rs`, `render/panel_text/batching.rs`, and `render/panel_shapes/batching.rs`, with common retained-batch ownership/retirement in `render/batch_store.rs`. Same-key color or panel-local XY translation changes patch only the referenced record and dirty GPU row; unrelated records and batches remain untouched.
- **Re-keying is required:** material compatibility and image texture are batch-key facts. When an override changes either fact, remove the old record, insert it into the compatible destination batch, create that batch through the existing renderer spawn path when absent, and retire an empty old batch. A moved slot keeps its stable identity and future overrides resolve its new record location.
- **Picker exclusion survives batch churn:** every batch entity created because of a re-key uses the same spawn path as initial batching and therefore carries `Pickable::IGNORE`; override processing never creates a second picking surface.
- **Interaction boundary:** visual-slot overrides may change appearance or translate presentation records in the owning widget's panel-local XY plane. They preserve every record's authored `DrawCommandDepth` and never alter the widget `Transform`, `rect`, `clipped_rect`, or interaction rank from Phase 3, so they cannot lift one widget over another or expand its hit bounds. Applications author fixed part layering through ordinary tree `z_index`; cross-widget depth and hit-bound changes require authoritative tree/layout authoring and are outside runtime state presentation.

**Files:**
- `src/widgets/visual.rs`, `src/widgets/mod.rs` — private slot references, override components, and changed-only dispatch
- `src/layout/engine/` — carry stable slot references into `ComputedWidgetRecord`
- `src/widgets/reify.rs` — attach stable visual-slot references without rewriting unrelated record fields
- `src/render/fill_batch.rs`, `src/render/image_batch.rs` — in-place update versus batch re-key entry points
- `src/render/panel_text/batching.rs`, `src/render/panel_shapes/batching.rs` — text/shape record update and re-key entry points
- `src/render/batch_store.rs` — record relocation, destination creation, and empty-batch retirement

**Constraints from prior phases:** Phase 7's reify path owns each unchanged button's authored `WidgetSpec` and tracked callback handle; slot attachment must preserve both, and only an actual callback or widget-kind change may replace or remove the handle. Phase 6.5 made every initial fill/image/text/shape batch entity `Pickable::IGNORE`; destination batches created during re-keying preserve that invariant. Phase 3 supplies deterministic widget interaction rank and rectangles, which visual overrides never mutate. Phase 2 guarantees interactivity-only edits skip geometry solving. This phase builds only private retained-render infrastructure; it does not define public preset APIs or behavior-state mappings.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: same-key overrides dirty only the referenced row; material/texture incompatibility relocates the record to the correct existing or newly created batch; the stable slot resolves the destination afterward; the old record and empty batch are retired; every newly created batch carries `Pickable::IGNORE`; repeated identical overrides are no-ops; unrelated slots remain untouched; override-only changes do not fire `Changed<DiegeticPanel>` or `Changed<ComputedDiegeticPanel>`, solve geometry, or alter authored draw depth, widget transforms, hit rectangles, interaction rank, or cross-widget picking order; slot updates retain an unchanged button's callback system entity. Exercise the private slot path deterministically in library tests. Smoke the unchanged canonical example as a regression check; Phase 7.5 adds its first visible slot exercise through direct `Button` state builders.

#### Retrospective

**What worked:**
- Stable slots flow from layout elements through computed widget records and reify into one widget-owned override index consumed by all four retained renderers.
- Existing `BatchStore` relocation/retirement and renderer spawn paths handled material/texture re-keying without parallel batch machinery.

**What deviated from the plan:**
- Runtime overrides preserve authored depth and support appearance plus panel-local XY translation only; a per-record z override would disturb panel-wide draw-order facts.
- `layout/element.rs` already owned computed widget-record construction and `render/batch_store.rs` already supplied the required movement primitives, so neither `layout/engine/` nor the store needed changes.
- The private slot path is proven in library tests; the unchanged example remained a callback regression smoke because no production state-presentation builder authors slots yet.

**Surprises:**
- Structural tree edits can reuse an old `(panel, element index)` for another widget, so dispatch must remove every stale widget key before inserting any current key.
- Image-buffer modification events written late in `PostUpdate` may be emitted on the next update; the no-op test drains both retained event buffers after a deterministic flush frame.

**Implications for remaining phases:**
- Production presentation writers must compare the desired override immutably before taking mutable component access so unchanged frames do not trigger dispatch.
- Applications author fixed part layering in ordinary layout trees; button and slider hover, press, focus, disabled, and value state may change only retained appearance or panel-local XY translation.
- Direct button and slider builders reuse the same private slot/index path and existing material/texture re-keying. Tooltip presentation remains the application-authored `TooltipTemplate` tree.

#### Phase 7.25 Review

- Updated Phase 7.5 to expose the private authoring/mutation hooks only inside the crate, carry direct state values inside the existing authored `Button`, and order presentation writes before override dispatch with an explicit deferred-command fence.
- Updated Phases 8 through 8.5 so later reify and capture work preserves stable slots, current overrides, authored presentation state, callback identity, and active press/drag state.
- Updated Phase 9 to reuse the same presentation order and record the unresolved direct slider-anatomy/fill geometry choice rather than assuming a translation-only override can resize a fill record.
- Updated tooltip phases so hidden tooltips retain their controller, panel, layout, and placement state while renderer rows may retire and rebuild; applications supply the ordinary `TooltipTemplate` tree.
- Resolved the Phase 7.5 authoring choice with direct `Button` builders. Phase 9 uses a direct `El::slider_thumb()` marker and moving thumb; preset/theme and variable-fill design are deferred until several widgets have been used together.

### Phase 7.5 — Button state presentation builders  · status: done (`326661b8`)

#### Work Order

**Goal:** Direct `Button` builders map hover, press, visible keyboard focus, and disabled state onto the button root's retained background, border, and material records without a preset/theme abstraction or relayout.

**Spec:**
- **Direct public authoring:** the normal surface remains the existing `El::background`, `El::border`, and `El::material` declaration. Extend `Button` itself with optional private-field builders `hovered_background`, `pressed_background`, `focused_background`, and `disabled_background`; the same four state prefixes apply to `*_border_color` and `*_material`. `focused_*` means the keyboard focus indicator is visible, not merely that the button remains the semantic focus target after a pointer press. No public `ButtonPreset`, `ButtonStyle`, appearance component, state enum, theme token, or custom/extended-material surface is introduced. `Handle<StandardMaterial>` remains the material boundary.
- **Root-surface boundary:** these builders affect only the element carrying `El::button`; arbitrary child text, images, icons, shapes, content, and layout stay as authored. A state background requires an authored normal background record, a state border color requires an authored normal border whose widths/radii remain fixed, and a state material requires an authored root surface. Validate missing targets during panel construction and `set_tree` with stable `PanelBuildError` variants rather than silently ignoring the builder. State changes never add/remove render roles or change layout geometry.
- **State composition:** start from the authored normal values, then layer focused → hovered → pressed → disabled overrides per property; a missing value leaves the previous layer intact. This lets visible keyboard focus change only the border while hover changes only the background. Presentation reads `PickingInteraction`, `Has<WidgetDisabled>`, the private `WidgetFocusVisible`, and the private `ButtonPress` marker. Pointer focus therefore remains the keyboard target without keeping the focus styling visible. Presentation never reads or mutates `ButtonCaptures`, which remains lifecycle and ordering authority.
- **Existing transport:** the authored `Button` already travels inside `WidgetSpec` through `ComputedWidgetRecord` and is reified on the widget entity. Store the state values there; do not add a second presentation snapshot or public component. Same-id updates may replace those authored values without replacing behavior state, callback identity, stable slots, or current overrides.
- **Role-specific retained color:** extend the private Phase 7.25 override value so the root SDF fill and border can receive independent colors while retaining the same element/slot identity. Material replacement may apply to both authored root SDF roles. This role distinction remains private and must not alter image, text, or shape records sharing another slot.
- **Production override authoring:** expose Phase 7.25's test-gated slot constructor, `El::visual_slot`, override builders, and set/clear operations only to crate production code; none becomes public. Add one shared private writer that first reads the current `WidgetVisualOverrides` immutably and returns when the desired slot value is equal, then takes mutable access only for a real change. Equality inside `WidgetVisualOverrides::set` is not enough after a Bevy `Mut` borrow has already marked the component changed.
- **Focus command fence:** add `WidgetSystems::FocusCommandsApplied`, an explicit `ApplyDeferred` after Phase 5's `WidgetSystems::SemanticInput`. Button state presentation runs after this fence, so pointer-driven indicator removal and application/traversal indicator insertion are visible in the same frame. Phase 10.5 reuses this scheduling point for tooltip focus eligibility.
- **Presentation ordering:** run the button state writer after `WidgetSystems::FocusCommandsApplied`. Follow it with `WidgetSystems::PresentationCommandsApplied`, an explicit `ApplyDeferred` needed when the writer first inserts its private override component. Run `dispatch_visual_overrides` after that fence, before the retained fill/image/text/shape routes in `PostUpdate`, so a state edge reaches retained data in the same frame. Phase 9 reuses this order.
- **Tree-backed disabled writes versus state presentation:** a panel/global cascade edge reaches the button state writer without mutating `DiegeticPanel` or `ComputedDiegeticPanel`. A widget-local `PanelWidgetWriter` edit performs the one authoritative tree mutation and visual-only computed refresh allowed by Phase 2; after `WidgetDisabled` changes, presentation writes retained overrides without another panel/computed refresh or geometry solve.
- **Deferred convenience layer:** preset, theme, variant, token, state-dependent child-content, and public custom-material design belongs to [`widgets-deferred.md`](widgets-deferred.md) after multiple widgets have exercised these direct APIs. A later helper may call these same builders but may not replace them.

**Files:**
- `src/widgets/button.rs` — direct state-value builders and state presentation writer
- `src/layout/builder.rs`, `src/layout/element.rs`, `src/widgets/id.rs` — private root slot authoring and missing-target validation
- `src/widgets/mod.rs` — focus/presentation fences and state-writer registration
- `src/widgets/visual.rs`, `src/render/fill_batch.rs` — changed-only role-specific retained overrides
- `src/lib.rs` — keep the curated `Button` export; add no preset/style exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical direct button-state exercise
- `docs/hana_diegetic/widgets-deferred.md` — deferred widgets-v2 preset/theme questions and v1 constraints

**Constraints from prior phases:** Phase 7.25 supplies stable slot identity, reified references, order-independent stale-key removal before current-key insertion, four retained-render routes, material/texture re-keying, and stock-picker exclusion for newly created batches. Its authoring/mutation hooks remain crate-private and test-gated until this phase installs the shared production writer above. Runtime overrides preserve authored depth and may change only appearance or panel-local XY translation; fixed part layering stays in the application's ordinary tree. Phase 7 reifies the authored `WidgetSpec` and preserves callback identity independently. Phase 6 supplies the private `ButtonPress` presentation marker and keeps `ButtonCaptures` private lifecycle authority. Phase 6.5 supplies the final per-face hit stream. Phase 3 supplies `PickingInteraction`; Phase 5 supplies semantic `WidgetFocused`, private `WidgetFocusVisible`, and `WidgetSystems::SemanticInput`. Phase 5.5's built-in adapter and app-owned example path both send the same Phase 5 messages before semantic input, so one `WidgetSystems::FocusCommandsApplied` fence covers either origin; do not add an adapter-specific fence. Presentation runs focus fence → button state writer → presentation command fence → override dispatch → retained renderer routes; do not rely on unspecified system insertion order.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: direct hover/press/visible-focus and panel/global disabled builders patch only the button root's requested background, border, or material property; per-property precedence is normal → focused → hovered → pressed → disabled; missing values fall through; missing authored fill/border/surface targets return stable build and `set_tree` errors; two buttons with different direct builder values retain the correct values through computed output and same-id reify; pressed presentation reads `ButtonPress` and never reads or mutates `ButtonCaptures`; pointer focus retains `WidgetFocused` without drawing the focused state, while current-frame application/traversal focus draws it after `WidgetSystems::FocusCommandsApplied`; first insertion is visible to dispatch after `WidgetSystems::PresentationCommandsApplied`; a widget-local disabled edit performs exactly its Phase 2 visual-only computed refresh and no layout solve, then presentation causes no second panel/computed change; repeated state leaves the `WidgetVisualOverrides` change tick unchanged and causes no retained upload; unrelated root roles, child content, and other slots remain untouched; `.on_click` fires once and retains the same tracked system through presentation-state changes. Extend and smoke-test the public path in `examples/widgets.rs` with visibly distinct normal, hovered, pressed, keyboard-focused, and disabled surfaces while preserving and expanding the existing diagnostic readout.

### Retrospective

**What worked:**
- Twelve direct `Button` builders compose root fill, border, and material overrides without relayout or a preset/theme API.
- Pointer, application, and traversal focus tests now drive the real Bevy observers through the two explicit command fences.

**What deviated from the plan:**
- `ButtonStatePresentation` moved behind `Option<Box<_>>` so buttons without state presentation do not carry the larger color/material block.
- The first implementation scanned every button each frame; review added `presentation_inputs_changed` so only authored or live-state edges run the writer.
- A post-checkpoint usability pass separated retained semantic focus from its visible indicator: pointer presses now hide focused presentation while traversal and application focus show it.

**Surprises:**
- `PointerHits` plus `PointerInput` can exercise hover, press, and pointer focus deterministically without moving the operating-system pointer.
- An application-focus regression must queue its trigger inside `Update`; manually flushing after `World::trigger` bypasses the focus-fence behavior the test is meant to prove.

**Implications for remaining phases:**
- Phase 9 should reuse the same change gate and visible-focus → presentation → dispatch fence chain for slider state presentation.
- Phase 10.5 can reuse `WidgetSystems::FocusCommandsApplied` for tooltip focus eligibility without another adapter-specific fence.
- Preset, theme, variant, and state-dependent child-content design remains deferred to `widgets-deferred.md` until several widgets exercise the direct APIs.

### Phase 7.5 Review

- Phase 8 now tests direction storage only; pointer-direction and thumb-direction behavior remain in Phases 8.5 and 9.
- Phase 8.5 now carries deterministic Bevy pointer-dispatch coverage plus deferred decisions for exact axis/thumb formulas and splitting projection from slider lifecycle.
- Phase 9 now names distinct slider-root/thumb slots, exact public validation errors and messages, `src/panel/builder.rs`, and a quiet-frame presentation gate.
- Phase 10 now names `src/panel/builder.rs`; its existing placement and invalid-associated-tooltip decisions remain deferred to its pre-dispatch check.
- Phase 10.5 now fixes the post-transform reveal schedule and deterministic pointer path, while its existing picking-policy decision and a new materialization/behavior split remain deferred to its pre-dispatch check.

### Phase 8 — Slider state and request behavior  · status: todo

#### Work Order

**Goal:** Establish slider applied state, validation, authored/runtime ownership, and app-controlled value requests before adding pointer capture.

**Spec:**
- `widgets/slider.rs`: extend the Phase 1 authoring/configuration module with runtime slider state and behavior. `WidgetSpec::Slider`, `El::slider`, `Slider`, `SliderRange`, `SliderStep`, `SliderDirection`, and `SliderConfigError` already exist and must not be redefined.
- **Shipped construction contract:** Phase 1 provides `SliderDirection::{LeftToRight, RightToLeft, BottomToTop, TopToBottom}`; finite strictly ordered `SliderRange`; finite positive `SliderStep`; validated `Slider::new(range, initial_value)` plus `step` and `direction` builders; and the `thiserror`-derived `SliderConfigError::{NonFiniteRange, UnorderedRange, NonFiniteValue, NonPositiveStep}` with stable-message tests.
- **Approved runtime value contract (PD1):** private-field `SliderState` is the public component containing range, applied raw-domain value, optional step, and direction. `SliderState::new(range, value, step, direction) -> Result<Self, SliderConfigError>` and `set_value(value) -> Result<bool, SliderConfigError>` reject non-finite input, snap to the lattice anchored at `range.start()`, then clamp, with the Boolean reporting an applied-value change. It also exposes `range()`, `value()`, `step()`, and `direction()` readers.
- **App authority and request API:** `SliderChangeRequested` targets the widget and carries `{ id, value, is_final, pointer_id: Option<PointerId> }`; this phase's semantic/remote requests are final and have no pointer, while Phase 8.5 adds non-final pointer proposals plus a final pointer proposal on release. App code explicitly applies or rejects the proposal with `SliderState::set_value`. The exported `slider_self_update` observer is the opt-in uncontrolled convenience. `RequestSliderAdjustment { entity, adjustment }` computes and emits a proposal without applying it; an app controller that starts from authored `(panel, id)` resolves that entity through `PanelWidgetReader` before constructing the request, rather than using a second id-targeted request type. `SliderAdjustment` is exactly `Absolute(f32) | Relative(f32) | RelativeSteps(f32)`. Every adjustment validates its numeric input; `RelativeSteps` emits no proposal when the state has no step.
- **Authored/runtime ownership:** `Slider::initial_value` applies only on first spawn. Phase 1 accepts any finite authored initial value, including values outside the range or off the optional step lattice; first reification constructs `SliderState` through `SliderState::new`, applying the same snap-to-`range.start()` lattice then clamp order as later state changes. Same-id reuse preserves the live applied value; an authored range/step/direction change updates the configuration and revalidates the preserved value, while an unrelated reify does not rewrite `SliderState`. Later direct slider presentation reads only the applied value, never an unaccepted proposal.
- **Bevy reference check:** both the project-version `bevy_ui_widgets 0.19.0` source and local `../bevy` at `0.20.0-dev` use raw-domain state, external `ValueChange<f32>` proposals, optional self-update, and absolute/relative/relative-step remote control. Hana adopts those semantics. It intentionally does not copy Bevy's independently insertable tuple components, warn-only invalid ranges, separate `SliderPrecision`, `TrackClick`, auto-orientation, accessibility dependency, or UI-space drag delta; Hana keeps one validated state, one step lattice for pointer and semantic values, four explicit directions, and captured-camera panel-local reprojection.
- **Continuous app-owned input:** keep slider adjustment outside the six built-in focus messages. An application may observe `Fire` from Bevy Kana's held-action helpers and send `RequestSliderAdjustment` repeatedly while a key or gamepad input is held; this uses Phase 5.5's preserved continuous path and requires no new widget adapter action.

**Files:**
- `src/widgets/slider.rs` — extend the Phase 1 authoring/configuration module with runtime state and behavior
- `src/widgets/reify.rs` — slider kind reify
- `src/widgets/mod.rs` — slider request handling and export wiring; the plugin does not install `slider_self_update`
- `src/lib.rs` — curated slider state, proposal, and request exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical slider state/request exercise

**Constraints from prior phases:** Phase 1 owns slider authoring, validated construction types, the `thiserror` error contract, computed snapshots, and Phase 1-owned kind replacement/removal. Extend reify here to preserve live `SliderState` on same-id reuse; pointer lifecycle remains Phase 8.5's responsibility. The shared reify edits preserve an unchanged button's authored `WidgetSpec`, Phase 7 tracked callback handle, and Phase 7.25 stable slots/current overrides; Phase 7.5's direct state values remain inside that authored `Button`. Only an actual callback, direct presentation value, or widget-kind change may update its corresponding state. The plugin registers request handling and exports `slider_self_update`, but never installs that opt-in observer for the application. Phase 5.5 preserves `Keybindings::spawn_key` and `Fire` as the app-owned continuous-input path; use that path in the canonical example to send repeated slider-adjustment requests rather than extending the built-in six-message adapter. PD1 is accepted and fixed above.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with Phase 1's construction/error tests retained and new tests for: `SliderState` retains each of the four authored directions; lattice anchoring plus snap/clamp order; first-spawn normalization of out-of-range and off-step authored initial values; app accept/reject and opt-in self-update; absolute/relative/relative-step requests, including a remote request sent to an entity resolved from `(panel, id)`; an app-owned held Bevy Kana action emits repeated `Fire` edges that each send one `RequestSliderAdjustment`; spawn-only initial value and authored range-change clamping; disabled sliders ignore semantic requests. Directional pointer projection and thumb movement remain owned by Phases 8.5 and 9. Extend and smoke-test the public path in `examples/widgets.rs` while preserving the built-in and app-owned input paths and expanding the measured diagnostic readout.

### Phase 8.25 — Shared private pointer-capture authority  · status: todo

#### Work Order

**Goal:** Extract the smallest private pointer/widget occupancy and raw-action ordering authority that buttons and sliders can share without changing the public button API or behavior.

**Spec:**
- Move only cross-widget facts out of Phase 6's button-specific `ButtonCaptures`: which `PointerId` currently owns which widget, which widget is occupied, and the checked raw-action sequence used to order release/cancel before a later press. Button-specific `ButtonPress`, terminal state, causes, and event emission remain in `button.rs`.
- One pointer cannot own two widgets and one widget cannot be owned by two pointers. Releasing or canceling frees both directions before a later raw action in the same unread batch can claim either side.
- Keep the shared authority crate-private. Do not expose a capture component, resource, trait, event, or generic public terminal payload. Phase 8.5 consumes the private operations from slider behavior.
- Adapt button press/release/cancel and both panel teardown paths to the extracted authority without changing when or which `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, or `ButtonCanceled` events fire.
- Preserve Phase 6's real-dispatcher and raw-reconciliation behavior, including partial picking-plugin composition, stale hover, pointer removal, checked sequence exhaustion, and remove/despawn terminal hooks.

**Files:**
- `src/widgets/button.rs` — retain button terminal state/events while delegating only occupancy/order facts
- `src/widgets/capture.rs` — new private shared pointer/widget occupancy and raw-order authority
- `src/widgets/mod.rs` — private module/resource initialization and system ordering
- `src/panel/lifecycle.rs`, `src/panel/mod.rs` — preserve button finalization through role removal and full despawn

**Constraints from prior phases:** Phases 6 and 7 are the behavioral baseline and their public APIs must remain unchanged. Preserve the exact insertion/removal timing of the private `ButtonPress` marker because Phase 7.5 reads it for pressed presentation; moving occupancy authority must not introduce an earlier visual release or a lingering pressed frame. Phase 6.5 supplies the final hit stream but does not participate in capture. Phase 5 supplies the paired component-removal/full-despawn finalization timing. This phase adds no slider lifecycle yet and no public type.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with every Phase 6 and Phase 7 button regression retained plus focused cross-authority tests: one pointer/one widget occupancy, two pointers competing for one button, release or cancel freeing both indexes before a same-batch new press, stale entities retiring without leaks, and checked sequence exhaustion preserving the prior owner. Pointer and semantic clicks each dispatch `.on_click` exactly once after extraction. The public button event sequence and payloads and private `ButtonPress` timing remain unchanged, and the canonical widget example's pointer/focus/button/callback readout behaves unchanged in a live smoke.

### Phase 8.5 — Slider pointer lifecycle and projection  · status: todo

#### Work Order

**Goal:** Add slider grab, drag projection, release, and exact-once cancellation on top of Phase 8's applied-state and proposal API.

**Approved slider axis and thumb-travel contract:**
- The active rectangle for pointer projection and thumb presentation is the slider root's content box, excluding its border and padding.
- When the slider has a marked thumb, let `C` be the content-box extent and `T` the thumb extent on the active axis. The usable visual travel is `max(C - T, 0)`. The thumb's authored position is its range-start baseline; the presentation writer translates it by the normalized applied value times that usable travel toward the range end, changes no cross-axis position, and preserves authored depth.
- Pointer projection follows the thumb center's actual path, not the content box's full extent. Its directed start/end coordinates are: left plus `T / 2` → right minus `T / 2` for `LeftToRight`; the reverse for `RightToLeft`; top plus `T / 2` → bottom minus `T / 2` for `TopToBottom`; and the reverse for `BottomToTop`. Clamp outside that directed interval, then normalize to `[0, 1]` and map through the raw slider range.
- If `T >= C`, there is no visible thumb travel, so pointer projection returns `ProjectionUnavailable` and emits no proposal. A headless slider instead projects over the full directed content-box extent; zero active-axis content extent returns `ProjectionUnavailable`.
- Phase 8.5 pointer tests and Phase 9 presentation tests share this endpoint table so clicking the thumb center at either visual endpoint yields the matching range endpoint without a jump.

**Pending decision: split projection from slider capture lifecycle**

Actual problem:
This Work Order currently combines a reusable captured-camera ray-to-panel projection boundary with button/slider occupancy, raw action ordering, drag proposals, terminal events, teardown, and the cumulative example. That is too much coupled behavior for one fresh delegated session and makes a projection defect expensive to isolate.

What exists now:
- `render::project_flat_panel_hit` is already the single flat-panel conversion boundary and can be extended and tested without slider terminal behavior.
- Phase 8.25 supplies the shared private occupancy/order authority; the remaining slider lifecycle can consume a completed projection API.

What should change:
- Split the reusable captured-camera projection work into Phase 8.5, then move slider capture, drag proposals, terminal events, and teardown into a new Phase 8.75.
- Keep Phase 9 after both subphases and update each Work Order's Files and acceptance tests to match its narrower responsibility.

Recommendation:
Use Phase 8.5 for captured-camera/render-target validation, ray intersection, panel-local coordinates, directional normalization, and deterministic projection tests. Add Phase 8.75 for slider grab/drag/release/cancel behavior using that boundary and Phase 8.25's shared authority.

**Spec:**
- **Drag mapping:** map panel-local position through the approved content-box/thumb-center endpoint table above to a normalized value, then through the chosen range. Each `Pointer<Drag>` reprojects `pointer_location.position` via the **captured press camera and render target** → `Camera::viewport_to_world` → ray → flat panel intersection → panel-local map → clamp. Extend the existing `render::project_flat_panel_hit` boundary in `render/panel_geometry.rs` with the ray-intersection input slider dragging needs; do not create a second `panel_local_from_ray` authority in `widgets/picking.rs`. `Drag.delta` is invalid for perspective world panels. Cancel if the captured camera/target disappears or no longer matches. Surface-panel integration later replaces only this shared flat projection boundary.
- **Lifecycle:** reuse Phase 6's proven ordering and lifecycle pattern: a private checked press sequence, terminal state set before marker removal, remove/despawn hooks, raw pointer reconciliation, and finalize-before-despawn handling. `SliderGrabbed`/`SliderReleased` carry `{ entity, id, pointer_id }`; `SliderCanceled` adds `cause`, whose variants are `PointerCanceled | PointerRemoved | CaptureLost | Disabled | ProjectionUnavailable | WidgetRemoved | WidgetKindChanged | Explicit`. Pointer drags emit Phase 8's non-final `SliderChangeRequested` proposals plus one final proposal on valid release. `WidgetDisabled` cancels an active drag and blocks pointer changes. A same-panel/same-id/same-kind tree refresh preserves capture and the live applied value; only removal, kind change, disable, projection/capture loss, explicit cancel, or owner-panel teardown terminates it.
- **Cross-widget ownership:** consume Phase 8.25's private shared authority. One `PointerId` cannot simultaneously capture a button and a slider, and one widget cannot be captured by two pointers. Button and slider terminal payloads remain typed in their own modules; expose no public capture API. Release/cancel finalization occurs before a later raw press can claim either freed pointer or freed widget.
- **Semantic cancel:** `SemanticWidgetIntent::Cancel` terminates an active slider drag on its resolved target exactly once with the stored `PointerId` and `Explicit` cause. It is a no-op when that slider has no active capture; the built-in Escape binding and an application-written `CancelFocusedWidget` message use this same path.
- **Panel teardown order:** extend both lifecycle paths shipped by Phase 5. Component-only role removal finalizes slider capture from `On<Remove, DiegeticPanel>` before Phase 4.5's combined `finalize_widget_anchor_state`; full panel despawn finalizes from the earlier `On<Despawn, DiegeticPanel>` observer before linked-child despawn is queued. Emit one terminal event while `WidgetOf` and both world and screen attachment relations remain queryable, with duplicate suppression when full despawn later reaches the remove observer.

**Files:**
- `src/widgets/slider.rs` — pointer lifecycle, terminal events, and projection use
- `src/widgets/capture.rs`, `src/widgets/button.rs`, `src/widgets/mod.rs` — consume the shipped private occupancy/order authority without changing button behavior
- `src/render/panel_geometry.rs` — extend the existing flat panel-hit projection boundary for captured-pointer rays
- `src/widgets/picking.rs` — read-only consumer/integration; no second ray-to-panel-local implementation
- `src/widgets/reify.rs` — finalize slider lifecycle on kind replacement/removal
- `src/panel/lifecycle.rs` — finalize active slider capture before anchor and owned-entity cleanup
- `src/panel/mod.rs` — extend the early full-panel-despawn finalizer registration
- `src/lib.rs` — curated slider lifecycle-event and cancellation exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical slider pointer-lifecycle exercise

**Constraints from prior phases:** Phase 8 supplies applied state and the proposal API. Phase 8.25 supplies the private cross-button/slider occupancy and raw-order authority while preserving button-specific terminal hooks and `ButtonPress` timing. Same-id/same-kind reify preserves active drag state, Phase 7.25 stable slots/current overrides, and all direct authored widget presentation values. Phase 6.5 supplies the final per-face hit stream: `Interactive` and `WidgetsOnly` may target a slider, while `PanelOnly` and `PassThrough` cannot. Phase 3 supplies panel-local geometry, `render::project_flat_panel_hit`, and the press hit's camera. Phase 4.5 supplies combined world/screen attachment cleanup through `finalize_widget_anchor_state`; Phase 5 supplies the post-interactivity fence over Phase 2's `WidgetDisabled` plus paired `On<Remove>`/early `On<Despawn>` panel finalization. Slider capture finalization extends both lifecycle paths and runs while both attachment relation forms remain queryable. Deterministic pointer integration feeds synthetic `PointerHits` and raw `PointerInput` through Bevy's real dispatcher; it never moves the operating-system pointer or substitutes directly triggered target events.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with tests driven through Bevy's real `pointer_events` dispatcher and real message-maintenance timing, not only manually triggered target events: `Interactive` and `WidgetsOnly` produce slider grabs while `PanelOnly` and `PassThrough` do not; release-over and release-away; non-final/final proposal ordering; zero-size track; drag beyond panel bounds; captured-camera loss and multi-camera reprojection through the shared flat projection boundary; pointer loss, kind change, component-only owner-role removal, full owner-panel despawn, disable-while-dragging, and built-in Escape each terminate exactly once; Escape preserves the captured pointer id and does nothing without active capture; same-id/same-kind refresh preserves an active drag; disabled sliders ignore grabs; both owner teardown paths emit exactly one terminal event while `WidgetOf` and world/screen attachment state remain queryable, before anchor cleanup or linked widget despawn, with no duplicate on the later remove observer. Raw-batch regressions cover release or cancel followed by a new press in both button→slider and slider→button directions, including same-pointer handoff and a second pointer competing for the freed widget; disabled or stale hover processing, resumed hover maintenance, `PickingPlugin` without `InteractionPlugin`, press-count reset/saturation independence, and actions after raw cancel preserve Phase 6's exact-once ordering rules. Extend and smoke-test the public path in `examples/widgets.rs` while preserving all existing diagnostic and input paths and expanding the measured readout instead of clipping it.

### Phase 9 — Direct slider state and thumb presentation  · status: todo

#### Work Order

**Goal:** Application-authored slider trees can mark a thumb, move it from the applied slider value, and directly express root hover, press, focus, and disabled appearance without a preset/theme abstraction or relayout.

**Spec:**
- **Direct anatomy authoring:** add `El::slider_thumb()` as a marker on one ordinary descendant of the nearest `El::slider`. It creates no ECS child and exposes no anatomy component; computed output associates that element's private stable slot with the owning slider. `VisualSlotId::SLIDER_ROOT` and `VisualSlotId::SLIDER_THUMB` are distinct crate-private fixed identities. Zero marked thumbs leaves the headless slider valid with no automatic value visualization.
- **Stable validation contract:** an orphan thumb returns `PanelBuildError::SliderThumbOutsideSlider(PanelElementId)` with "slider thumb `{0}` must be inside a slider subtree"; a second thumb returns `PanelBuildError::SliderHasMultipleThumbs(PanelElementId)` with "slider `{0}` contains more than one thumb". Root state builders mirror the button errors: `SliderStateBackgroundRequiresBackground` displays "slider `{0}` state background requires an authored background"; `SliderStateBorderColorRequiresBorder` displays "slider `{0}` state border color requires an authored border"; and `SliderStateMaterialRequiresSurface` displays "slider `{0}` state material requires an authored background or border". Each carries the slider's `PanelElementId`. Panel construction and `set_tree` return the same variants before any partial update is queued.
- **Application-owned structure:** applications author the track, thumb, optional labels, fixed decoration, sizes, and `DrawZIndex` with ordinary `El` trees. An `El::overlay()` is the natural arrangement but is not required by the API. Widgets v1 supplies no variable-length fill; retained fill resizing and preset structure are deferred to [`widgets-deferred.md`](widgets-deferred.md).
- **Value presentation:** the marked thumb's authored position is the range-start baseline. Use the approved content-box/thumb-center endpoint table from Phase 8.5: translate the thumb by the normalized applied value times `max(content extent - thumb extent, 0)` toward the directed range end, and write only that panel-local XY translation to its private retained slot. Respect all four directions, leave the cross axis unchanged, preserve authored z/depth and hit geometry, and perform no `LayoutTree` or `ComputedDiegeticPanel` regeneration per value change.
- **Direct root state builders:** mirror Phase 7.5's `Slider` builders for optional hovered, pressed, focused, and disabled background, border-color, and `Handle<StandardMaterial>` overrides on the slider root. They use the same authored-target validation, property composition, and root-surface boundary as `Button`; child thumb/label appearance remains application-authored and constant in widgets v1.
- **Presentation ordering and gate:** reuse Phase 7.5's order: focus/applied state → slider state/thumb writer → `WidgetSystems::PresentationCommandsApplied` → `dispatch_visual_overrides` → retained renderer routes. Add a private slider presentation run condition covering changed `SliderState`, authored `WidgetSpec`/slots, `PickingInteraction`, focus, disabled, and the private drag/press marker, including removal edges back to normal. A quiet frame never walks all sliders. The writer still immutable-compares before mutable override access so an unchanged requested override produces no change tick or upload.

**Files:**
- `src/widgets/slider.rs` — direct root-state builders and value/state presentation writer
- `src/layout/builder.rs`, `src/layout/element.rs`, `src/widgets/id.rs` — `El::slider_thumb`, computed association, and validation
- `src/panel/builder.rs` — exact public `PanelBuildError` variants and stable messages
- `src/widgets/visual.rs`, `src/widgets/mod.rs` — reuse retained overrides and presentation ordering
- `src/lib.rs` — keep the curated slider exports; add no preset/style exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical direct slider-presentation exercise

**Constraints from prior phases:** Phase 8 supplies `SliderDirection` and the applied-value contract; Phase 8.5 supplies the completed pointer lifecycle and pressed/drag state. Phase 7.25 supplies stable visual slots and batch re-keying while preserving authored depth. Phase 7.5 supplies the direct root-state convention, role-specific private color overrides, shared immutable-before-mutable writer, and presentation command fence. Slider reuses those paths and does not introduce a second presentation component, theme/preset abstraction, or material surface.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: zero thumbs leaves headless behavior intact; orphan, duplicate-thumb, and missing root-surface targets return the exact stable construction and `set_tree` errors above; the slider-root and thumb slots remain distinct; one thumb tracks the applied value in all four directions from its range-start baseline; pointer projection and thumb presentation share the approved directed endpoint table, including content-box padding/border exclusion, exact center alignment at both endpoints, outside clamping, `T >= C`, and zero-extent headless behavior; direct hover/press/focus/disabled root values follow Phase 7.5 precedence and target rules; first override insertion reaches dispatch through `WidgetSystems::PresentationCommandsApplied`; a value or state change dirties/re-keys only the expected root/thumb records and causes no relayout; a direct run-condition test proves one authored/state insertion, change, or removal re-arms presentation once and a quiet frame skips the all-slider walk; repeated value/state leaves override change ticks and retained uploads untouched; authored thumb depth, slider hit bounds, and child content remain unchanged. Extend and smoke-test the public path in `examples/widgets.rs` while preserving all existing diagnostic and input paths and expanding the measured diagnostic readout.

### Phase 10 — Tooltip template, relationship, and controller reify  · status: todo

#### Work Order

**Goal:** Associated and standalone tooltip declarations produce stable lightweight controller entities related to their targets, without materializing panels or creating anchor demand.

**Spec:**
- **Semantic relationship:** every tooltip is its own entity with public `TooltipTemplate` and `TooltipFor(target)` components; public `Tooltips` is the reverse target membership and is declared with `linked_spawn`, so target despawn owns tooltip-controller cleanup without hierarchy parenting. The target may be a widget or a `DiegeticPanel`. This relationship is semantic ownership, not placement; Phase 10.5 creates a checked same-space panel attachment only after materialization.
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

**Pending decision: tooltip anchor and offset authoring**

Actual problem:
Phase 10 currently gives `TooltipTemplate` only a panel blueprint, timing, and disabled policy, while Phase 10.5 promises anchor/offset defaults and a nonzero world-widget offset. No field or builder currently supplies those placement values, so materialization and template diff behavior are underspecified.

What exists now:
- The plan currently says `TooltipTemplate` contains the blueprint plus `show_after`, `hide_after`, and `disabled_policy`.
- Phase 4.5 supplies typed panel/widget targets, checked attachment mutation, and unit-aware `PanelAnchorOffset`; Phase 10.5 is supposed to consume that one placement path.
- A placement-policy change should not require rebuilding the immutable panel blueprint.

What should change:
- Decide whether `TooltipTemplate` owns private source-anchor, target-anchor, and `PanelAnchorOffset` values with consuming builders.
- Freeze exact defaults and include placement values in template equality/diff classification.
- Define placement-only replacement as a `DiegeticPanelCommands` attachment update on an already materialized controller, without rebuilding its panel blueprint or respawning it.

Recommendation:
Add `.source_anchor(...)`, `.target_anchor(...)`, and `.offset(...)` builders to `TooltipTemplate`, default to `TopCenter → BottomCenter` with `Px(8.0)` downward separation, and classify placement-only changes independently from blueprint replacement. Apply the settled contract in this Phase 10 Work Order and have Phase 10.5 consume it directly.

Approve this direction, or modify it?

**Pending decision: invalid associated tooltip without a widget**

Actual problem:
`El::tooltip(...)` is available on an ordinary element, while associated tooltip identity is keyed by that element's widget id. The plan says the same `El` must be a widget but does not specify what panel construction or `set_tree` returns when it is not.

What exists now:
- Phase 1 already validates widget identity and reports authoring failures through the `thiserror`-derived `PanelBuildError` from both panel construction and runtime `set_tree`.
- Standalone tooltip authoring remains valid for an already-known widget or panel entity and does not use this layout validation.

What should change:
- Add one explicit `PanelBuildError` variant for an associated tooltip declared on a non-widget `El`.
- Run it through the same build/set-tree validation path and pin its stable display message.

Recommendation:
Add `PanelBuildError::TooltipRequiresWidget(PanelElementId)` with a direct `thiserror` message naming the offending element. Reject the entire panel build or tree replacement synchronously and queue no partial controller changes, matching the existing widget validation contract.

- **Template equality and deferred creation:** equality compares panel-blueprint pointer identity plus policy values. Associated authoring carries the same value through `ComputedTooltipRecord`, cloning only the `Arc`; standalone authoring inserts it directly. An identical clone is unchanged, a policy-only replacement is distinguishable from a blueprint replacement, and no phase spawns panel/render components merely because a template exists.
- **Controller reify and fence:** add `TooltipSystems::ReifyControllers` after `WidgetSystems::ReifyCommandsApplied`. It uses `PanelWidgetReader` to create/reuse associated controllers and synchronizes the template plus `TooltipFor(widget)` independently. Follow it with `TooltipSystems::ControllerCommandsApplied`, an explicit `ApplyDeferred` fence consumed by Phase 10.5. Ordering after the widget fence alone is insufficient for systems that need newly created tooltip controllers and relationships.
- **Panel-role ownership:** resolve every controller's target to its owning panel role (`WidgetOf::panel()` for a widget, or the target itself for a panel) and synchronize the existing private `PanelOwned` record independently of `TooltipFor`. Retargeting transfers this ownership. `linked_spawn` still handles target-entity despawn; `PanelOwned` lets Phase 3's central lifecycle clean up controllers when only the target panel's `DiegeticPanel` role is removed and the target entity survives.
- **Identity and cleanup:** same panel/widget id reuses the controller across unrelated, policy-only, and identical-tree refreshes; template fields update independently. Associated declaration removal despawns that controller, and target despawn does the same through `linked_spawn`. Controller indexes follow source revision even when an identical `set_tree` does not change `ComputedDiegeticPanel`; never clear an index unless the same command path also updates/reifies it. Extend both Phase 5 lifecycle paths: component-only panel-role removal cleans controller indexes/ownership from ordinary `On<Remove, DiegeticPanel>`, while full panel despawn performs any required controller finalization from the earlier `On<Despawn, DiegeticPanel>` path before linked widget/tooltip cleanup is queued. Both paths clean each controller exactly once even when `PanelOwned` and linked target cleanup overlap.

**Files:**
- `src/widgets/tooltip.rs` — template and lightweight controller state
- `src/widgets/mod.rs` — `TooltipSystems` controller-reify and deferred-command fence
- `src/widgets/relationship.rs` — `TooltipFor` / `Tooltips`
- `src/widgets/reify.rs` — associated tooltip controller reuse/removal after widget reify
- `src/layout/builder.rs`, `src/layout/element.rs` — separate tooltip declaration and computed record
- `src/panel/builder.rs` — settled associated-tooltip `PanelBuildError` variant and stable message
- `src/panel/lifecycle.rs` — finalize panel-owned controllers before widget/panel-role teardown
- `src/panel/mod.rs` — extend early full-panel-despawn finalization where controller bookkeeping requires live targets
- `src/lib.rs` — curated template and relationship exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical associated and standalone controller exercise

**Constraints from prior phases:** Phase 1 preserves widget/text indexes when an identical `set_tree` advances the source revision without changing computed output; tooltip controller indexes must follow the same rule because no computed change is guaranteed to retrigger reify. Phase 2 provides `PanelWidgetReader` and `WidgetSystems::ReifyCommandsApplied`. Phase 3 provides `PanelOwned`; Phase 5 proved panel teardown has two timing paths, ordinary `On<Remove, DiegeticPanel>` for component-only role removal and earlier `On<Despawn, DiegeticPanel>` finalization before linked-child cleanup on full despawn. This phase extends both where controller bookkeeping needs live targets rather than relying only on linked target-entity despawn. Controller existence alone creates no world or screen demand from Phases 4/4.5.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: associated tooltip authoring does not alter `Button`/`Slider` equality; the settled non-widget associated-tooltip validation returns its stable `PanelBuildError` from both construction and `set_tree` while rejection queues nothing; one lightweight controller and exact reverse relationship exist after `TooltipSystems::ControllerCommandsApplied` with no `DiegeticPanel`, placement relation, render data, or geometry demand; unrelated and policy-only tree replacements reuse that controller; an exact identical `set_tree` replacement preserves the same associated controller and every controller lookup/index without relying on a computed-panel change; a blueprint replacement updates the same controller; a standalone tooltip can target an entity resolved from `(panel, id)` and the same call accepts an already-known event target without lookup; retargeting transfers panel-role ownership; omitted builders produce 500 ms show and zero hide delays plus `Suppress`; associated declaration removal and target despawn each clean up exactly once; component-only target-panel role removal and full target-panel despawn each clean associated/standalone controllers and indexes exactly once through the appropriate lifecycle path. Extend and smoke-test the public path in `examples/widgets.rs` while preserving all existing diagnostic and input paths and expanding the measured readout instead of clipping it.

### Phase 10.5 — Tooltip behavior and lazy panel materialization  · status: todo

#### Work Order

**Goal:** Eligible tooltip controllers materialize once into hidden anchored panels, reveal only when ready, emit visibility events, and thereafter hide/show without respawning.

**Pending decision: split tooltip materialization from visible behavior**

Actual problem:
This Work Order combines dual-space panel construction, typed attachment, layout/placement readiness, transform propagation, hover/focus eligibility, two timers, visibility events, GPU visibility, and four teardown paths. A fresh delegated session must modify nearly every tooltip and panel lifecycle boundary at once.

What exists now:
- Phase 10 supplies a stable lightweight controller plus `TooltipSystems::ControllerCommandsApplied`, but no panel or anchor demand.
- Materialization/readiness has a natural private boundary: request a hidden panel once, then report when its final propagated placement is ready.

What should change:
- Split hidden dual-space materialization/readiness from eligibility, timers, visibility events, and visible-state finalization.
- Keep both phases on the same controller entity and preserve the rule that later hide/show never respawns its panel, layout, or placement state.

Recommendation:
Use Phase 10.5 for a private materialization request/ready contract, hidden panel construction, world/screen attachment, readiness, persistent placement, and hidden cleanup. Add Phase 10.75 for hover/focus eligibility, show/hide timers, `Visibility`, `TooltipShown`/`TooltipHidden`, and duplicate-safe visible finalization. Keep Phase 11 after both phases.

**Pending decision: picking policy for materialized tooltip panels**

Actual problem:
Phase 6.5 requires every panel to state how its front and back participate in the diegetic picking backend. A tooltip panel that defaults to `Interactive` can take hover away from its target; an interactive tooltip instead needs eligibility to remain true while the pointer moves from the target onto tooltip widgets.

What exists now:
- Fairy Dust screen overlays use `PanelPicking::PASS_THROUGH`, and decorative `hana_conduit` panels opt out the same way.
- Tooltip behavior is currently specified as hover-or-focus explanation with rich visual content, not as a second interactive widget surface.

What should change:
- Choose whether the first tooltip API guarantees non-interactive content and installs `PanelPicking::PASS_THROUGH`, or supports interactive tooltip widgets and expands eligibility, focus, capture, teardown, and retargeting rules accordingly.
- Carry the selected policy into Phase 11 replacement/reset tests so rebuilt tooltips never inherit stale picking state.

Recommendation:
Make first-version materialized tooltips `PanelPicking::PASS_THROUGH` on both faces and document that tooltip content is non-interactive. This preserves target hover and keeps the existing timing state machine authoritative. Treat interactive tooltip content as a separate later design using `WidgetsOnly` plus tooltip-hover eligibility.

**Spec:**
- **Picking policy:** apply the owner decision above during materialization and preserve it across hide/show. Under the recommended first-version contract, the tooltip installs Hana-owned `PanelPicking::PASS_THROUGH` on both faces, cannot steal hover from its target or block lower hits, and contains no interactive widgets. If interactive tooltip content is selected instead, this phase must first specify tooltip-hover eligibility and the additional focus/capture/teardown behavior named in the decision block.
- **Eligibility and ordering:** read target `PickingInteraction` plus private `WidgetFocusVisible` where applicable. Pointer focus alone therefore does not keep a tooltip eligible after hover ends. Run after Phase 7.5's `WidgetSystems::FocusCommandsApplied` and Phase 10's `TooltipSystems::ControllerCommandsApplied`, so same-frame interactivity, focus-indicator commands, and newly reified controllers are all visible. `TooltipDisabledPolicy::Suppress` prevents or immediately ends visibility; `Show` leaves hover-or-visible-focus eligibility unchanged.
- **One private state machine:** `TooltipPhase::{Hidden, WaitingToShow(Timer), Visible, WaitingToHide(Timer)}` is authoritative. `show_after` starts when eligibility becomes true and is canceled on loss. `hide_after` starts when eligibility becomes false, is canceled if eligibility returns, and hides on expiry; `Duration::ZERO` is immediate. Target removal despawns immediately. `Visibility` is derived from the phase; no second visible marker or simultaneous timer components. Tick only waiting entities.
- **Visibility events:** public non-propagating `TooltipShown` and `TooltipHidden` derive `EntityEvent` with sole event-target field `entity: Entity`. Emit exactly once per actual hidden→visible or visible→hidden edge. `TooltipShown` observes visible state plus ready layout/placement; `TooltipHidden` observes hidden state and fires before controller cleanup, so effects can still query the entity, transform, template, and relation. Canceled waits and redundant states emit nothing.
- **Lazy materialization and command visibility:** on the first show wait, insert the hidden `DiegeticPanel` on the same controller; pre-materialized controllers carry no panel or placement state. `TooltipSystems::MaterializationCommandsApplied` makes that panel and its synchronized `PanelSpace` visible. A following system then uses `PanelEntityReader` for the controller and target panel plus `PanelWidgetReader` for a widget target, and queues the checked same-space attachment on ordinary `Commands`. `TooltipSystems::AttachmentCommandsApplied` applies that operation before the existing screen-demand and coordinate-specific placement fences. Only after these two stages may readiness reveal the panel. Even `show_after == Duration::ZERO` waits for successful layout and placement and never flashes at a fallback transform. Once materialized, the same controller/panel entity and its layout and placement state remain while hidden. Normal renderer routes may retire hidden GPU batch rows and rebuild them on show; that is not tooltip or panel respawn.
- **Coordinate-space inheritance:** resolve a widget target through `WidgetOf` or read a panel target directly, then branch on the target panel's synchronized `PanelSpace`. World materialization copies its layout unit. Screen materialization copies the complete screen presentation context from the target panel: window, camera order, render layers, and pixel layout unit. It then mints matching Phase 4.5 typed handles and uses the single checked same-space attachment path. World tooltips lower to `hana_valence::AnchoredTo`; screen tooltips retain private panel authoring and use the Phase 4.5 screen widget-target adapter. The materialized placement target must always match `TooltipFor`.
- **Dual-space readiness:** layout is ready only after the materialized controller has a current `ComputedDiegeticPanel`. Screen placement is ready after `PanelSystems::PositionScreenSpace` when the controller's `ResolvedScreenPanelPosition::anchor_position` is `Some`; world placement is ready after `AnchorSystems::Resolve` when the controller still has its Valence attachment and no current-frame `ResolveDiagnostics` entry names it as a skipped source. Emit `TooltipShown` only after the subsequent transform-propagation boundary, so event observers read the final `GlobalTransform`. A fallback or current diagnostic keeps the controller hidden and waiting.
- **Post-transform reveal:** ready-state revelation and `TooltipShown` emission run in `PostUpdate` after `TransformSystems::Propagate`. Materialization and checked attachment remain in their earlier scheduled stages; they do not move into the post-transform system. The show edge therefore exposes the final `GlobalTransform` in the same frame that readiness becomes true.
- **Lifecycle and demand:** pre-materialized `TooltipFor` membership is not anchor demand. Materialization inserts the world/screen placement relationship and starts demand; hiding retains it. World `AnchoredHere` and Phase 4.5's screen reverse relationship keep geometry resident only while placement needs it. There is no inactivity eviction. One duplicate-safe visibility finalizer handles controller despawn, target-entity despawn, component-only target-panel role removal, and full target-panel despawn. The target/entity `On<Despawn>` and the panel's early `On<Despawn, DiegeticPanel>` path emit `TooltipHidden` before linked-spawn hooks can remove `TooltipFor`, widget ownership, or placement relations; the ordinary `On<Remove, DiegeticPanel>` path handles role removal when the panel entity survives. Every path emits at most once while the tooltip template, final transform, semantic target, and world or screen placement relation remain queryable, and panel-role finalization runs before `finalize_widget_anchor_state` or owned-entity cleanup.
- **Defaults:** `show_after` defaults to 500 ms, `hide_after` to zero, and disabled policy to `Suppress`; anchors and offset use the placement contract settled in Phase 10's pending decision. Rich content is ordinary panel content; overflow avoidance is out of scope.

**Files:**
- `src/widgets/tooltip.rs` — state machine, visibility events, first materialization, readiness, and duplicate-safe finalization
- `src/widgets/mod.rs` — tooltip behavior order and materialization deferred fence
- `src/panel/lifecycle.rs` — extend both panel-role teardown paths with tooltip visibility finalization
- `src/panel/mod.rs` — register the early full-panel-despawn finalizer while targets and relations are live
- `src/panel/coordinate_space.rs`, `src/panel/anchoring.rs` — read-only typed-space mirror, attachment readiness, and current world diagnostics
- `src/screen_space/anchoring/resolve.rs`, `src/screen_space/anchoring/placement.rs` — read-only screen placement readiness and current diagnostics
- `src/lib.rs` — curated tooltip visibility-event exports
- `crates/hana_diegetic/examples/widgets.rs` — canonical tooltip timing, visibility-event, and retained-panel exercise
- Read-only: `crates/hana_valence` resolver and attachment graph

**Constraints from prior phases:** Phase 10 supplies stable controller identity, `TooltipFor`/`Tooltips`, the settled placement builders/defaults, template diff classification, and `TooltipSystems::ControllerCommandsApplied`. Phase 7.5 supplies `WidgetSystems::FocusCommandsApplied`, after Phase 5's semantic routing and marker commands; Phase 5.5's built-in adapter and app-owned path both feed that same routing before the fence, so tooltip eligibility needs no adapter-specific scheduling. Tooltip eligibility runs after the focus fence rather than merely after `WidgetSystems::InteractivityCommandsApplied`. Phase 6.5 supplies the final per-face hit stream that drives pointer-hover eligibility. Phase 3 supplies `PickingInteraction` and the owner-panel teardown contract; Phases 4 and 4.5 supply world/screen widget target geometry, typed same-space placement, demand, and `finalize_widget_anchor_state`. `PanelSpace` is a required mirror synchronized on panel replacement and conversion; coordinate-space branches use it, then mint live handles through `PanelEntityReader`/`PanelWidgetReader` after the relevant command fence. A materialized tooltip is an active attachment even while hidden, so converting its source or target panel is rejected until that placement is removed; a pre-materialized semantic `TooltipFor` alone does not block conversion. Visible-tooltip finalization must run through both panel lifecycle paths and the target's early despawn path before combined anchor or linked-spawn cleanup. Deterministic hover integration feeds synthetic `PointerHits` and raw `PointerInput` through Bevy's real dispatcher; it never moves the operating-system pointer or substitutes directly triggered target events.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: materialized tooltip panels carry the settled Hana-owned `PanelPicking` policy and preserve its promised hover/blocking behavior; synthetic `PointerHits` plus raw `PointerInput` drive pointer eligibility through Bevy's real dispatcher without operating-system pointer movement; show wait cancels; hide grace cancels on renewed eligibility and `Duration::ZERO` hides immediately; current-frame pointer/app/traversal focus is visible to eligibility after `WidgetSystems::FocusCommandsApplied`; first show materializes hidden through panel fence → typed-handle acquisition → attachment fence and reveals only after layout+placement, including zero-delay world and screen widget targets plus `Fit`; the `PostUpdate` reveal after `TransformSystems::Propagate` emits `TooltipShown` with the final `GlobalTransform` in that same ready frame; every later hide/show preserves the same controller/panel entity and resident layout/placement state, while hidden GPU rows may retire and rebuild through normal renderer paths; `Suppress` immediately hides an already-visible tooltip; `TooltipShown` and `TooltipHidden` target the tooltip entity exactly once per completed visibility edge and emit nothing for canceled waits or redundant states; visible associated removal, controller despawn, and target-entity despawn each emit one hidden event before linked-spawn cleanup while hidden cleanup emits none; component-only target-panel role removal and full target-panel despawn each emit one hidden event through the appropriate lifecycle path before combined anchor cleanup, while `TooltipFor` and world or screen placement state remain queryable, with no duplicate on the later remove observer; standalone and associated tooltips share the path; a secondary-window screen target transfers its window, camera order, and render layers; a world widget tooltip honors the settled nonzero offset. Extend and smoke-test the public path in `examples/widgets.rs` while preserving all existing diagnostic and input paths and expanding the measured readout instead of clipping it.

### Phase 11 — Tooltip replacement and retargeting  · status: todo

#### Work Order

**Goal:** A materialized tooltip updates its application-authored template or target without replacing its controller entity.

**Spec:**
- **Template diff handling:** an identical template does nothing. A policy-only replacement updates current and future timers without rebuilding panel components. A placement-only replacement queues one checked attachment update after typed-handle acquisition and the attachment fence, preserving the materialized panel and blueprint. A new blueprint first transitions hidden, emitting `TooltipHidden` if visible, then runs one private panel reset path and rebuilds hidden on the same controller entity.
- **Complete reset:** blueprint replacement reuses the settled `DiegeticPanel` role-removal teardown instead of hand-maintaining a second cleanup list. It removes every materialized layout, render, widget, text, placement, retained-sibling, index, and Hana-owned `PanelPicking` component, including `PanelWidgetIndex`. An explicit deferred-command fence completes that role removal before the replacement blueprint is installed. The replacement builder's settled initial `PanelPicking` then uses Phase 6.5's existing absent-only ownership path, so no stale picking policy, render record, index, or placement survives. Reveal resumes only after fresh layout and placement readiness.
- **Retargeting:** a same-space, same-layout-unit target change synchronizes `TooltipFor`, `PanelOwned`, inherited presentation context, and the checked placement target in place. A coordinate-space or layout-unit change first hides and detaches, waits for the attachment command fence, resets materialized panel state, and rematerializes on the same controller with the new target context. Conversion is never attempted while attached, and the materialized placement target always matches `TooltipFor`.
- **Application-authored presentation:** every tooltip continues to display the ordinary `LayoutTree` stored by its `TooltipTemplate`; this phase adds no default layout, preset, style, variant, or theme API. That design belongs to [`widgets-deferred.md`](widgets-deferred.md) after button, slider, and tooltip APIs have been used together.

**Files:**
- `src/widgets/tooltip.rs` — template diff classification, replacement, retargeting, and rematerialization
- `src/widgets/mod.rs` — replacement/retarget ordering after the tooltip command fences
- `src/panel/diegetic_panel.rs`, `src/panel/lifecycle.rs` — reusable complete panel-role reset
- `src/panel/coordinate_space.rs`, `src/panel/anchoring.rs` — inherited target context and checked replacement attachment
- `crates/hana_diegetic/examples/widgets.rs` — canonical replacement and retargeting exercise with application-authored tooltip trees

**Constraints from prior phases:** Phase 10 supplies stable controller identity, application-authored template diff inputs, semantic relationships, panel-role ownership, and the settled placement builders/defaults. Phase 10.5 supplies the sole visibility state machine, hidden materialization path, readiness gates, duplicate-safe finalizer, persistent controller/panel/layout/placement contract, and settled tooltip picking policy; hidden GPU rows may retire and rebuild through normal renderer paths. Phase 3 supplies the complete panel-role teardown contract; Phases 4/4.5 supply typed same-space attachment, demand, and detach-before-convert behavior; Phase 6.5 supplies absent-only `PanelPicking` installation and ownership-aware teardown. Replacement and retargeting must call the existing lifecycle, attachment, picking, and renderer paths rather than creating parallel authorities.

**Acceptance gate:** `cargo nextest run -p hana_diegetic --lib` green with new tests: a new application-authored blueprint completes role-removal teardown before installing the replacement, resets every old panel/widget/text/render/placement/picking component, rebuilds hidden on the same controller, and receives a fresh Hana-owned copy of the settled tooltip `PanelPicking`; an identical clone does nothing; policy-only and placement-only changes do not rebuild; placement-only replacement updates the checked attachment after its fence; same-space/same-unit retargeting updates ownership, inherited context, and attachment in place; cross-space or different-unit retargeting emits one hidden event if visible, detaches, and rematerializes the same controller without attempting an attached conversion; reveal waits for fresh layout and placement; repeated equal replacement or retargeting is a no-op. Extend and smoke-test the public path in `examples/widgets.rs` with application-authored tooltip trees while preserving all existing diagnostic and input paths and expanding the measured readout instead of clipping it.

### Phase 12 — Demonstration checkpoint (stop and discuss)  · status: todo

#### Work Order

**Goal:** Decide, with the project owner, how to demonstrate the widget system. This phase is a discussion checkpoint, not delegated implementation.

**Spec:**
- Stop after Phase 11 and use `crates/hana_diegetic/examples/widgets.rs` as the cumulative dual-space Fairy Dust baseline: Phase 4's world readout still follows the bottom slider through typed world handles, Phase 4.5 adds a distinct typed screen-widget attachment exercise, Phase 5.5 leaves both Hana's built-in per-window controls and an app-owned Bevy Kana action visibly exercised, Phase 6 leaves visible `Pointer`, `Focus`, and `Button` rows proving pointer and semantic activation separately, Phase 7 adds the persistent `Callback` row and primary-button callback count, and Phase 6.5 leaves the world Widget Lab front `Interactive`/back `PanelOnly` while Fairy Dust screen overlays remain `PASS_THROUGH`. Design together how that lab is extended or supplemented; do not reopen which example owns the cumulative widget path, remove either input integration proof, replace those diagnostic rows, change those established picking policies, or clip them when adding later output.
- The plan must prove the pieces work together in real diegetic UI: buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and existing IME/text input coexisting on one panel.

**Files:** `crates/hana_diegetic/examples/widgets.rs` is the read-only baseline until the discussion lands; no implementation files are selected yet.

**Constraints from prior phases:** All widget subsystems through Phase 11 complete and are tested through deterministic minimal-app coverage before demonstration work begins; the canonical example already retains both world- and screen-widget anchoring paths through the same-space public API plus the built-in adapter and app-owned Bevy Kana input paths, and conversion tests demonstrate detach → convert → reattach rather than cross-space fallback.

**Acceptance gate:** A written demonstration plan agreed with the project owner, anchored on the existing `examples/widgets.rs` lab and naming any supplemental examples; no code gate.

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
- **Relationship decision:** tooltips are separate lightweight entities with `TooltipFor(target)` / `Tooltips`, never fields inside `Button`, `Slider`, or `WidgetSpec`. Associated declarations reify a controller related to the widget; standalone tooltips use the same relation directly. First eligibility materializes the controller into an anchored panel, inheriting the target's coordinate space. The linked-spawn relationship target owns entity-despawn cleanup; a private `PanelOwned` record mirrors the target's owning panel role so removing only `DiegeticPanel` provides the same controller lifetime boundary.
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
