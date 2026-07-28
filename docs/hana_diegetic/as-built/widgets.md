# Headless Widgets

> **As built.** Buttons, sliders, editable fields, tooltips, focus, and
> interactivity in `hana_diegetic`. Widgets own semantic behavior and typed
> events; their visuals stay ordinary layout primitives. A widget reifies as a
> panel child entity that Bevy picking can target, and anchoring comes from
> `hana_valence`. Deferred preset/theme design lives in
> [`widgets-deferred.md`](../widgets-deferred.md).

## The shape of the thing

A widget is **semantic identity over retained panel content**. It is not a mesh,
not a render entity, and not a container for its own visuals. The application
authors an ordinary layout tree — fills, borders, text, images, shapes — and
marks one element as a widget. Layout solves that tree exactly as it would
without the widget. Reify then creates one child entity of the panel carrying
the widget's identity, rect, and behavior state.

This split is what lets widget state be free: hover, press, focus, disabled, and
slider value change only retained batch records through widget-owned override
components. None of them regenerates the layout tree, writes `DiegeticPanel` or
`ComputedDiegeticPanel`, or runs a geometry solve.

There is no `bevy_ui` and no `bevy_a11y` dependency. `PickingInteraction` from
`bevy_picking` supplies all-pointer hover/press state; `bevy_enhanced_input`
supplies an optional semantic-action adapter.

## Authoring

`El<L = Row, Role = LayoutOnly>` carries a zero-sized role marker. Ordinary
elements are `LayoutOnly`. Four methods on `El<L, LayoutOnly>` flip the role:

```rust
el.button(id)                       // -> El<L, WidgetElement<Button>>
el.slider(id, range)                // -> El<L, WidgetElement<Slider>>
el.widget(id, widget)               // -> El<L, WidgetElement<W>>  (pre-built Button or Slider)
el.editable_field(id, spec)         // -> El<L, WidgetElement<ImeEditableFieldSpec>>
```

The role marker is generic over the widget type, so the type system knows which
widget an element became. `.widget` is the adoption path for a `Button` or
`Slider` built away from the layout chain; the sealed `Widget` trait implements
it for both. Id and declaration are assigned together — a widget cannot be
authored without its identity.

Widget ids **are** `PanelElementId`; there is no widget-id newtype. Event-emitting
widgets require `Named` ids, because auto ids reposition on structural edits and
would fire spurious cancels. Duplicates reuse the existing
`PanelBuildError::DuplicateElementId` validation.

`Button` and `Slider` are private-field authoring builders, not ECS components.
`Button` is `Clone + Debug + PartialEq + Default` with `new()` and `on_click(...)`.
`Slider` has no `Default` because range and initial value are required.
`SliderRange::new` and `SliderStep::new` reject non-finite, unordered, or
non-positive input with `SliderConfigError`; `Slider::new(range)` itself is
infallible and its authored numbers are validated when the panel builds, so the
declaration chain never returns a `Result`.

The first API rejects **interactive descendants inside a widget** and **widgets
inside precomposed subtrees**. Arbitrary non-interactive child layout is fine.
Nested interaction needs an ownership and hit-order design that does not exist.

### Runtime tree replacement

`DiegeticPanelCommands::set_tree(entity, tree) -> Result<(), PanelBuildError>`
is the one public path. It validates synchronously with the same validator panel
construction uses, and queues the deferred replacement only for a valid tree; a
rejected tree queues nothing and leaves the current tree in place. `Ok(())` means
validation succeeded and the replacement was queued — not that the deferred
command later found a live panel entity. There is no `try_set_tree`.

## State appearance

Every state look is one `Appearance` bundle. `Appearance` is a public fluent
builder over four properties:

```rust
Appearance::new()
    .background(color)      // patches the root SDF fill record
    .border_color(color)    // patches the root SDF border color
    .border_width(width)    // all four sides; grows inward
    .material(handle)       // Handle<StandardMaterial> for fill and border
```

Four verbs on `El<L, WidgetElement<W>>` take one bundle each:

```rust
el.button("apply")
    .hovered(Appearance::new().background(BLUE))
    .focused(Appearance::new().border_color(WHITE).border_width(Px(2.0)))
    .pressed(Appearance::new().background(DARK))
    .disabled(Appearance::new().background(GRAY).border_color(DIM))
```

**A later call replaces the bundle an earlier call authored for that state** — it
does not merge into it.

`pressed` is accepted only for `Button` and `Slider`. At a widget root,
`El::pressed` is gated by `Pressable`; on a widget part, it can be authored but
`WidgetBuilder::with` rejects it for an editable field. An editable field has no
pressed state (it takes a caret and keystrokes; it is not held), so this is a
compile error rather than a silently ignored layer. Hovered, focused, and
disabled reach every widget kind.

`focused` means **the keyboard focus indicator is visible**, not merely that the
widget is still the semantic focus target. A pointer press keeps focus but hides
the indicator.

### What a state layer may touch

A state layer patches records layout already emitted; it never authors a missing
one. `.background(X).disabled(Appearance::new().background(Y))` is not redundant
— the ordinary call is what emits the fill record the state patches. Two escape
hatches exist for a state-only role: `Border::all(Px(0.0), color)` emits a
zero-width border record a focus width can widen, and
`El::new().background(Color::NONE)` emits a transparent fill record.

Authoring a state property with no compatible record is a build error, not a
silent no-op:

| Error | Displays |
| --- | --- |
| `StateBackgroundRequiresBackground` | widget `{0}` state background requires an authored background |
| `StateBorderColorRequiresBorder` | widget `{0}` state border color requires an authored border |
| `StateBorderWidthRequiresBorder` | widget `{0}` state border width requires an authored border |
| `StateMaterialRequiresSurface` | widget `{0}` state material requires an authored background or border |

State builders affect **only the element carrying the widget declaration**. Child
text, icons, images, and shapes stay as authored. A state border width applies to
all four sides and grows inward from the authored outer bounds, so no state
change alters solved layout.

### Composition

Layers apply in `[Focused, Hovered, Pressed, Disabled]` order, **per property**.
A property a later state does not name keeps the earlier state's result, or the
ordinary declaration if no active state named it. This is what lets visible focus
change only the border while hover changes only the fill.

Presentation reads `PickingInteraction`, `Has<WidgetDisabled>`, the private
focus-visible marker, and the private press/drag marker. It never reads or
mutates the private capture resources, which remain lifecycle authority.

## Interactivity

```rust
pub enum WidgetInteractivity { Enabled, Disabled }
```

No `Inherit` variant — inheritance is `Cascade<WidgetInteractivity>`, where
`Cascade::Inherit` continues to the logical parent and an absent ECS component
means non-participation. `WidgetDisabled` is the derived presence marker with a
private field, queried through `Has<WidgetDisabled>` and not constructible by
callers.

**One logical cascade** spans both storage domains. Root-to-leaf precedence:
global default → explicit ECS ancestors / owning panel → parent layout elements →
child layout elements → the widget element. The layout walk folds its
non-entity segment into one `Cascade` value on the computed record; reify
synchronizes that onto the widget entity, whose explicit `CascadeFrom(panel)`
lets `bevy_kana` produce the final `Resolved<WidgetInteractivity>`. The layout
tree stays authoritative — there is no second runtime-authoring layer and no
custom precedence resolver.

A child `Enabled` override inside a disabled parent **is** enabled; ancestor
disabling is not sticky.

Mutation has two entry points. `PanelWidgetWriter::override_interactivity(widget, value)`
and `inherit_interactivity(widget)` edit the authoritative widget element in the
owning panel's tree, returning `false` when the widget, panel, or authored source
cannot be resolved. `CascadeEntityCommandsExt::override_widget_interactivity(value)`
and `inherit_widget_interactivity()` cover panels and other ECS-authored
ancestors. Raw `Cascade<T>` and `Resolved<T>` stay `bevy_kana` machinery and are
not re-exported.

Disabled is visual and behavioral only. Changing content or dimensions requires
an explicit tree edit.

## Identity and lookup

Each widget entity carries `PanelWidget { id }` (exposing `id()`) and
`WidgetOf(panel)` (exposing `panel()`). `PanelWidgets` is the Bevy-maintained
reverse membership set. The relationship is a **traversal index only** — no
`linked_spawn`; widgets sit under `ChildOf(panel)`, which owns despawn.

`PanelWidgetReader` is the read-only bridge from an authored `(panel, id)` to the
live entity, over a private per-panel map rebuilt during reify. It validates that
the mapped entity is still a live `PanelWidget` with the right `WidgetOf` before
returning it, so missing, not-yet-reified, removed, and stale entries all return
`None`. Identical ids on different panels resolve independently.

**Entity events already carry their target and must not re-resolve it.** The
reader is a pre-event bridge for code that starts from `(panel, id)` and needs an
entity to install a scoped observer or issue entity-targeted control.

## Reify and scheduling

Reify is change-gated on `Changed<ComputedDiegeticPanel>`. It reuses entities by
panel-local id, writes components only on diff, rebuilds the id map and current
preorder, and sweeps every unvisited entity. A same-id/same-kind update preserves
live state — applied slider value, active press, callback identity, stable slots,
current overrides. A kind change retains entity identity while replacing the kind
and authored snapshot without leaving stale components.

The ordering that makes a widget correct in its creation frame:

```
CascadeSet::Propagate                      (existing ECS cascade participants)
PanelSystems::ComputeLayout
WidgetSystems::Reify
WidgetSystems::ReifyCommandsApplied        ApplyDeferred — new entities visible
WidgetSystems::ResolveInteractivity        -> WidgetDisabled
WidgetSystems::InteractivityCommandsApplied  ApplyDeferred
WidgetSystems::Focus
WidgetSystems::SemanticInput
WidgetSystems::FocusCommandsApplied        ApplyDeferred
  <state presentation writers>
WidgetSystems::PresentationCommandsApplied ApplyDeferred
  dispatch_visual_overrides
PanelSystems::ResolvePanelAttachments
```

Cascade propagation stays **before** layout because layout consumes
`Resolved<FontUnit>`; reversing them creates a schedule cycle through the
font-unit refresh systems. Semantic widget reify runs in `Update`, not
`PanelChildSystems::Build` — that `PostUpdate` timing is too late for same-frame
screen targets. Render-child batching stays in `PostUpdate`.

Anything consuming newly reified widgets orders after
`ReifyCommandsApplied`, not merely after `Reify`.

## Transform, geometry, and picking

Widgets carry a real panel-local `Transform` whose translation is the widget's
panel-local offset; `GlobalTransform` propagates through `ChildOf(panel)`. This
is deliberately unlike text runs, which carry no `Transform` — copying that shape
would break the picking backend and collapse anchor geometry to the panel origin.

Layout writes the panel-local rect, ancestor-clipped rect, computed preorder, and
interaction rank into the computed record **once**. Picking bounds and anchor
points project that record; nothing recomputes the rect with different
invalidation triggers. Fully clipped widgets are not hit targets. Overlap order
is deterministic: `DrawZIndex`, then source order.

### One diegetic backend

The backend iterates Bevy's `(camera, pointer)` rays, applies the mesh backend's
camera-order / visibility / `RenderLayers` / `Pickable` / render-target filters,
and raycasts only `PanelInteractionMesh` entities. It then rectangle-tests the
`PanelWidgets` of intersected panels and emits the panel and all matching widgets
in **one ordered `PointerHits` group**, so widget depth is actually comparable
with its panel's. Widget hits sit slightly nearer than their panel and order
against one another by interaction rank.

It always reports the owning `DiegeticPanel` entity, never its private mesh
child, so panel background interaction keeps a stable target.

**No per-widget meshes.** Widgets are semantic entities over retained batched
content; their authoritative hit bounds already include clipping and order, and
per-widget meshes would duplicate geometry and need a second synchronization path
for relayout, clipping, and future surface projection.

Hit geometry stays in **panel-local space** through one shared flat projection
helper. Curved panels are gated on `surface-panels.md`, which replaces that one
boundary with `PanelSurface::project()`; widget rectangle tests do not change.

### Per-face control

```rust
pub enum FacePicking { Interactive, PanelOnly, WidgetsOnly, PassThrough }
pub struct PanelPicking { front: FacePicking, back: FacePicking }
```

`Default` is `Interactive` on both faces; an absent component behaves the same.
`PanelOnly` makes the face a panel grab target with no widget response — intended
for back faces. `WidgetsOnly` lets widget rects respond while the panel
background passes through to lower hits. `PassThrough` makes the face invisible
to picking, and a face that emits no hits never blocks. `.picking(...)` on both
world and screen builders sets it.

Identity resolution stays two-sided: front and back rays through the same local
point resolve the same panel and widget identities, and per-face filtering
controls only which of those identities are emitted.

Every panel-spawned render mesh — fill, image, text, SDF batch — carries
`Pickable::IGNORE`, so an application running `MeshPickingPlugin` never has panel
content compete with its own mesh picking. `Pickable::IGNORE` is the stock-picker
control; `PanelPicking` is the diegetic-backend control. They are separate.

**Hit areas are rectangles.** A transparent region inside a panel still catches
clicks; content overhanging the panel edge does not hit-test. `WidgetsOnly` is
the transparent-panel path. Non-rectangular hit regions are out of scope.

## Focus

Focus is per-window and shared, never button-local. One private authority maps
each window to its active panel, focused widget, remembered interaction camera,
and focus-indicator visibility. Markers are derived, never an independent
authority.

`WidgetFocusable` is inserted only when a widget entity is first spawned.
Removing it opts that live widget out of keyboard traversal without changing
pointer picking, and same-id reify, reorder, and authored refresh do not restore
it. `WidgetFocused` is a public read-only presence marker with a private field,
recording the retained keyboard target independently of whether focus should be
drawn; a private focus-visible marker records the latter.

Focus is gained by pointer, traversal, semantic routing, or application request.
It is lost by transfer, despawn, `WidgetFocusable` removal, panel/window
input-scope loss, or explicit clear — **not by disable**. A disabled focusable
widget can hold and receive focus and participates in traversal; behavior modules
ignore its activate and change input.

Traversal follows the **current computed preorder**, never `PanelWidgets`
insertion order. Next/previous/first/last stay within the window's active panel
and wrap deterministically; focusing a widget on another panel transfers the
active panel. Structural reorder changes traversal without respawning entities.
When nothing is focused, the application picks a panel with
`RequestPanelFocus { window, panel }` — the library does not guess between panels
from spawn order.

Automatic pointer focus requires the picked camera to resolve to a real window. A
hit through an image or texture view still interacts with the exact widget but
leaves remembered keyboard focus unchanged.

Public surface: `RequestPanelFocus { window, panel }`,
`RequestWidgetFocus { window, widget }`, `ClearWidgetFocus { window }`, and
`WidgetFocusChanged { window, previous, current, cause }`. Causes are `Pointer`,
`Traversal`, `Semantic`, `Application`, `ExplicitClear`, `WidgetRemoved`,
`FocusabilityRemoved`, `ScopeLost` — disable is deliberately absent.
Indicator-only changes emit nothing, because the semantic target did not change.

## Semantic input

```rust
pub enum WidgetInput {           // each variant carries { window: Entity }
    FocusNext, FocusPrevious, FocusFirst, FocusLast, Activate, Cancel,
}
```

One message queue, so different operations keep their write order. Focus
authority resolves the named window to its focused widget, applies IME and
disabled gating, and emits one private entity-targeted intent that button and
slider behavior consume. Applications and headless tests use this same public
path. Nothing in the core depends on a binding library.

### The optional adapter

`WidgetInputPlugin` translates `bevy_enhanced_input` action edges into those six
messages and nothing else — it never reads or mutates focus authority and never
emits the private intent.

`WidgetInputMode` is a component on each `Window`: `Default` installs Tab /
Shift+Tab / Home / End / Enter or Space / Escape; `Bindings(WidgetInputBindings)`
installs that window's custom controls; `Manual` installs nothing, leaving the
application to drive `WidgetInput` from its own enhanced-input contexts and
`bevy_kana` action macros. Adding the plugin gives every existing and new window
`Default` unless it already specifies a mode. `WidgetInputDisabled` tears down a
window's adapter-owned entities while preserving its mode and remembered focus.

Keyboard and built-in gamepad actions run **only for the operating-system-focused
window**; with no unique focused window, gamepad input emits nothing. Other
windows keep their remembered focus without responding.

`WidgetInputBindings::builder()` returns a fluent builder whose six methods each
take `impl Into<Binding>` and add alternatives, so Enter and Space can both
activate. `build()` deduplicates same-action repeats and rejects `Binding::None`
and cross-action conflicts through `WidgetInputBindingsError`.

Shortcut lowering lives in `bevy_kana::Keybindings::spawn_shortcut`, not in a
widget-owned suppression table: it tracks the unmodified physical edge, requires
declared modifiers with `Chord`, excludes undeclared ones with `BlockBy`, and
applies `Press`. This is what makes releasing Shift from Shift+Tab not manufacture
a fresh Tab press. A modifier key used as a shortcut's primary input never blocks
itself.

`WidgetInputMode::control_summary()` returns a display-only `WidgetControlSummary`
with six `Vec<String>` label fields for help text. It is a derived value — not a
component, resource, or runtime authority — and `Manual` returns empty fields.

## Button behavior

Events: `ButtonPressed`, `ButtonReleased`, `ButtonClicked`, `ButtonCanceled`, all
`EntityEvent` targeting the widget, all carrying `id: PanelElementId` as a
convenience payload. Pressed and released carry `pointer_id: PointerId`; clicked
carries `Option<PointerId>` (`None` for semantic activation); canceled carries
`PointerId` plus a cause. There is no double-click event.

**A pressed button resolves to exactly one terminal path:**

- `Pressed → Released → Clicked` for a valid pointer click
- `Pressed → Released` without `Clicked` for a release that no longer activates
- `Pressed → Canceled` for capture loss, disable-while-pressed, widget removal,
  same-id kind change, panel teardown, pointer cancellation or removal, or
  explicit cancel
- semantic activation emits `ButtonClicked` alone, never entering the pointer
  lifecycle

`ButtonCancelCause` is exactly `PointerCanceled | PointerRemoved | CaptureLost |
Disabled | WidgetRemoved | WidgetKindChanged | Explicit`.

An accepted `set_tree` or interactivity edit that preserves panel, named id, and
kind **does not** cancel an active press merely because snapshots refreshed.
Removal, kind change, disable, and panel teardown are the structural cancellation
edges.

Capture is emulated: a private resource maps each occupied `PointerId` to one
entry holding the widget, id, press sequence, and typed terminal state. A second
press for an occupied pointer, or on an already-captured widget, is ignored. The
terminal choke point sets the terminal state **before** removing the private press
marker; that marker's remove/despawn hook emits the events. A terminal event is
never queued after its target is gone.

Inserting `WidgetDisabled` on a pressed button actively removes the press with a
`Disabled` cause — a flag alone would let the pending release resolve as a click.

Bevy's targeted observers are authoritative for release, click, and drag-end. A
raw-input fallback in `PickingSystems::Last` handles pointer loss, running only
when the picking resources exist so `PickingPlugin` without `InteractionPlugin`
stays valid composition. Because Bevy documents `PointerAction::Cancel` as
terminal, every later raw action for that pointer is warned about and ignored.

Clicking a button calls the IME blur classifier with the owning panel **before**
stopping propagation, so the click commits an editor outside that focus scope and
the panel's double-click field activator cannot open a field underneath the
button.

### `.on_click`

`Button` stores a private cloneable callback template — an `Arc`-owned typed
`SystemHandleTemplate<In<ButtonClicked>, ()>` compared by `Arc` identity, so
widget declarations stay comparable. Reify builds one tracked `SystemHandle` on
the widget. The plugin installs **exactly one** global `ButtonClicked` observer
that reads the target's handle and runs it; reify never installs a per-widget
observer. Reuse never re-registers, callback replacement drops the old strong
handle, and the final drop lets Bevy clean up the registered system.

Application code may equally observe `ButtonClicked` globally or through an
entity-scoped observer and read the widget from the event target. Id alone is
never globally unique.

## Slider behavior

`SliderState` is the public private-field component: validated range, applied
raw-domain value, optional step, direction. `SliderState::new(...)` and
`set_value(...)` reject non-finite input, snap to the lattice anchored at
`range.start()`, then clamp — in that order — with `set_value` reporting whether
the applied value changed.

**The application is the authority on value.** `SliderChangeRequested { id, value,
is_final, pointer_id }` is a *proposal*; the application explicitly accepts it by
calling `set_value`, or ignores it. The exported `slider_self_update` observer is
the opt-in uncontrolled convenience — the plugin never installs it.
`RequestSliderAdjustment { entity, adjustment }` computes and emits a proposal
without applying it, where `SliderAdjustment` is `Absolute(f32) | Relative(f32) |
RelativeSteps(f32)`; `RelativeSteps` emits nothing when the state has no step.

`Slider::initial_value` applies only on first spawn, normalized through the same
snap-then-clamp order. Same-id reuse preserves the live applied value; an
authored range/step/direction change updates configuration and revalidates the
preserved value. Presentation reads only the accepted value, never an unaccepted
proposal.

### Anatomy and travel

`El::slider_thumb()` marks one ordinary descendant of the nearest slider. It
creates no ECS child and exposes no anatomy component — computed output
associates that element's private slot with the owning slider. Zero thumbs leaves
a valid headless slider with no automatic value visualization. An orphan thumb is
`SliderThumbOutsideSlider`; a second is `SliderHasMultipleThumbs`.

The active rectangle is the slider root's **content box**, excluding border and
padding. With content extent `C` and thumb extent `T` on the active axis, usable
travel is `max(C - T, 0)`. Pointer projection follows the **thumb center's actual
path**: left + `T/2` → right − `T/2` for `LeftToRight`, reversed for
`RightToLeft`, top + `T/2` → bottom − `T/2` for `TopToBottom`, reversed for
`BottomToTop`. Values clamp outside that interval, normalize to `[0, 1]`, and map
through the raw range.

When `T >= C` there is no visible travel: pointer projection is unavailable and
emits no proposal, while presentation centers the thumb on the content axis. A
headless slider (no thumb) projects over the full directed content extent; zero
active-axis extent is likewise unavailable.

Presentation solves the desired thumb center from that same endpoint table and
**subtracts the thumb's solved authored center**, so range endpoints are exact
even when the authored tree did not place the thumb at the directed range start.
The delta stays in layout points (Y increasing downward) and converts once —
at one private shared boundary — through the panel's `points_to_world()` scale and
Y inversion before it is written as a retained offset. A missing panel or a
non-finite or non-positive scale clears the offset rather than manufacturing one.

Applications author the track, thumb, labels, decoration, sizes, and `DrawZIndex`
with ordinary trees. `El::overlay()` is the natural arrangement but is not
required. Widgets v1 has no variable-length fill.

### Pointer lifecycle

A press must **project successfully before it captures**. Only then does it claim
occupancy, store the camera and normalized render target, emit `SliderGrabbed`,
and emit one non-final proposal. An initial projection failure claims nothing and
emits nothing. Each drag reprojects and emits one non-final proposal. A valid
release reprojects, frees occupancy, emits one final proposal, then emits
`SliderReleased` — in that order, so a later action in the same raw batch can
claim the freed pointer or widget. A click without movement still reprojects the
release location. Projection loss *after* capture cancels with
`ProjectionUnavailable` and emits no proposal for the failed location.

`SliderCancelCause` is `PointerCanceled | PointerRemoved | CaptureLost | Disabled
| ProjectionUnavailable | WidgetRemoved | WidgetKindChanged | Explicit`.

Capture stores its latest raw projected target independently of `SliderState`, so
rejecting any or every non-final proposal does not alter applied state or the
target release later uses. Capture never snaps, clamps, or writes `SliderState`.

### Shared capture authority

One private authority owns the cross-widget facts: pointer → widget ownership,
widget → pointer ownership, the attempted-press sequence used by raw
reconciliation, and checked exhaustion. **One pointer cannot own two widgets and
one widget cannot be owned by two pointers.** Releasing or canceling frees both
directions before a later raw action in the same unread batch can claim either
side. Button ids, terminal outcomes, causes, and event emission stay in their own
modules.

One private raw dispatcher owns terminal-before-later-press ordering for all
captured widgets and handles button → slider, slider → button, and same-kind
handoff within a single raw batch. There is no public capture API, trait, or
generic terminal payload.

## Retained visual slots

Ordinary fills, borders, images, text, and shape parts stay retained render
records, not ECS children. Layout attaches stable private slot ids to elements;
the computed widget record carries slot-to-record references; widget entities own
changed-only override components. Overrides route through the four existing
record writers (fill, image, text, shape) with common ownership and retirement in
the batch store.

**Re-keying is required.** Material compatibility and image texture are batch-key
facts, so an override changing either removes the record, inserts it into the
compatible destination batch, creates that batch through the ordinary spawn path
when absent, and retires an empty old batch. A moved slot keeps its stable
identity. Every batch created this way goes through the same spawn path and so
carries `Pickable::IGNORE`.

**Interaction boundary.** An override may change appearance or translate a record
in the owning widget's panel-local XY plane. It preserves every record's authored
draw depth and never alters the widget `Transform`, rect, clipped rect, or
interaction rank — so it cannot lift one widget over another or expand its hit
bounds. Fixed part layering is ordinary tree `z_index`; cross-widget depth and hit
bounds require authoritative tree authoring.

Presentation writers **compare immutably before taking mutable access**. Equality
checked inside a setter is too late — a `Mut` borrow has already marked the
component changed. Repeated identical state must leave the change tick untouched
and cause no upload.

## Anchoring to widgets

Anchor geometry publishes **lazily**: only while a widget has attachment demand.
It fills on new demand or rect change and is removed after the final demand ends.
`Changed<Transform>` is never the refill trigger. Geometry points are projections
of the single computed rect expressed in the widget-local frame, matching the
panel provider's centered convention — which is why the widget's own `Transform`
must carry its panel-local offset.

Because ordinary transform propagation runs after valence resolution, and a
widget's owner panel may itself move within that pass, a private internal
`AnchoredTo` bridge from widget to owning panel exists while world demand does.
The widget becomes a real resolver candidate only while demanded, so graph order
resolves an anchored owner panel first, writes the widget's transform second, then
resolves sources targeting that widget. This covers first spawn, parented panels,
same-frame panel motion, and anchored-panel → widget → tooltip chains without
resolving every widget every frame. No valence type enters the public widget API.

Screen attachments use an equivalent private source/target relationship plus a
graph dependency proxy that derives the widget rect from the owning panel's
current resolved screen rect and transform — never from the child widget's
possibly-stale `GlobalTransform`.

### Typed same-space handles

Attachment mutation is same-space **by construction**. `PanelEntity<Space>` and
`WidgetEntity<Space>` are opaque handles with no public unchecked constructor,
minted only after checking the live panel, or for a widget its `WidgetOf` and
owning panel. A `World` source can target only `World` panels and widgets, and
likewise for `Screen`; raw `Entity` is not accepted for attach or retarget, though
it remains available for unrelated ECS work.

Attach, retarget, detach, and world↔screen conversion all queue one complete
operation on the caller's ordinary `Commands`, preserving written order without a
command-wrapper type. Conversion is rejected while the panel has an outgoing
placement, is another panel's target, or owns a targeted widget; the caller
detaches, converts, then reacquires the destination-space handle through
`PanelEntityReader` and reattaches. A rejected live operation changes nothing and
warns. Because ECS identity can outlive a handle, every queued mutation rechecks
that its handle still matches the live panel when it applies.

## Tooltips

Every tooltip is its own entity carrying `Tooltip` and `TooltipFor(target)`;
`Tooltips` is the reverse membership and **does** use `linked_spawn`, so target
despawn owns controller cleanup. The relationship expresses what the tooltip
describes, not how to place it.

`.tooltip(tooltip)` exists only on `El<L, WidgetElement<W>>`, so it is
unavailable before an element becomes a widget — no tooltip-specific panel-build
error is needed. The declaration sits parallel to the widget declaration, never
inside `Button`, `Slider`, or the private widget spec. Standalone authoring is
`TooltipCommandsExt::spawn_tooltip(target, tooltip) -> Entity`, which returns the
controller id but keeps relationship mutation private.

`Tooltip` **is its own visual-layout authoring context**: `Tooltip::new(root)`
creates the `Fit × Fit` tooltip, and `with`, `text`, and `image` mirror the
ordinary closure-based layout operations. All of them accept only
`El<_, LayoutOnly>`, so a button or slider cannot enter a tooltip tree at compile
time — no runtime widget suppression or content error exists. Internally the tree
is copy-on-write `Arc<LayoutTree>`; once cloned or attached, that immutable
blueprint is the deferred panel source.

Defaults installed by `Tooltip::new`: `show_after` 500 ms, `hide_after` zero,
`TooltipDisabledPolicy::Suppress`, source anchor `TopCenter`, target anchor
`BottomCenter`, an eight-pixel downward offset, and
`TooltipPlacementPolicy::KeepVisible`. Consuming builders override each.

```rust
pub enum TooltipDisabledPolicy { Show, Suppress }
pub enum TooltipPlacementPolicy { KeepVisible, Fixed }
```

### Targets

`TooltipTarget` is a trait over **typed handles**, not over `Mesh3d` or any single
component, with a compile-time `World` or `Screen` associated space. Panel and
widget handles implement it; `TooltipTargetEntity<Space>` is the general handle
returned by checked target-authoring commands; applications may implement it for
their own handles. Raw `Entity` does not implement it.

One checked mesh-anchor command takes any live `Mesh3d` entity plus
`MeshFace::{PositiveX, NegativeX, PositiveY, NegativeY, PositiveZ, NegativeZ}` and
returns a typed world target. While demand exists, Hana reads the entity's
**current** `Mesh3d` and `Aabb` and derives the nine rectangular anchor points and
face frame. The application owns keeping `Aabb` correct after mesh changes — Hana
tracks no asset revisions, membership, or bounds generations. Missing components
leave the target pending and hidden with a bounded warning; restoring them
recovers it.

A general target has **no invented panel owner**: entity despawn drives cleanup
through `linked_spawn`, and the private panel-ownership record is written only
when a real panel role exists.

### Materialization and visibility

The controller starts lightweight — no panel, no anchor demand. A private
preparation request inserts a hidden panel on that same entity exactly once,
after which a fence makes the panel and its synchronized space queryable, a
following system queues the checked same-space attachment, and another fence
applies it before the coordinate-specific placement work. A private ready state is
written only after layout, attachment, placement, **and final transform
propagation** all succeed. Pending means not safe to reveal — it is not an extra
motion frame. A missing provider or a current resolver diagnostic leaves the
tooltip hidden rather than placing it at a fallback transform.

Materialization clones the blueprint's content into the owned panel tree without
mutating or replacing the shared `Arc`. Fit sizing and reflow affect only the
materialized panel.

A panel or widget target supplies coordinate space and layout unit; a screen
target also supplies window, camera order, and render layers. A general world
target reads the global panel layout-unit default — there is no tooltip-specific
hard-coded unit. These are placement and presentation facts, not copies of the
target's fill, border, or text styling.

`KeepVisible` tries the authored side first with the smallest along-edge shift
that could keep the tooltip visible, never shifting past the point where it stops
overlapping the target anchor, then the opposite side, then the remaining side
with more room, then the last — each candidate getting the same limited shift, and
equal room broken by a fixed documented tie-break. It keeps an eight-logical-pixel
margin inside every viewport edge. When the natural fit result is wider than the
usable viewport it constrains **only the tooltip panel's outer width** and reruns
layout so wrappable content reflows; it never rewrites explicit sizes inside the
authored tree. If the result still does not fit, the controller stays hidden —
v1 adds neither clipping nor scrolling. `Fixed` uses the authored placement and
natural size even when the result clips, and ignores the margin.

Screen placement evaluates the target panel's window. World placement evaluates
the **remembered interaction camera**: pointer hits supply it, focus authority
retains it beside the active panel, keyboard traversal reuses it, and initial
keyboard-only focus selects the highest-order active camera in that window that
can render the active panel. That camera governs placement only — it does not
promise exclusive rendering.

Visibility is one private state machine — hidden, waiting to show, visible,
waiting to hide — and `Visibility` is derived from it. Entering the first show
wait issues the preparation request. `show_after` starts when hover or visible
keyboard focus begins and cancels on loss; `hide_after` starts on loss, cancels if
eligibility returns, and hides on expiry, with zero meaning immediate. A finished
show timer still waits for readiness. Only waiting entities tick. Pointer focus
alone does not keep a tooltip eligible after hover ends.
`TooltipDisabledPolicy::Suppress` prevents or immediately ends visibility.

`TooltipShown` and `TooltipHidden` are non-propagating entity events emitted
exactly once per real visibility transition. Shown observes a ready transform;
hidden precedes any cleanup, so effects can still query the entity and its target
relation.

Every materialized tooltip carries Hana-owned `PanelPicking::PASS_THROUGH` on both
faces, preserved across hide and show, with no v1 override. It cannot take hover
from its target or block a lower hit, and because its blueprint admits no widget
declarations, nothing inside it can reify, take focus, activate, or capture a
pointer.

Hiding retains the same panel, layout, and attachment — there is no inactivity
eviction. Renderer routes may retire hidden GPU batch rows and rebuild them on
show; that is not a respawn.

### Replacement is recreation

An identical `Tooltip` clone is a no-op that preserves the controller. **Any**
non-identical replacement — content, timing, disabled policy, anchors, offset, or
placement policy — retires the old controller through its ordinary despawn path
and creates a fresh one for the same target. No timer, readiness, placement, or
visibility state transfers, and a visible old controller emits exactly one
`TooltipHidden` while its relationships and transform are still queryable.
Equality compares blueprint pointer identity plus every policy value, which is
what makes an identical clone distinguishable.

There is no retarget or in-place replacement API. Application code replaces a
standalone tooltip by despawning its controller and calling `spawn_tooltip`
again.

An application wanting an unchanged associated tooltip to survive a whole-tree
reconstruction must clone the **same** blueprint; authoring another visually
equivalent tree intentionally requests replacement.

## Panel teardown

`commands.entity(panel).remove::<DiegeticPanel>()` removes the panel **role**
without despawning the entity. Hana finalizes and removes every entity and
retained component recorded as owned by that panel; unrelated application
components on the entity survive. `commands.entity(panel).despawn()` instead
removes the entity and its hierarchy through normal Bevy cleanup. `set_tree` is
not teardown — the role remains installed.

Hana never detaches or reparents application entities. Anything an application
parents beneath a Hana-owned runtime entity follows Bevy's normal recursive-despawn
semantics.

**Two paths, because Bevy queues linked-child despawns from the parent's despawn
hook before deferred commands from remove observers.** Terminal events that need
live widget relationships run from an earlier `On<Despawn, DiegeticPanel>`
observer; component-only role removal runs from the ordinary
`On<Remove, DiegeticPanel>` path. Both are ordered so behavior finalizes while
targets are still queryable:

```
focus finalization
  → button and slider capture finalization
    → tooltip controller finalization
      → combined world/screen anchor cleanup
        → owned-entity despawn
```

Each path cleans exactly once even where ownership records and linked cleanup
overlap.

## Plugins

`HeadlessLayoutPlugin` is layout only, for benchmarks. `HeadlessDiegeticUiPlugin`
composes it with the private widgets and IME plugins — widget, tooltip, focus,
slider, and IME behavior with no shaders, render assets, gizmos, or render
sub-app — so downstream clients can run deterministic tests.
`DiegeticUiPlugin` installs that same behavioral composition before adding text,
screen-space, gizmo, and renderer integration. The private plugins are not
exposed as assembly pieces. `WidgetInputPlugin` is opt-in.

## Invariants

- **Behavior modules never construct layout or render primitives** — no `El`,
  `LayoutTree`, `PanelDraw`, materials, or `TextStyle`. Ordinary tree authoring
  and the widget builders supply the private slots and state values behavior
  presents.
- **No relayout on state change.** Hover, press, focus, disabled, and value flips
  patch retained records only.
- **Change-gated systems, never unconditional per-frame walks.** Reify is gated on
  computed-panel change; interactivity writes its marker only on diff; anchor
  geometry exists only while demand is nonempty. A quiet frame walks nothing.
- **Widget events are `EntityEvent` targeting the widget.** The panel-local id is a
  payload convenience, never the routing key. The owning panel resolves through
  `WidgetOf` and is never duplicated on components or events.
- **Public opaque types, not leaked private ones.** A public trait whose methods
  mention crate-private types trips `private_interfaces`, and E0446 forbids a
  public trait exposing a private associated type; every type reachable from a
  public associated type is public with private fields.
- **Exported errors derive `thiserror::Error`**, declare messages beside their
  variants, and have exhaustive stable-message tests. Converting sources and
  intentionally lossy normalization mappings stay explicit.
- **Deterministic pointer tests** feed synthetic `PointerHits` plus raw
  `PointerInput` through Bevy's real hover and dispatch path. They never move the
  operating-system pointer, and a directly triggered target event is not
  sufficient integration coverage.
- **Component-level state does not prove coordinate conversion.** Anything
  crossing layout, attachment, and final-render frames needs a test at the final
  consumer — a widget-level test passes happily while a layout-space Y delta is
  written into render space with the wrong sign.

## Crate-private

`WidgetsPlugin`, the widget spec and kind enums, computed records, id and order
maps, callback templates and handles, capture and terminal state, visual slot ids
and overrides, anchor bridges and geometry, tooltip phases and timers, and screen
dependency relations. Raw `Cascade<T>` and `Resolved<T>` remain `bevy_kana`
machinery rather than widget API.

## Canonical example

`crates/hana_diegetic/examples/widgets.rs` is the cumulative integration target:
a world Widget Lab panel on a cube with front `Interactive` / back `PanelOnly`
picking and pass-through screen overlays; a typed world readout following the
bottom slider and a distinct typed screen-widget attachment; both the built-in
per-window input adapter and an application-owned `bevy_kana` action driving the
same messages; visible pointer, focus, button, callback, slider, state, and
tooltip diagnostic rows; and associated plus standalone tooltip controllers
including the cube as a mesh-face target.
