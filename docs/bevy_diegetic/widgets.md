# Headless Widgets Adhoc Review

Date: 2026-06-16
Source: High-level headless widget architecture discussion for `bevy_diegetic`.

Related plan: [`surface-panels.md`](./surface-panels.md) maps panels (and the
widgets on them) onto curved parametric surfaces. Widgets stay curvature-blind —
the surface remap is confined to the panel boundary — but two touchpoints here
must be authored surface-ready to avoid re-churn: the anchoring generalization
(Decision 20, Phase 0) and widget picking geometry (Decision 3). Both are
annotated below.

## Ordered Concepts

1. Headless widget boundary
   - Widgets own semantic behavior, state transitions, and typed events.
   - Visuals remain ordinary diegetic layout and render primitives.
   - No new renderer or Bevy UI dependency is introduced for widgets.

2. Widget identity and layout records
   - Add panel-local `WidgetId` values for semantic identity.
   - Reject duplicate widget ids within one panel through the panel builder's `Result<_, PanelBuildError>` path.
   - Treat layout-derived widget records as reification inputs that create/update widget child entities.

3. Widget entities, relationships, and picking
   - Materialize widgets as panel child entities, analogous to panel text runs.
   - Add a `WidgetOf` / `PanelWidgets` relationship index.
   - Emit picking geometry for widget entities so Bevy picking can target widgets directly.

4. Typed widget event contract
   - Emit widget-specific events rather than exposing raw pointer handling.
   - Keep app state authoritative; widget events request changes, app code applies them.
   - Target the materialized widget entity and include the panel-local widget id.
   - Resolve the owning panel through the widget relationship rather than duplicating panel ownership on widget components.
   - Expose semantic activation/change inputs so keyboard shortcuts and `bevy_enhanced_input` can wire into widgets.

5. `ButtonPressed`
   - Fired when an interactive button accepts a press/capture gesture.
   - Targets the widget entity and carries its `WidgetId`.
   - Used for behavior observers that care about press-down timing, not ordinary activation.

6. `ButtonReleased`
   - Fired when a previously pressed button receives a valid release.
   - Targets the widget entity and carries its `WidgetId`.
   - Describes release timing separately from whether the button activates.

7. `ButtonClicked`
   - Fired for the button's primary activation.
   - Covers pointer click and semantic activation from keyboard/action routing.
   - No double-click button event in the first API.

8. `ButtonCanceled`
   - Fired when a press/capture sequence is abandoned before activation.
   - Covers pointer leaving/capture loss, disabled transitions, or explicit cancellation.
   - Lets consumers clean up press-start side effects when no click occurs.

9. Button hover state
   - Track whether pointer hover currently targets the button.
   - Use hover for visual styling and tooltip eligibility.
   - Do not add hover enter/exit events unless review decides consumers need them.

10. Button focus state
   - Track keyboard/action focus separately from pointer hover.
   - Use focus for visual styling, tooltip eligibility, and semantic activation routing.
   - Define how focus is gained, lost, and transferred between widgets.
   - Provide keyboard-only traversal and activation hooks across all widgets.
   - Design with accessibility (`a11y`) and Bevy's accessibility stack in mind.
   - Mirror `bevy_lagrange` input patterns: semantic actions, binding/control summaries, and a thin enhanced-input adapter.

11. Button disabled handling
   - Read resolved `WidgetInteractivity`.
   - Prevent pointer and semantic activation while disabled.
   - Cancel an active press if the button becomes disabled while pressed.
   - Surface disabled as visual state for presets/custom styling.

12. Button pointer activation
   - Use Bevy picking on the materialized widget entity.
   - Emit `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, and `ButtonCanceled` according to the lifecycle invariants.
   - Capture the pointer during press so release/cancel is deterministic.

13. Button semantic activation
   - Expose a non-pointer activation path for keyboard shortcuts and action systems.
   - Align with `bevy_lagrange`-style semantic action and control-summary patterns.
   - Emit `ButtonClicked` without fabricating pointer press/release events.

14. Slider behavior slice
   - Implement grab, drag, value change, release, cancel, disabled, and optional snapping.
   - Map panel-local pointer position to a normalized value, then to the slider range.
   - Use an explicit headless direction such as left-to-right, right-to-left, bottom-to-top, or top-to-bottom.
   - Include bottom-to-top for fader-style controls such as mixing panels.

15. Preset and custom styling boundary
   - Built-in presets are helper builders that generate `LayoutTree` fragments.
   - Users can build their own presets from the same headless state and events.
   - Presets should read widget state and produce normal `El`, `TextStyle`, `PanelDraw`, and `DrawZIndex` output.
   - Preset style slots should be material-first. Plain colors and images are convenience inputs that resolve to `StandardMaterial`; custom animation/shader cases can use custom material handles or `ExtendedMaterial`.
   - Presets should allow richer content such as text, images/icons, custom materials, and animation hooks.
   - Avoid vague shared visual nouns. Prefer widget-specific preset structs such as `ButtonPreset`/`ButtonStyle` and `SliderPreset`/`SliderStyle`, with shared internal helpers only where they remove real duplication.

16. Widget interactivity and disabled resolution
   - Design a concrete `WidgetInteractivity` model.
   - Use enums instead of booleans for interaction state.
   - Split inherited/specification from the concrete enabled/disabled state.
   - Resolve global, panel, layout subtree, and individual-widget disabled/interactive overrides.
   - Do not carry a disabled reason in the first API.
   - Make disabled changes visual-only by default, with no layout recompute unless a preset opts into different content or dimensions.

17. Tooltips
   - Treat tooltips as first-class headless widget affordances.
   - Model a tooltip as a normal `DiegeticPanel`/panel template that is shown for a target, not as a separate placement/layout primitive.
   - Do not add a separate `TooltipOf` / `TooltipsFor` relationship in the first API. `AnchoredTo` already identifies the target entity, and `Tooltip` identifies the anchored panel as a tooltip.
   - Generalize the existing panel-specific anchoring layer (`AnchoredToPanel`, `PanelsAnchoredHere`, `PanelAnchorGeometryParam`) so a tooltip panel can anchor to any target entity that exposes anchor geometry, not only another panel.
   - For panel targets, attach `Tooltip` and generic `AnchoredTo` to the tooltip panel.
   - For widget targets, reuse the same `Anchor` and `AnchorOffset` vocabulary against materialized widget geometry through the generic anchoring resolver.
   - Do not require full recursive panel-in-panel composition for first-pass tooltips. Treat embedded/nested panels as a plausible follow-up where a panel-backed element is measured like an element and exposes anchor geometry.
   - Concrete tooltip anchoring path:
     - A tooltip is a normal `DiegeticPanel` entity.
     - The tooltip panel carries `Tooltip` for show/hide policy.
     - The tooltip panel carries `AnchoredTo` for both target association and spatial placement.
     - `AnchorGeometryParam::get(target)` resolves target anchor geometry from either a panel or materialized widget.
     - The generic anchoring resolver places the tooltip panel by pinning its source anchor to the target anchor plus `AnchorOffset`.
   - Do not bake shortcut/keybinding semantics into the headless tooltip API. Shortcut labels are normal tooltip panel content supplied by the client or a later helper.
   - Keep the first tooltip type surface small: `Tooltip` should assume hover-or-focus behavior and carry only show/hide delays plus `TooltipDisabledPolicy`.
   - Do not add a public `TooltipTrigger` enum or separate `TooltipTiming` struct in the first API. Special trigger policies can be added later with extra marker components or helper systems if real use cases appear.
   - Keep tooltip runtime state private and minimal. Use a private `TooltipTimer` component for delayed show/hide timing; visibility should be represented by component presence on the tooltip panel entity, not by a `visible: bool` field.
   - Tooltip timer behavior: while hidden, `TooltipTimer` waits to show and is removed if the target stops hovering/focusing; while visible, `TooltipTimer` waits to hide after the configured duration, and the tooltip is also hidden immediately if the target stops hovering/focusing.
   - Provide authoring helpers that build the same runtime components. Widget builders should accept either a prebuilt tooltip panel or a tooltip layout closure, with defaults for `show_after`, `hide_after`, `disabled_policy`, anchors, and offset.
   - Do not promise a bare `.tooltip(|b| { ... })` API unless a prototype proves closure inference works without annotating the builder parameter. The fallback ergonomic shape is `.tooltip(TooltipPanel::layout(|b| { ... }))` or a separate `.tooltip_layout(|b| { ... })` method.
   - Reuse the same tooltip builder machinery for standalone tooltip creation and widget-attached tooltip authoring, so presets and custom widgets both end in `DiegeticPanel + Tooltip + AnchoredTo`.
   - Support rich tooltip panels: text, dividers, secondary metadata, images/icons, custom materials, and multiple text styles.
   - Keep overflow avoidance as optional presentation helper behavior, not as a first-pass headless `fit` concept. If added, it should adjust anchors/offsets within the existing anchor model.
   - Support tooltips authored in layout, plus world-space and screen-space tooltip panels.

18. Slider overlay preset
   - Use `El::overlay()` for the default slider preset.
   - Layer track, fill, thumb, and optional labels in one shared content rectangle.
   - Use `DrawZIndex` for thumb-over-track ordering.

19. Module structure and exports
   - Add a private `widgets` module next to `ime`.
   - Keep widget headless APIs in their own modules: `button.rs`, `slider.rs`, and tooltip modules.
   - Keep relationship, reification, interactivity, picking geometry, and preset support as shared private modules.
   - Re-export curated public types from `lib.rs`, not the whole implementation module.

20. Rollout and tests
    - Phase 0: Rename and generalize anchoring before widget-specific work.
      - Rename the panel-specific anchoring relationship names to generic names, such as `AnchoredToPanel` -> `AnchoredTo` and `PanelsAnchoredHere` -> `AnchoredHere`.
      - Rename anchor geometry access from panel-specific names to generic names, such as `PanelAnchorGeometryParam` -> `AnchorGeometryParam`.
      - Rename other public anchor geometry types only where they become generic in the same pass, such as `PanelAnchorOffset` -> `AnchorOffset`, while keeping behavior unchanged.
      - Preserve the current panel-to-panel behavior while making the target side generic enough to resolve panel and widget anchor geometry.
      - Rename entity-creation-from-computed-output terminology from `reconcile`/`materialize` to `reify` where the concept is shared. Existing panel text `reconcile_panel_text_children` can become `reify_text_entities`; widget entity creation can live in `widgets/reify.rs`.
    - Phase 1: Build widget identity, widget relationships, reification, and picking geometry.
    - Phase 2: Add button behavior and a simple preset.
    - Phase 3: Add slider behavior and the overlay preset.
    - Phase 4: Add tooltip behavior and one default tooltip panel presentation.
    - Phase 5: Stop and review how to demonstrate the widget system.
      - Decide with the project owner which existing examples should change and which new examples should be added.
      - Use the demonstration plan to prove widgets, tooltips, focus, disabled state, sliders, buttons, panel ordering, and existing IME/text input can coexist.
    - Cover headless behavior with `HeadlessLayoutPlugin`/minimal app tests and examples.

## Decisions

Decisions from the adhoc review will be recorded here.

### 1. Headless widget boundary

Decision: Keep.

Widgets own semantic behavior, state transitions, and typed events. Visuals stay ordinary diegetic layout and render primitives. Enforce this by keeping widget behavior modules independent from preset/layout modules: behavior systems may read computed widget records and emit events, but they should not construct `El`, `LayoutTree`, `PanelDraw`, materials, text styles, or render commands.

Feedback:

- Use Bevy's built-in picking for widget and panel pointer routing.
- Keep each widget's public headless API in its widget module, such as `widgets/button.rs` or `widgets/slider.rs`.
- Keep presets as optional pre-built layout helpers. Users must be able to attach a headless widget marker to custom layout and style it themselves.
- Revisit naming for the `El` builder helper. `El::button(Button::new(...))` may be too redundant if common usage can be `El::button(...)` or `El::button_id(...)`.
- Materialize widgets as panel child entities, analogous to panel text runs, so Bevy picking can target widget entities directly.
- Use a relationship index for panel widgets, analogous to `TextRunOf` / `PanelTextRuns`.
- Treat layout-derived widget records as reification inputs, not as the long-lived runtime API.
- Add keyboard shortcut and action routing hooks. The core widget API should expose semantic activation/change events that can be wired from `bevy_enhanced_input`.
- Disabled/enabled is likely cascade-like: global widgets, panel widgets, layout subtree, and individual widget overrides all need a coherent resolution rule.
- `WidgetId` should be panel-local by default, matching panel element ids. Public events should target the widget entity and carry `WidgetId`; panel ownership comes from the `WidgetOf` relationship.
- Widget picking should use Bevy picking on materialized widget entities. The picking geometry may start as the element bounds and later support shape-aware hit tests for rounded or custom shapes.
- Disabled/interactivity should be treated as mostly visual-only. Changing disabled state should not normally force layout unless a preset explicitly chooses to render different content or dimensions.
- Tooltips are required as a first-class widget affordance, not an afterthought.
- Duplicate widget ids within one panel should be rejected, matching the existing duplicate element id build-time contract.
- Initial widget scope is IME integration, buttons, sliders, and tooltips.
- Tooltip content should allow optional shortcut display, without forcing `bevy_enhanced_input` as the only shortcut source.
- Tooltip presentation must support layout-authored tooltips and spawned world/screen tooltip panels with correct ordering.
- The shortcut/action design should follow the shape of `bevy_lagrange`: semantic action types, binding descriptions/control summaries, and a thin enhanced-input adapter rather than raw key handling embedded in widgets.
- Panel-in-panel/subpanel composition is out of scope for this widget plan. Basic headless widgets should use element-tree authoring plus materialized widget entities.

Open questions:

- Define panel dragging support as part of the library surface. A diegetic panel may contain draggable controls, but the app may also want to select and drag the whole panel.
- Decide event precedence between panel dragging and child widget gestures. Candidate policy: child draggable widgets capture first, while panel dragging starts only from panel background or from explicitly marked drag regions.
- Decide whether fallback panel drag from a button press that becomes a drag is allowed, configurable, or explicitly disabled.
- Decide whether disabled/enabled uses the existing entity cascade, a layout-tree propagation pass, or a hybrid. Element-subtree disable implies layout-tree propagation unless layout elements become an entity hierarchy.
- Define how tooltip authoring works for custom-styled headless widgets and preset widgets, and whether tooltip contents are plain text, a panel builder callback, or both.
- Define the exact `WidgetInteractivity` type surface and override/resolution rules.

### 2. Widget identity and layout records

Decision: Keep.

Use panel-local `WidgetId`s for stable semantic identity. Reject duplicate widget ids within one panel through `PanelBuildError`, matching the existing duplicate element id build-time contract. Treat layout-derived widget records as reification inputs only; runtime interaction happens through materialized widget child entities.

### 3. Widget entities, relationships, and picking

Decision: Keep.

Materialize widgets as child entities under their panel. Add a typed widget relationship index, analogous to `TextRunOf` / `PanelTextRuns`, so a panel can enumerate its widgets without scanning all children. Emit widget picking geometry on those widget entities, starting with resolved bounds and allowing custom shapes for compound widgets such as sliders.

Surface-panels coordination: keep widget picking geometry in **panel-local
space**. The panel's single `PanelSurface::project()` (see `surface-panels.md`)
converts a world-space pointer hit to panel-local coordinates at the panel
boundary, before any widget bounds test, so widget picking needs no curvature
logic and works unchanged on curved panels. Do not place widget picking meshes
independently in world space.

### 4. Typed widget event contract

Decision: Keep.

Widgets emit semantic events, not raw pointer events. Events target the materialized widget entity and carry the panel-local `WidgetId`; owning panel context is resolved through the `WidgetOf` relationship instead of duplicated on widget state or events. Widgets also expose semantic activation/change inputs so pointer picking, keyboard shortcuts, and enhanced-input actions can drive the same behavior path.

### 5. `ButtonPressed`

Decision: Keep.

Fire `ButtonPressed` when an interactive button accepts a press/capture gesture. This is a press-down event, not activation. It targets the widget entity and carries the panel-local `WidgetId`.

### 6. `ButtonReleased`

Decision: Keep.

Fire `ButtonReleased` when a previously pressed button receives a release. This is an interaction lifecycle event, not activation. A normal pointer click emits `ButtonPressed`, then `ButtonReleased`, then `ButtonClicked`. A release can happen without a click when activation is no longer valid, and keyboard/action activation can emit `ButtonClicked` without a pointer release.

### 7. `ButtonClicked`

Decision: Keep.

Fire `ButtonClicked` for the button's primary activation. Pointer clicks emit it after a valid press/release sequence. Keyboard shortcuts and enhanced-input semantic activation may emit it directly without pointer press/release. Do not add a double-click button event in the first API.

### 8. `ButtonCanceled`

Decision: Keep.

Fire `ButtonCanceled` when a press/capture sequence is abandoned before activation. The implementation must explicitly cover pointer capture loss, a button becoming disabled while pressed, widget despawn, panel/tree replacement removing the widget, and explicit cancellation. A pressed button must always resolve to exactly one terminal lifecycle path: `ButtonReleased` with optional `ButtonClicked`, or `ButtonCanceled`.

Implementation invariant:

- `Pressed -> Released -> Clicked` for a valid pointer click.
- `Pressed -> Released` without `Clicked` for a valid release that no longer activates.
- `Pressed -> Canceled` for capture loss, disable-while-pressed, despawn/removal, or explicit cancel.
- Semantic keyboard/action activation can emit `ButtonClicked` without entering the pointer press lifecycle.

### 9. Button hover state

Decision: Keep.

Track hover state from Bevy picking on the materialized widget entity. Hover feeds visual styling and tooltip eligibility. Hover enter/exit events are not part of the first public API unless a later review promotes them.

### 10. Button focus state

Decision: Keep.

Focus is a shared widget focus/navigation/a11y system, not button-local behavior. Track focused widget entity separately from hover. Focus can be gained by pointer focus, keyboard traversal, semantic action routing, or app request; lost by transfer, disable, despawn/removal, panel/window input-scope loss, or explicit clear. Provide semantic focus actions such as next, previous, first, last, activate focused, and cancel focused. Mirror `bevy_lagrange` by exposing semantic actions and neutral control summaries, with a thin enhanced-input adapter.

Add a focus participation component, likely `WidgetFocusable`, to materialized widget entities by default. Removing it opts a widget out of keyboard focus traversal without changing pointer picking. Disabled state should not remove focusability by itself; disabled widgets may retain or receive focus, but widget behavior must ignore activation/change input while disabled.

### 11. Button disabled handling

Decision: Keep.

Buttons read resolved `WidgetInteractivity`. Disabled buttons ignore pointer and semantic activation, cannot keep focus/capture, and cancel an active press if they become disabled while pressed. Disabled state is exposed for preset and custom styling, and disabled changes should be visual-only by default.

### 12. Button pointer activation

Decision: Keep.

Use Bevy picking on the materialized button entity. Pointer press captures the pointer and emits `ButtonPressed`. Release/cancel follows the button lifecycle invariants: valid pointer clicks emit `ButtonReleased` then `ButtonClicked`; non-activating releases emit `ButtonReleased` only; capture loss, disable-while-pressed, despawn/removal, and explicit cancel emit `ButtonCanceled`.

### 13. Button semantic activation

Decision: Keep.

Expose a non-pointer activation path for keyboard shortcuts, action systems, and `bevy_enhanced_input`. Semantic activation can target the focused button or an explicitly targeted button. It emits `ButtonClicked` without fabricating pointer press/release events.

### 14. Slider behavior slice

Decision: Keep.

Headless slider behavior includes grab, drag, value change, release, cancel, disabled handling, optional snapping, and mapping panel-local pointer position to a normalized value. The headless slider owns an explicit direction, such as left-to-right, right-to-left, bottom-to-top, or top-to-bottom; bottom-to-top supports fader-style controls such as mixing panels.

### 15. Preset and custom styling boundary

Decision: Modify.

Use widget-specific preset and style names, such as `ButtonPreset`, `ButtonStyle`, `SliderPreset`, and `SliderStyle`. Avoid vague shared visual nouns such as `WidgetSurface`, `WidgetMaterial`, `WidgetPartStyle`, or `Paint` in the public preset API. The built-in presets should be material-first: plain color and image inputs are convenience APIs that resolve to `StandardMaterial`, while custom shader or animated cases can use custom material handles or `ExtendedMaterial` through custom presets or later generic extension points.

Presets remain layout/render helpers, not headless behavior. They read headless widget state and generate normal diegetic layout/render primitives. Users can skip presets entirely and style a headless widget with their own element tree and material systems.

### 16. Widget interactivity and disabled resolution

Decision: Modify.

Use enum-based authoring for inherited/default/overridden interactivity, then materialize the resolved state as a Bevy-style marker component on the widget entity.

Authoring shape:

```rust
pub enum WidgetInteractivity {
    Inherited,
    Set(InteractionState),
}

pub enum InteractionState {
    Enabled,
    Disabled,
}
```

Runtime shape:

```rust
pub struct WidgetDisabled;
```

The resolver applies global, panel, layout-subtree, and widget-level values, then inserts or removes `WidgetDisabled` on the materialized widget entity. Widget behavior and styling systems query `Has<WidgetDisabled>`, mirroring Bevy's `InteractionDisabled` pattern. Do not expose a `ResolvedWidgetInteractivity`, do not use `enabled: bool` as the authoring API, and do not carry a disabled reason in the first API.

Disabled changes should remain visual/state-only by default. A disabled widget must ignore pointer and semantic activation, lose focus/capture if needed, and cancel any active press/drag lifecycle according to the widget's event contract.

### 17. Tooltips

Decision: Modify.

A tooltip is a normal `DiegeticPanel` with tooltip behavior and generic anchoring. The first API should not add a separate `TooltipOf` / `TooltipsFor` relationship because `AnchoredTo` already identifies the target entity, and the `Tooltip` component identifies the anchored panel as a tooltip.

Runtime shape:

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

The tooltip panel also carries `AnchoredTo::new(target, tooltip_anchor, target_anchor).with_offset(offset)`. `AnchorGeometryParam::get(target)` must resolve anchor geometry from either a panel or a materialized widget entity.

Keep runtime state component-driven. Use private `TooltipTimer` for delayed show/hide timing. Visibility is represented by component presence on the tooltip panel entity, not by a `visible: bool` field. While hidden, the timer waits to show and is removed if hover/focus stops. While visible, the timer waits to hide after `hide_after`, and the tooltip hides immediately if hover/focus stops.

Shortcut/keybinding display is normal tooltip panel content supplied by the user or a later helper; it is not headless tooltip behavior.

Authoring helpers should all lower to the same runtime shape: `DiegeticPanel + Tooltip + AnchoredTo`. Widget builders should accept a prebuilt `TooltipPanel`, and support a named layout helper such as `TooltipPanel::layout(|b| { ... })`. Do not promise bare `.tooltip(|b| { ... })` unless a prototype proves it works without closure parameter annotations.

### 18. Slider overlay preset

Decision: Keep.

Use `El::overlay()` for the default slider preset. Track, fill, thumb, and optional labels share one content rectangle and are layered rather than arranged. Use `DrawZIndex` so thumb renders above fill and fill renders above track. This preserves overlay layout mode as the standard way to build slider visuals.

### 19. Module structure and exports

Decision: Keep.

Use a cohesive `widgets` module with public headless widget APIs in widget-named files and shared support in narrowly named cross-widget modules.

Proposed shape:

```text
crates/bevy_diegetic/src/widgets/
  mod.rs
  button.rs
  slider.rs
  tooltip.rs
  id.rs
  relationship.rs
  interactivity.rs
  focus.rs
  picking.rs
  reify.rs
  presets/
    mod.rs
    button.rs
    slider.rs
    tooltip.rs
    style.rs
```

`button.rs` owns the headless button component, state transitions, typed events, pointer behavior, disabled handling, and semantic activation path. `slider.rs` does the same for sliders. `tooltip.rs` owns the `Tooltip` policy component, `TooltipDisabledPolicy`, private `TooltipTimer`, and tooltip show/hide systems.

Shared modules should stay narrow:

- `id.rs`: `WidgetId` and duplicate-id validation helpers.
- `relationship.rs`: `WidgetOf` and `PanelWidgets`.
- `interactivity.rs`: `WidgetInteractivity`, `InteractionState`, and runtime `WidgetDisabled`.
- `focus.rs`: `WidgetFocusable`, focus traversal, and focus actions.
- `picking.rs`: picking geometry emitted for materialized widget entities.
- `reify.rs`: turns computed widget records into concrete widget entities.

Presets live under `widgets/presets/` and are optional layout/render builders. They may generate `El`, text, materials, and draw ordering, but headless widget modules should not depend on presets.

Re-export curated public types from `lib.rs` or `widgets/mod.rs`. Do not expose the whole implementation tree as the public API.

### 20. Rollout and tests

Decision: Modify.

Keep the rollout phased, but make Phase 5 an explicit stop-and-discuss checkpoint rather than an implementation phase. After the core widget system exists, pause and decide how to demonstrate it. That discussion should cover which existing examples should be changed and which new examples should be created.

Phase outline:

- Phase 0: Rename and generalize anchoring, and rename shared entity-creation terminology to `reify`.
  - Surface-panels coordination: the generalized `AnchorGeometryParam` should
    return an **oriented frame** (point + tangent frame, matching
    `SurfaceSample` in `surface-panels.md`), not the concrete flat `PanelPlane`.
    That result covers screen bounds (2D), the flat plane (constant frame), and a
    curved-surface sampler, so `surface-panels.md` Phase 5 reuses this layer
    instead of rewriting it.
- Phase 1: Build widget identity, widget relationships, reification, and picking geometry.
- Phase 2: Add button behavior and a simple preset.
- Phase 3: Add slider behavior and the overlay preset.
- Phase 4: Add tooltip behavior and one default tooltip panel presentation.
- Phase 5: Stop and design the demonstration plan with the project owner.

The demonstration plan should prove the pieces work together in real diegetic UI examples: buttons, sliders, tooltips, focus, disabled state, panel ordering, and existing IME/text input. It will likely include both changes to existing examples and new examples, but those choices should be made at the Phase 5 review point.

Tests should protect the headless contracts before the demonstration work: duplicate widget ids, button press/release/click/cancel paths, disable-while-pressed, despawn/removal cancellation, semantic activation without fabricated pointer events, slider direction/value mapping, tooltip timer behavior, and generic anchoring to both panels and materialized widgets.
