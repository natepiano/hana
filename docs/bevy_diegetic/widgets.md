# Headless Widgets

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Adds headless widgets (buttons, sliders, tooltips, focus, interactivity) to `bevy_diegetic`: widgets own semantic behavior and typed events, visuals stay ordinary layout primitives, widgets materialize as panel child entities targeted by Bevy picking, and anchoring comes from `hana_valence`.

## Delegation Context

- **Project** — `bevy_diegetic` (workspace member at `crates/bevy_diegetic`). Diegetic UI layout engine for Bevy — in-world panels driven by a Clay-inspired layout algorithm. This plan adds a headless `widgets` module that materializes widgets as panel child entities.
- **Stack** — Rust (edition 2024). Bevy `0.19.0` (workspace pin, `crates/bevy_diegetic/Cargo.toml:14`). `bevy_picking` + `mesh_picking` features already enabled (reuse `bevy_picking::Hovered`/`PickingInteraction`; custom picking backend; no bevy_ui). `bevy_enhanced_input` `0.26.0` at workspace level for semantic-action adapters. `hana_valence` is a workspace path dep (`Cargo.toml:43`).
- **Layout** (only phase-touched paths):
  - `crates/bevy_diegetic/src/widgets/` — NEW module: `mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `id.rs`, `relationship.rs`, `interactivity.rs`, `focus.rs`, `picking.rs`, `reify.rs`, `presets/` (`mod.rs`, `button.rs`, `slider.rs`, `tooltip.rs`, `style.rs`).
  - `crates/bevy_diegetic/src/ime/` — `activation.rs`, `field.rs`, `ids.rs`, `mod.rs` (`ImePlugin`).
  - `crates/bevy_diegetic/src/panel/` — `builder.rs`, `anchoring.rs`, `anchor_geometry.rs`, `diegetic_panel.rs`, `world_anchoring.rs`, `attachment_resolver.rs`, `perf.rs`.
  - `crates/bevy_diegetic/src/render/panel_text/` — `reconcile.rs`, `relationship.rs`, `mod.rs`.
  - `crates/bevy_diegetic/src/screen_space/anchoring/` — `candidate.rs`, `placement.rs`, `projection.rs`, `rect.rs`, `resolve.rs`, `window.rs`, `mod.rs`.
  - `crates/bevy_diegetic/src/cascade/` — `attributes.rs`, `resolved.rs`, `cascade_set.rs`, `plugin.rs`, `mod.rs`.
  - `crates/bevy_diegetic/src/lib.rs` — curated public re-exports.
- **Key files:**
  - `src/panel/builder.rs` — panel builder; `PanelBuildError` (`:45`), `DuplicateElementId(PanelElementId)` (`:51`), `build()` calls `tree.duplicate_named_element_id()` (`:680`). Widget ids reuse this exact validation path.
  - `src/render/panel_text/reconcile.rs` — text reify: `reconcile_panel_text_children` (rename target), `update_reused_panel_text_child` (`:419`, the reuse-on-diff pattern widget reify mirrors).
  - `src/render/panel_text/relationship.rs` — `TextRunOf` / `PanelTextRuns` (template for `WidgetOf`/`PanelWidgets`; no `linked_spawn`).
  - `src/render/panel_text/mod.rs` — text-child ordering in `PanelChildSystems::Build` (`:104`).
  - `src/ime/activation.rs` — IME double-click activation observer: `On<Pointer<Click>>` gated `click.count < 2` (`:28`); calls `computed.field_at_local_position(panel_local)` (`:39`).
  - `src/panel/diegetic_panel.rs` — `field_at_local_position(&self, panel_local: Vec2) -> Option<&PanelFieldRecord>` (`:1591`); panel-local record-lookup pattern for the picking backend.
  - `src/ime/ids.rs` — id types; `PanelElementId::auto` (`:64`). Widget ids land in this element-id namespace; no new `WidgetId` newtype.
  - `src/ime/mod.rs` — `ImePlugin` (`pub(crate)` `:70`, `impl Plugin` `:89`); mirror for `WidgetsPlugin`.
  - `src/cascade/mod.rs` — `Override<A>`, `Resolved<A>`, `resolve_walk` parent-walk, most-specific-wins (`:43`, `:75`, `:95`).
  - `src/cascade/attributes.rs`, `src/cascade/resolved.rs` — attribute defs + `Resolved<A>` cache.
  - `src/panel/anchor_geometry.rs` — current diegetic geometry provider: `PanelAnchorGeometryParam` (`:34`), `ResolvedPanelAnchorGeometry` with `from_screen_panel`/`from_world_panel` (`:67`, `:79`). The `ResolvedAnchorGeometry` widgets publish is the `hana_valence` contract component (see `../hana_valence/initialize.md`).
  - `src/panel/world_anchoring.rs` + `src/panel/attachment_resolver.rs` — panel anchoring bridge keyed on `AnchoredToPanel` (`AnchoredTo`/`AnchoredHere` are the hana_valence successors).
  - `src/screen_space/anchoring/candidate.rs` — screen placer builds candidate rects from panels only today; Phase 11 teaches it widget targets. Ordered `.after(PanelChildSystems::Build)`.
  - `src/panel/perf.rs` — `DiegeticPerfStats` (`:45`), `pub reconcile_ms: f32` (`:54`, rename target), `DIAG_PANEL_RECONCILE_MS` (`:258`).
  - `src/render/mod.rs` — `PanelChildSystems` set enum (`:128`); `TextRunOf`/`PanelTextRuns` re-exports.
  - `src/lib.rs` — curated re-exports (`PanelBuildError` `:255`); widget public types re-export here.
- **Build:** `cargo build && cargo +nightly fmt` after changes.
- **Test:** `cargo nextest run` (never `cargo test`).
- **Lint:** the `clippy` skill. Workspace lints are strict: `all`/`cargo`/`nursery`/`pedantic` denied, `unwrap_used`/`expect_used`/`panic`/`unreachable` denied, `missing_docs = "deny"`, `self_named_module_files` denied (use `module/mod.rs` directory form).
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_hana`
- **Invariants:**
  - **Valence gate:** `hana_valence` exists at `crates/hana_valence`, but Phases 1+ are gated on its milestones 1 and 4 — skeleton + resolver extracted, panel bridge and screen-placer re-pointed — per `../hana_valence/initialize.md`. The gate excludes M4's `panel_anchoring` example port (that transitively needs M2). hana_valence types stay out of diegetic's public signatures; diegetic authoring helpers insert `hana_valence::AnchoredTo` internally.
  - No bevy_ui / bevy_a11y dependency. `WidgetDisabled`, `WidgetFocused`, `ButtonPress` stay bespoke; only already-present deps may be reused (`bevy_picking::Hovered`, `bevy_enhanced_input`).
  - Widgets materialize as panel child entities under `ChildOf(panel)`; the `WidgetOf`/`PanelWidgets` relationship is a traversal index only, no `linked_spawn` — `ChildOf` owns despawn.
  - Behavior modules never construct layout/render primitives (`El`, `LayoutTree`, `PanelDraw`, materials, `TextStyle`, `DrawZIndex`). Presets depend on behavior, never the reverse.
  - No relayout on hover/press/focus/disabled flips: pure-visual component-level writes only; presets must not regenerate their `LayoutTree` fragment on state change (that trips `Changed<DiegeticPanel>` and forces a full relayout).
  - Change-gated systems, never unconditional per-frame walks: reify gated on `Changed<ComputedDiegeticPanel>` and reuses entities by id; interactivity resolver writes `WidgetDisabled` only on diff; anchor-geometry fill is lazy (`With<AnchoredHere>`-gated).
  - Widget ids reuse `PanelElementId` and its `duplicate_named_element_id` → `DuplicateElementId` validation; event-emitting widgets require `Named` ids (auto ids reposition on structural edits and would fire spurious cancels).
  - Cascade attributes use the existing `Override<A>`/`Resolved<A>`/`resolve_walk` convention, most-specific-wins.
  - Widget events derive `EntityEvent` targeting the widget entity; the panel-local id is a payload convenience only, never the routing key. Owning panel resolves through `WidgetOf`, never duplicated on components or events.
  - Widget picking geometry stays in **panel-local space** (surface-panels coordination: the panel's single `PanelSurface::project()` converts world hits to panel-local at the panel boundary, so widget picking needs no curvature logic; never place widget picking geometry independently in world space).

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
- any test files referencing the renamed items (find with `rg -n "reconcile" crates/bevy_diegetic`)

**Constraints from prior phases:** None. Note: this phase is independent of the valence gate; the gate blocks Phase 1, not this rename.

**Acceptance gate:** `cargo build && cargo +nightly fmt` clean; `cargo nextest run` green; `rg -n "reconcile" crates/bevy_diegetic` shows no remaining uses in the entity-creation sense.

### Phase 1 — Widget identity, authoring, relationship, reify, plugin skeleton  · status: todo

#### Work Order

**Goal:** Widgets can be authored in a panel's element tree and materialize as reused, relationship-indexed panel child entities.

**Precondition (verify before starting):** hana_valence milestones 1 and 4 complete per the Valence gate invariant. If not landed, stop and report.

**Spec:**
- **Ids** (`widgets/id.rs`): widget ids ARE `PanelElementId` — no newtype. Event-emitting widgets require `Named` ids; reject auto/positional ids for widgets at build time. Duplicate rejection comes free: the widget id lands in the element-id namespace, so `duplicate_named_element_id` → `PanelBuildError::DuplicateElementId` already covers it. `id.rs` holds only validation helpers (e.g. the named-id requirement).
- **Authoring** (`src/panel/builder.rs` + element config types): an `El` config method `.button(id, spec)` mirroring `.editable_field(id, spec)` — NOT an `El::button` constructor (collides with the layout-mode-constructor role of `row`/`column`/`overlay`) and NOT a `LayoutBuilder` leaf (leaves cannot hold children; a button wraps arbitrary child layout). The builder stores `common.widget: Option<WidgetSpec>` carried onto `Element` parallel to `editable`. `WidgetSpec` is a type-erased record with a kind (button first; slider added in Phase 8); it must be `Clone` + comparable for diffing.
- **Relationship** (`widgets/relationship.rs`): `WidgetOf` / `PanelWidgets`, modeled on `TextRunOf`/`PanelTextRuns` (`src/render/panel_text/relationship.rs`). No `linked_spawn` — widgets sit under `ChildOf(panel)`, which owns despawn; the relationship is a traversal index only.
- **Reify** (`widgets/reify.rs`): a change-gated system (`Changed<ComputedDiegeticPanel>`) walking the computed tree's widget records. Reuses existing widget entities keyed by widget id; writes components only on diff — mirror `update_reused_panel_text_child` (`src/render/panel_text/reconcile.rs:419`, post-Phase-0 name `reify.rs`). An in-flight press/drag/focus survives an unrelated layout recompute because the entity is reused, not recreated.
- **Plugin** (`widgets/mod.rs`): `WidgetsPlugin` (`pub(crate)`, mirror `ImePlugin` at `src/ime/mod.rs:70`) + a `WidgetSystems` set. Reify runs in `PanelChildSystems::Build` after precompose caches, like text reify (`src/render/panel_text/mod.rs:104`). Register the plugin where `ImePlugin` is registered.
- **Module structure:** private `widgets` module next to `ime`; curated public types re-exported from `lib.rs`/`widgets/mod.rs`, never the whole tree.

**Files:**
- `src/widgets/mod.rs`, `src/widgets/id.rs`, `src/widgets/relationship.rs`, `src/widgets/reify.rs` — new
- `src/panel/builder.rs` — `.button(id, spec)` config method + `common.widget`
- `src/lib.rs` — re-exports + plugin registration site
- Read-only templates: `src/render/panel_text/relationship.rs`, `src/render/panel_text/reify.rs`, `src/ime/mod.rs`

**Constraints from prior phases:** Phase 0 renamed `reconcile_panel_text_children` → `reify_text_entities` and `reconcile.rs` → `reify.rs`.

**Acceptance gate:** `cargo nextest run` green with new tests: duplicate widget id rejected via `DuplicateElementId`; auto-id widget rejected; reify creates widget entities under `ChildOf(panel)` with `WidgetOf`/`PanelWidgets`; a structural edit keeps named widget entities (reuse, not respawn); panel despawn drops all widgets without double-despawn (analogous to `panel_despawn_drops_all_runs_without_double_despawn`).

### Phase 2 — Interactivity resolution  · status: todo

#### Work Order

**Goal:** Enabled/disabled resolves across global, panel, layout-subtree, and widget levels into a `WidgetDisabled` marker on widget entities.

**Spec:**
- Authoring enum (`widgets/interactivity.rs`):
  ```rust
  pub enum WidgetInteractivity {
      Inherited,
      Enabled,
      Disabled,
  }
  ```
  (flattened — no nested `InteractionState`). Runtime shape: `pub struct WidgetDisabled;` — a presence marker queried via `Has<WidgetDisabled>`, mirroring Bevy's `InteractionDisabled` pattern but bespoke (bevy_ui is not a dependency). No `ResolvedWidgetInteractivity` type, no `enabled: bool` authoring, no disabled reason.
- **Cascade:** implement on the existing `Override<A>`/`Resolved<A>`/`resolve_walk` convention (`src/cascade/mod.rs:43,75,95`): `WidgetInteractivity::Enabled/Disabled` becomes an `Override<WidgetInteractivity>`; precedence is most-specific-wins (first override starting at the entity itself). A child `Set(Enabled)` inside a disabled panel is enabled. Sticky-ancestor-disabled is rejected as inconsistent with every other diegetic cascade attribute.
- **Layout-subtree level:** layout Els have no entity, so the subtree level is delivered by pre-seeding widget `Override`s during reify's tree-walk (the walk Phase 1 already performs): when an ancestor El sets disabled, reify stamps the widget entity's `Override<WidgetInteractivity>`.
- **Resolver:** change-gated on any cascade source change; inserts/removes `WidgetDisabled` only on diff — never an unconditional per-frame walk (archetype-move churn across hundreds of widgets).
- Disabled changes are visual/state-only by default: no layout recompute unless a preset explicitly opts into different content or dimensions.

**Files:**
- `src/widgets/interactivity.rs` — new
- `src/widgets/reify.rs` — subtree pre-seed in the tree-walk
- `src/cascade/attributes.rs` — attribute registration
- `src/panel/builder.rs` — El-level interactivity authoring
- Read-only: `src/cascade/mod.rs`, `src/cascade/resolved.rs`

**Constraints from prior phases:** Phase 1 built `widgets/reify.rs` (change-gated tree-walk, reuse by id), `WidgetSpec` on `common.widget`, and `WidgetsPlugin`/`WidgetSystems`.

**Acceptance gate:** `cargo nextest run` green with new tests: most-specific-wins precedence (child `Enabled` inside disabled panel is enabled); layout-subtree disable pre-seeds widget overrides; resolver writes `WidgetDisabled` only on diff (assert no archetype move when state is unchanged across a relayout).

### Phase 3 — Widget `Transform`, single rect source, custom picking backend  · status: todo

#### Work Order

**Goal:** Widgets are first-class Bevy picking targets via a custom backend testing panel-local rects; pointer hover works on widget entities.

**Spec:**
- **Transform:** widgets carry a real panel-local `Transform` — translation = the widget's panel-local offset; `GlobalTransform` propagates via `ChildOf(panel)`. This is deliberately unlike text runs (which carry no `Transform`; their placement is baked into run records) — copying the text-run shape would break the picking backend and collapse anchor geometry to the panel origin.
- **Single rect source:** reify writes each widget's panel-local rect exactly once. Picking bounds (this phase) and anchor-geometry points (Phase 4) are both projections of that one rect — no duplicate rect computation with divergent triggers.
- **Picking backend** (`widgets/picking.rs`): a system in `PickingSystems::Backend` emitting `PointerHits` — bevy_picking backends only produce hits; all downstream events/observers work unchanged. Per pointer: raycast the panel surface once (reuse the existing panel interaction-mesh hit path), convert to panel-local coordinates (the `field_at_local_position` pattern, `src/panel/diegetic_panel.rs:1591`), test widget rects, emit hits targeting widget entities. Emit widget hits with depth slightly nearer than the panel so widgets are the deeper (first) pick target — this ordering is what widget-vs-IME observer precedence relies on in Phase 6. Panel-local space keeps curvature out: on curved panels `PanelSurface::project()` handles the world→panel-local mapping at the panel boundary (surface-panels invariant).
- **Hover:** insert `bevy_picking::Hovered` on materialized widget entities (opt-in, immutable, change-detected — already a dependency; no bespoke `WidgetHovered`). `PickingInteraction` remains available for pressed/hovered/none styling.

**Files:**
- `src/widgets/picking.rs` — new (backend)
- `src/widgets/reify.rs` — Transform + rect writes, `Hovered` insertion
- Read-only: `src/panel/diegetic_panel.rs`, the panel interaction-mesh picking path

**Constraints from prior phases:** Phase 1: widgets reified under `ChildOf(panel)`, reuse keyed by id, `WidgetSystems` set exists. Phase 2: `WidgetDisabled` marker exists (backend may still report hits on disabled widgets; behavior systems gate on the marker).

**Acceptance gate:** `cargo nextest run` green with new tests: pointer over a widget rect yields `Pointer<Over>`/`Pointer<Out>` targeting the widget entity; the widget hit is deeper (preferred) over its panel; `Hovered` flips on hover enter/exit; an off-origin widget picks at its actual location (Transform correctness).

### Phase 4 — Lazy anchor-geometry publication  · status: todo

#### Work Order

**Goal:** Entities can anchor to widgets: widget reify publishes `hana_valence` `ResolvedAnchorGeometry` on demand.

**Spec:**
- Publish `ResolvedAnchorGeometry` (the hana_valence contract component — the valence resolver reads a component, never a diegetic `SystemParam`) on widget entities **lazily**: only widgets `With<AnchoredHere>`, triggered on widget-layout change or `Added<AnchoredHere>`. Never eager on every widget (a resident ~9-entry map per untargeted widget, hundreds mostly never read), and never `Changed<Transform>`-gated refill (valence M34 forbids it).
- Runs inside valence's `AnchorSystems::FillGeometry` set, before `Resolve`, so panel and widget geometry providers order cleanly ahead of the resolver — and in the same frame as reify so widget geometry is published before tooltip/anchor resolve.
- Geometry points are projections of the Phase 3 single rect, expressed in the **widget-local frame** matching the panel provider's centered convention; the resolver composes `global_transform * geometry[anchor]`, which is why the widget's own `Transform` must carry its panel-local offset.
- Publication lives in the same system that writes the rect (no divergent triggers).
- **Diagnostics:** warn once per target-and-reason (not once-ever) when `AnchoredTo` targets an entity lacking `ResolvedAnchorGeometry` or a despawned target — parity with valence M16, sharing valence's dedup key convention so both crates deduplicate the same way.

**Files:**
- `src/widgets/reify.rs` (or a sibling geometry system in `widgets/`) — publication + lazy gating
- `src/widgets/mod.rs` — `AnchorSystems::FillGeometry` set membership
- Read-only: `src/panel/anchor_geometry.rs` (centered-convention reference), `crates/hana_valence` (contract types), `../hana_valence/initialize.md`

**Constraints from prior phases:** Phase 3 built the single panel-local rect source and gave widgets a real panel-local `Transform` (`GlobalTransform` via `ChildOf`). Phase 1's reify is change-gated on `Changed<ComputedDiegeticPanel>`.

**Acceptance gate:** `cargo nextest run` green with new tests: a panel `AnchoredTo` an **off-origin** widget's corner resolves to the correct world position (catches the Transform/centered-frame composition); geometry is absent on widgets nothing anchors to; geometry refills on widget-layout change; missing-geometry warning fires once per target-and-reason.

### Phase 5 — Focus subsystem  · status: todo

#### Work Order

**Goal:** Keyboard/action focus works across all widgets: tracking, traversal, semantic actions, and an enhanced-input adapter.

**Spec:**
- `widgets/focus.rs`. Focus is a shared widget subsystem, not button-local. Track the focused widget entity separately from hover.
- `WidgetFocusable` participation component, inserted on materialized widget entities by default; removing it opts a widget out of keyboard traversal without changing pointer picking.
- `WidgetFocused` presence marker on the focused entity (bespoke — bevy_input_focus is not a dependency), so presets restyle via `Has<WidgetFocused>` component-flip, no relayout.
- Focus gained by: pointer focus, keyboard traversal, semantic action routing, app request. Lost by: transfer, disable, despawn/removal, panel/window input-scope loss, explicit clear.
- Semantic focus actions: next, previous, first, last, activate-focused, cancel-focused. Mirror `bevy_lagrange` input patterns: semantic action types, neutral control summaries, and a thin `bevy_enhanced_input` adapter — no raw key handling embedded in widgets.
- Disabled widgets may retain or receive focus, but widget behavior ignores activation/change input while disabled (activation gating happens in behavior systems, not focus).
- Design with accessibility in mind (structure the traversal so an a11y layer can attach later), without adding bevy_a11y.

**Files:**
- `src/widgets/focus.rs` — new
- `src/widgets/mod.rs` — systems in `WidgetSystems` after picking
- `src/widgets/reify.rs` — default `WidgetFocusable` insertion
- Read-only reference: `bevy_lagrange` semantic-action/control-summary patterns (`/Users/natemccoy/rust/bevy_hana/crates/bevy_lagrange`)

**Constraints from prior phases:** Phase 2: `WidgetDisabled` presence marker. Phase 3: widgets are pick targets with `Hovered`. Activation of a focused button (emitting `ButtonClicked`) lands in Phase 6 — this phase only routes the activate-focused action to a seam Phase 6 fills.

**Acceptance gate:** `cargo nextest run` green with new tests: traversal next/previous/first/last order; focus loss on disable, despawn, and explicit clear; `WidgetFocusable` removal skips the widget in traversal; disabled widget retains focus but the activate-focused action is a no-op on it.

### Phase 6 — Button behavior  · status: todo

#### Work Order

**Goal:** Headless button with the four-event lifecycle, emulated pointer capture, semantic activation, and IME coexistence.

**Spec:**
- `widgets/button.rs`. Events derive `EntityEvent` targeting the widget entity, carrying the panel-local id as a payload convenience: `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, `ButtonCanceled`. No double-click event.
- **Lifecycle invariant** — a pressed button resolves to exactly one terminal path:
  - `Pressed -> Released -> Clicked` for a valid pointer click.
  - `Pressed -> Released` without `Clicked` for a valid release that no longer activates.
  - `Pressed -> Canceled` for capture loss, disable-while-pressed, despawn/removal, panel/tree replacement, pointer cancellation/removal, or explicit cancel.
  - Semantic activation emits `ButtonClicked` without entering the pointer lifecycle.
- **Emulated capture** (bevy_picking has no capture API): press inserts `ButtonPress { pointer: PointerId }` on the widget. Facts the implementation relies on: there is **no drag threshold** — `DragStart` fires on the first non-zero move while pressed, so a still click never enters the drag lifecycle; a release over empty space emits **no** `Pointer<Release>` at all, making `DragEnd` (which keeps dispatching to the origin entity) the only terminal signal for drag-off-then-release-in-void; `Pointer<Release>`/`Pointer<Cancel>` observers must be **global** (`add_observer`), not entity observers, because those events target the currently-hovered entity. (`bevy_ui_widgets::Button` implements exactly this shape.)
- **Choke point:** centralize terminal-event emission in one `On<Remove, ButtonPress>` observer that inspects the removal cause and emits exactly one terminal event — exactly-one-terminal-path is structural, not a convention across five observers.
- **Disable-while-pressed:** inserting `WidgetDisabled` on a pressed button must actively remove the live `ButtonPress` with a Canceled cause — a flag alone lets the pending Release/DragEnd resolve as Clicked. Disabled buttons ignore pointer and semantic activation and cannot keep capture.
- **Semantic activation:** a non-pointer path (keyboard shortcuts, action systems, the Phase 5 activate-focused seam) targeting the focused or an explicitly targeted button; emits `ButtonClicked` directly, no fabricated pointer events.
- **IME precedence:** widget entities are the deeper pick target (Phase 3), so widget observers see pointer events first and call `propagate(false)` before panel-level IME double-click activation (`src/ime/activation.rs:28`). Schedule ordering does not govern observer trigger order — pick depth/bubbling does.
- Hover state comes from `bevy_picking::Hovered` (Phase 3); no hover enter/exit events in the first API.

**Files:**
- `src/widgets/button.rs` — new
- `src/widgets/mod.rs` — observers + systems registration
- Read-only: `src/ime/activation.rs`, `/Users/natemccoy/rust/bevy/crates/bevy_ui_widgets/src/button.rs` (shape reference)

**Constraints from prior phases:** Phase 1: `WidgetSpec` button kind; reify reuse means in-flight `ButtonPress` survives unrelated relayouts. Phase 2: `WidgetDisabled` via `Has<>`. Phase 3: widget hits are deeper than panel hits; `Hovered` present. Phase 5: activate-focused action seam to wire to semantic activation.

**Acceptance gate:** `cargo nextest run` green with new tests: press→release→click sequence; release-without-click; every cancel path — capture loss (drag off + release in void, via DragEnd), **disable-while-pressed asserts `ButtonCanceled` (not Released)**, widget despawn/removal, explicit cancel; semantic activation emits `ButtonClicked` alone; a button over an editable field consumes the click and IME double-click activation still works beside it (coexistence test).

### Phase 7 — `.on_click` sugar + ButtonPreset  · status: todo

#### Work Order

**Goal:** Ergonomic click handling and a default button visual preset.

**Spec:**
- **Event consumption, base path:** app code writes a global observer for `ButtonClicked` filtered by widget id; document the id→Entity lookup through `PanelWidgets`. This ships alongside the sugar, not instead of it.
- **`.on_click` sugar:** a raw closure cannot live in the type-erased `WidgetSpec` (records are `Clone`/comparable; `IntoObserverSystem` closures are neither). Instead: `.on_click(closure)` registers the closure as a system and stores its `SystemId` (plain data) in the widget record; reify inserts one uniform observer that runs the stored `SystemId` on `ButtonClicked`. Same mechanism as bevy_ui_widgets callbacks.
- **ButtonPreset / ButtonStyle** (`widgets/presets/button.rs`, shared helpers in `presets/style.rs`): helper builders generating `LayoutTree` fragments. Material-first: plain colors and images are convenience inputs resolving to `StandardMaterial` (color stays zero-ceremony, parity with `.background(color)`); custom animation/shader cases use custom material handles or `ExtendedMaterial`. Widget-specific names only — no `WidgetSurface`/`WidgetMaterial`/`Paint` shared nouns. Presets read `Hovered`, `Has<WidgetDisabled>`, `Has<WidgetFocused>`, and press state, and restyle via component-level writes (material handle, color, `DrawZIndex`) on already-materialized children — never regenerating the `LayoutTree` fragment on state change. Rich content allowed: text, images/icons, custom materials, animation hooks.
- **Boundary guardrail:** presets depend on behavior, never the reverse; add a test/lint asserting behavior modules (`button.rs`, `focus.rs`, `interactivity.rs`, …) reference no layout/material types.

**Files:**
- `src/widgets/presets/mod.rs`, `src/widgets/presets/button.rs`, `src/widgets/presets/style.rs` — new
- `src/widgets/button.rs` or `src/panel/builder.rs` — `.on_click` on the button spec/builder
- `src/widgets/reify.rs` — uniform `SystemId`-running observer insertion
- `src/lib.rs` — preset re-exports

**Constraints from prior phases:** Phase 6 defined the four `EntityEvent`s and the `ButtonPress` lifecycle. Phase 1's `WidgetSpec` is `Clone` + comparable — the `SystemId` field must preserve that.

**Acceptance gate:** `cargo nextest run` green with new tests: `.on_click` closure runs on click; global-observer path works via `PanelWidgets` lookup; hover/press/disabled/focus restyle causes no relayout (assert `Changed<DiegeticPanel>` does not fire on a hover flip); behavior-module boundary test passes.

### Phase 8 — Slider behavior  · status: todo

#### Work Order

**Goal:** Headless slider: grab, drag, value change, release, cancel, disabled, optional snapping, with correct out-of-bounds drag mapping.

**Spec:**
- `widgets/slider.rs`; extend `WidgetSpec` with the slider kind and add the `.slider(id, spec)` authoring method mirroring `.button`.
- **Types:** `SliderDirection` is a single four-variant enum — `LeftToRight`, `RightToLeft`, `BottomToTop`, `TopToBottom` (never `vertical: bool` + `reversed: bool`); bottom-to-top serves fader-style mixing controls. Value stored raw plus a `SliderRange` with clamp-on-write (or a normalized newtype clamping to [0,1]).
- **Value seam:** app state is authoritative — slider events request changes, app code applies them (the IME app-owned value path). Specify the component/field carrying the current value that the headless slider exposes and presets read for thumb/fill placement.
- **Drag mapping:** map panel-local pointer position to a normalized value, then to the range. During a drag, reproject **per frame** from `Pointer<Drag>.pointer_location.position` (viewport px, present on every pointer event) via `Camera::viewport_to_world` → ray → panel-plane intersection → panel-local map → clamp. `Pointer<Drag>` carries no `HitData`, so `panel_local_from_hit` cannot be reused — add a `panel_local_from_ray(ray, panel, transform)` helper; `Drag.delta` screen pixels are unusable on world panels under perspective.
- **Lifecycle:** grab/release/cancel reuse the Phase 6 emulated-capture machinery (press state component, global Release/Cancel observers, DragEnd terminal, choke-point removal observer); disabled handling per the button rules including cancel-on-disable-while-dragging. Slider change events derive `EntityEvent` targeting the widget entity.
- Optional snapping applied after clamp.

**Files:**
- `src/widgets/slider.rs` — new
- `src/widgets/picking.rs` (or a shared geometry module) — `panel_local_from_ray` helper
- `src/panel/builder.rs` — `.slider(id, spec)`
- `src/widgets/reify.rs` — slider kind reify

**Constraints from prior phases:** Phase 6 built the emulated-capture pattern (`ButtonPress`-style state component, global observers, `On<Remove, …>` choke point) — mirror it, don't re-derive it. Phase 3's rect source gives the slider its panel-local track geometry. Phase 2's `WidgetDisabled` gates input.

**Acceptance gate:** `cargo nextest run` green with new tests: direction/value mapping for all four directions; clamp-on-write; snapping; **drag-beyond-panel-bounds** still tracks and clamps (the reprojection path); cancel paths incl. disable-while-dragging; disabled slider ignores grab.

### Phase 9 — Slider overlay preset  · status: todo

#### Work Order

**Goal:** Default slider visual preset using overlay layout.

**Spec:**
- `widgets/presets/slider.rs`: `SliderPreset` / `SliderStyle` (widget-specific names, material-first slots like ButtonPreset).
- Use `El::overlay()` — track, fill, thumb, and optional labels share one content rectangle and are layered, not arranged. `DrawZIndex` orders thumb above fill above track.
- Thumb/fill placement reads the Phase 8 value seam; restyle and thumb movement via component-level writes on materialized children — no `LayoutTree` regeneration per value change.
- Preset respects `SliderDirection` for fill/thumb placement in all four directions.

**Files:**
- `src/widgets/presets/slider.rs` — new
- `src/widgets/presets/style.rs` — shared helpers only where they remove real duplication
- `src/lib.rs` — re-exports

**Constraints from prior phases:** Phase 8: `SliderDirection`, `SliderRange`, and the value-seam component the preset reads. Phase 7: preset conventions (material-first slots, component-flip restyle, boundary guardrail).

**Acceptance gate:** `cargo nextest run` green with new tests: thumb/fill placement tracks value in all four directions; value change causes no relayout; preset builds under the behavior/preset boundary test.

### Phase 10 — Tooltip behavior + authoring  · status: todo

#### Work Order

**Goal:** Tooltips as normal anchored panels with hover/focus show-hide policy, lazy-spawned on first show.

**Spec:**
- `widgets/tooltip.rs`. A tooltip is a normal `DiegeticPanel` + `Tooltip` + `hana_valence::AnchoredTo` — no separate `TooltipOf`/`TooltipsFor` relationship (`AnchoredTo` already identifies the target; `Tooltip` marks the panel as a tooltip). Ownership split: hana_valence owns placement (`AnchoredTo`, geometry contract, resolver); diegetic owns tooltip behavior (show/hide policy, timers) and geometry provision. Authoring helpers insert `AnchoredTo` internally; hana_valence types stay out of public signatures.
- Public policy type:
  ```rust
  pub struct Tooltip {
      pub show_after: Duration,
      pub hide_after: Duration,
      pub disabled_policy: TooltipDisabledPolicy,
  }

  pub enum TooltipDisabledPolicy {
      Show,
      Suppress,
  }
  ```
  No public `TooltipTrigger` enum, no `TooltipTiming` struct; hover-or-focus behavior is assumed.
- **Runtime state is component-driven and private.** Split the delay timers into `TooltipShowDelay`/`TooltipHideDelay` (or an explicit phase enum) so "waiting to show while already visible" is unrepresentable. Visibility is represented by component presence on the tooltip panel entity, never a `visible: bool` field — name that component explicitly. While hidden: the show timer waits and is removed if hover/focus stops. While visible: the hide timer waits out `hide_after`, and the tooltip hides immediately if hover/focus stops.
- **Residency:** lazy-spawn the tooltip panel on first show — `show_after` masks the spawn+layout latency (only `show_after == 0` risks a one-frame flash). After first spawn, subsequent show/hide transitions toggle `Visibility` (Hidden/Inherited) only — never despawn/respawn per hover (a spawn costs full layout + reify + geometry fill). Lazy spawn also defers the Phase 4 geometry fill to first show (`Added<AnchoredHere>` trigger).
- **Lifecycle:** hide/despawn the tooltip when its `AnchoredTo` target despawns — do not ride valence M16's silent last-transform fallback. Specify the visible→disabled transition per `TooltipDisabledPolicy`. Hover/focus eligibility reads `Hovered` and `WidgetFocused`.
- **Authoring:** widget builders accept a prebuilt tooltip panel or a layout helper — `TooltipPanel::layout(|b| { ... })` or a `.tooltip_layout(|b| { ... })` method; do not promise bare `.tooltip(|b| { ... })` unless a prototype proves closure inference works without annotating the builder parameter. Defaults for `show_after`, `hide_after`, `disabled_policy`, anchors, offset. Standalone tooltip creation uses the same `TooltipPanel` machinery — both entry points visibly lower to `DiegeticPanel + Tooltip + AnchoredTo`; include the standalone snippet in module docs. Rich content (text, dividers, icons, materials, multiple styles) is ordinary panel content; shortcut labels are content supplied by the client, not headless behavior. Overflow avoidance is not a first-pass concept.
- **Placement spaces:** world tooltip on world target → valence 3D placer; screen tooltip on screen target → existing diegetic screen placer (widget targets land in Phase 11). Cross-space (screen tooltip on world target) is out of scope; the later seam is projecting the world anchor to viewport coordinates.

**Files:**
- `src/widgets/tooltip.rs` — new
- `src/panel/builder.rs` / widget spec types — tooltip authoring hooks
- `src/widgets/presets/tooltip.rs` — default tooltip panel presentation
- Read-only: `crates/hana_valence` resolver + `AnchoredTo`, `src/panel/attachment_resolver.rs`

**Constraints from prior phases:** Phase 4 publishes widget `ResolvedAnchorGeometry` lazily on `Added<AnchoredHere>` — tooltip spawn triggers it; the same-frame ordering guarantee (widget geometry before resolve, both in `AnchorSystems::FillGeometry`→`Resolve`) holds. Phase 3's `Hovered` and Phase 5's `WidgetFocused` drive eligibility. Phase 2's `WidgetDisabled` drives `TooltipDisabledPolicy`.

**Acceptance gate:** `cargo nextest run` green with new tests: show-delay timer removed when hover stops before `show_after`; visible tooltip hides immediately on hover/focus stop and after `hide_after`; first show spawns, second show only toggles `Visibility`; target despawn hides/despawns the tooltip; `TooltipDisabledPolicy::Suppress` blocks show on a disabled widget; world-space tooltip anchored to a widget places correctly.

### Phase 11 — Screen-placer widget targets  · status: todo

#### Work Order

**Goal:** Screen-space tooltips and anchored panels can target widgets, not only panels.

**Spec:**
- The screen placer (`src/screen_space/anchoring/`) builds candidate rects solely from panels today and silently drops non-panel targets; valence M4 only re-pointed the panel path. Teach `candidate.rs`/`resolve.rs` to read widget targets: when an `AnchoredTo` target is a widget, derive the candidate rect from the widget's published `ResolvedAnchorGeometry` (projected to viewport space via the existing projection helpers) instead of the panel rect.
- Keep the placer's existing ordering (`.after(PanelChildSystems::Build)`) and fit-on-screen fallback behavior; it stays in diegetic (needs window sizes) and plugs into the valence attachment skeleton.
- Apply the Phase 4 diagnostics convention here too: a screen-space `AnchoredTo` naming a target with no geometry warns once per target-and-reason with the shared dedup key.

**Files:**
- `src/screen_space/anchoring/candidate.rs` — widget-target candidate rects
- `src/screen_space/anchoring/resolve.rs` — target resolution
- `src/screen_space/anchoring/projection.rs` — reuse/extend projection helpers

**Constraints from prior phases:** Phase 4: widgets publish `ResolvedAnchorGeometry` (widget-local frame, composed through `GlobalTransform`) lazily on `AnchoredHere`. Phase 10: screen-space tooltips lower to `DiegeticPanel + Tooltip + AnchoredTo` and currently only place against panel targets.

**Acceptance gate:** `cargo nextest run` green with new tests: screen-space tooltip anchored to a widget places at the widget's viewport rect (not the panel's); panel-target placement is unregressed; missing-geometry warning dedups per target-and-reason.

### Phase 12 — Demonstration checkpoint (stop and discuss)  · status: todo

#### Work Order

**Goal:** Decide, with the project owner, how to demonstrate the widget system. This phase is a discussion checkpoint, not delegated implementation.

**Spec:**
- Stop after Phase 11 and design the demonstration plan together: which existing examples change, which new examples are added.
- The plan must prove the pieces work together in real diegetic UI: buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and existing IME/text input coexisting on one panel.

**Files:** none until the discussion lands.

**Constraints from prior phases:** All widget subsystems (Phases 1–11) complete and tested headlessly (`HeadlessLayoutPlugin`/minimal-app tests) before demonstration work begins.

**Acceptance gate:** A written demonstration plan agreed with the project owner; no code gate.
