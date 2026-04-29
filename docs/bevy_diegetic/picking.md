# Panel picking — design proposal

## Problem

`bevy_diegetic` spawns and rebuilds many `Mesh3d` entities per panel
(RTT display quad, MSDF text glyphs, panel-geometry rectangles, lights).
Bevy's `mesh_picking` treats every `Mesh3d` as a pick target by default,
so consumers face two failure modes when placing a panel in a 3D scene:

1. **Picking competition.** Internal panel meshes intercept picks
   intended for the underlying movable object the panel is mounted on
   (e.g., a glassy backdrop plane that owns the `Movable` semantics).
   Consumers have no clean way to make panel meshes "transparent to
   picking" without walking the spawned mesh tree themselves.
2. **Picking churn.** Pick targets change every layout rebuild. A
   `Pointer<DragStart>` lands on the panel display quad; the next
   `Pointer<Drag>` tick lands on a *different* quad after the layout
   re-spawns. Confirmed-drag state (`DragIntent`, `DragActive`) lives on
   entities that get despawned, observers re-fire `late arrival`
   continuously, and `commands.entity(target).insert(...)` panics on
   stale entity IDs (current bevy_picking error policy).

Either consumers can't drag the underlying object, or they can't
reliably interact with the panel itself, depending on which case bites.

### Concrete failure observed in hana

This proposal is motivated by an actual failure in the
[hana](https://github.com/hanallc/hana) editor. hana mounts a status
display on a flat `StatusPlane` rectangle (the `Movable` backdrop) and
attaches a `DiegeticPanel` as a child for the visible content. The
panel's `LayoutTree` is rebuilt via `panel.set_tree(...)` every time the
displayed data changes — which happens on every diagnostics tick (~500
ms for FPS / frame time) plus whenever camera radius or movables count
changes. During a multi-second drag, the panel rebuilds many times. Each
rebuild despawns and respawns the display-quad `Mesh3d`.

Because the display quad sits in front of the backdrop, picking hits it
first. The drag chain fires on the quad rather than the `StatusPlane`
that owns `Movable`. Then the rebuild despawns the quad mid-drag and the
next `Pointer<Drag>` tick lands on either the freshly-spawned quad (no
`DragIntent`) or the stale ID, taking the late-arrival branch in
`hana::selection::drag_threshold::on_drag`. The deferred
`commands.entity(target).insert(DragActive)` then flushes against an
entity that has been recycled.

Symptom log (panic):

```
2026-04-22T01:52:35.853122Z  INFO hana::selection::drag_threshold:
  DRAG_THRESHOLD: late arrival confirmed on 520v55, start=(1784, 626)

thread 'main' panicked at bevy_ecs-0.18.1/src/error/handler.rs:125:1:
  Encountered an error in command
  `bevy_ecs::system::commands::entity_command::insert<
     hana::selection::drag_threshold::DragActive
  >::{{closure}} ...`:
  Entity despawned: The entity with ID 520v55 is invalid; its index now
  has generation 56.
```

Note generation `v55` → `v56` — the entity index was recycled between
the observer firing and the deferred command flushing.

Symptom log (no panic, just unresponsive — same root cause, after hana
silenced the panic via `try_insert`):

```
DRAG_THRESHOLD: late arrival confirmed on 331v29, start=(1413, 487)
DRAG_THRESHOLD: late arrival confirmed on 331v29, start=(1409, 487)
DRAG_THRESHOLD: late arrival confirmed on 331v29, start=(1405, 486)
... [dozens per second, drag goes nowhere] ...
```

The panel is undraggable because `DragActive` never sticks: every
attempt fires against an entity that is despawned by the next layout
rebuild. The drag-ownership systems watching the `StatusPlane` (the
actual `Movable`) never observe state changes.

hana relevant code:
- `crates/hana/src/status_cube/systems.rs::attach_status_panel` — spawns
  the `DiegeticPanel` as a child of `StatusPlane`.
- `crates/hana/src/status_cube/systems.rs::refresh_status_panel` — calls
  `panel.set_tree(...)` on every `StatusLabelColumns` change.
- `crates/hana/src/selection/drag_threshold.rs` — patched with
  `try_insert` / `try_remove` to silence the panic, but the panel still
  cannot be dragged because picks route to the churning quad instead of
  the `StatusPlane` backdrop.

### What this proposal fixes

`Picking::None` on the panel root makes every internal panel mesh
transparent to picking. With the proposal in place, hana's status
panel can simply not opt into picking — picks fall straight through
the display quad to the `StatusPlane` backdrop, the `Movable` system
sees a stable target, drag works, and the rebuild churn is irrelevant
because no drag state is ever placed on a panel-internal entity.

The despawn-safety patches in hana's `drag_threshold.rs` remain useful
as a general defense (other systems despawn entities mid-drag too —
e.g., `selection/entity/selection_bounds.rs::update_selection_aabb`
despawns the `Selection` entity when the selected set goes empty). But
those are belt-and-suspenders; the picking-side fix removes the most
frequent producer of the race.

## Goals

- One conceptual property that controls both "is this region pickable?"
  and "what kind of picking surface (front/back/both)?"
- Cascade-resolved through the layout tree, mirroring the existing
  `CascadeSet` / `Resolved<>` pattern used for fonts, units, and alpha
  modes.
- Proxy count stays proportional to **picking regions**, not layout
  elements — no per-leaf churn during `set_tree(...)` rebuilds.
- Internal panel render meshes always non-pickable, in every mode, so
  they never compete or churn.
- Backwards-compatible default for existing examples that rely on
  panel-level picking via `.observe()`.

## Public API

One enum, one method on `El`, one convenience method:

```rust
/// How an element participates in mouse picking. Cascades through the
/// layout tree: any descendant inherits this value unless it sets its
/// own. Resolved via the same `CascadeSet` machinery used for fonts,
/// units, and alpha modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum Picking {
    /// Not a pick target. Picks pass through. Default at the root
    /// when nothing higher cascaded a value.
    #[default]
    None,
    /// Pick proxy at this element's bounds, front face only (local
    /// +Z side).
    Front,
    /// Back face only.
    Back,
    /// Both faces — useful for panels freely re-oriented in 3D.
    Both,
}

impl El {
    /// Sets picking for this element. Cascades to descendants. When the
    /// element is the top of a non-`None` cascaded region (parent's
    /// resolved value differs), the panel spawns one invisible pick
    /// proxy sized to this element's computed bounds.
    pub fn picking(self, p: Picking) -> Self;

    /// Convenience: marks this element as a per-element pick target with
    /// a marker component attached to the proxy. Implicitly opts in;
    /// mode resolves from the cascade (defaulting to `Front` if nothing
    /// higher set a value). The marker is cloned and re-attached on
    /// every layout rebuild.
    pub fn picking_with<C: Component + Clone>(self, marker: C) -> Self;
}
```

No separate builder method on `DiegeticPanel`. The panel root is just
the topmost `El` — set picking on it like any other element.

## Semantics

### Cascade resolution

Same as other cascade properties. Each element resolves to either its
own explicit value or the closest ancestor's value. Default at the
implicit "outside the panel" level is `Picking::None`.

### Proxy spawn rule

Spawn a pick proxy at element `E` if either:

1. **Top-of-region**: `E.resolved_picking != None` AND
   `parent.resolved_picking == None` (or `E` is the root). This is
   what makes `picking(Front)` on the root produce *one* panel-bounds
   proxy instead of one proxy per descendant.
2. **Marker**: `E` has a marker attached via `picking_with(...)`. The
   proxy carries that marker. Mode = explicit value if set on `E`, else
   the resolved cascade value, else `Front`.

The proxy is always a direct child of the panel entity (not the visual
parent in the layout tree), so picks bubble through the panel's parent
chain regardless of layout nesting.

### Per-element marker proxies stack in front

Marker proxies are positioned slightly in front of the top-of-region
proxy along local +Z, so they win the hit on overlap. Both bubble to
the panel entity, so the consumer can either:

- Observe at the panel and dispatch on the marker query (`Query<&MyBtn>`),
  or
- Observe directly on the proxy entity (advanced; less ergonomic).

### Mode handling

| Mode | Proxy material/orientation |
|---|---|
| `None` | No proxy spawned. |
| `Front` | Standard quad. Mesh-picking back-face culling means only +Z hits register. |
| `Back` | Quad rotated 180° around local Y. Front face becomes the back side. |
| `Both` | Quad with `RayCastBackfaces` so both faces register. |

All proxies use a fully-transparent material so they're invisible.
`Pickable::default()` (blocks lower picks; hoverable).

### Internal render meshes

Every `Mesh3d` the panel infrastructure spawns (RTT display quad,
MSDF text glyphs, panel-geometry rectangles) is tagged with
`Pickable { should_block_lower: false, is_hoverable: false }` —
unconditionally, in every `Picking` mode. They never participate in
picking, so they don't compete and don't churn.

## Worked examples

### Pure decoration

```rust
// Don't touch picking. Default cascade is None.
DiegeticPanel::world().with_tree(tree).build()
```

No proxies spawned. Picks pass through to whatever's behind the panel.

### Drag the whole panel

```rust
let root = El::new()
    .picking(Picking::Front)
    .width(...).height(...);
LayoutBuilder::with_root(root).with(...).build()
```

One proxy at panel bounds. `Pointer<DragStart>` on the panel area
bubbles to the panel entity.

### Buttons + drag elsewhere

```rust
let root = El::new()
    .picking(Picking::Front)             // panel drag
    .width(...).height(...);

LayoutBuilder::with_root(root).with(..., |b| {
    b.with(El::new().picking_with(SaveBtn), |b| { b.text("SAVE", style); });
    b.with(El::new().picking_with(ResetBtn), |b| { b.text("RESET", style); });
});
```

Three proxies: one panel-bounds (root), one per button. Buttons win on
overlap (z-stacked in front). Consumer dispatches:

```rust
commands.spawn(panel).observe(
    |down: On<Pointer<Down>>, save: Query<&SaveBtn>, reset: Query<&ResetBtn>| {
        if save.get(down.entity).is_ok() { /* save */ }
        else if reset.get(down.entity).is_ok() { /* reset */ }
        else { /* start drag */ }
    },
);
```

### Buttons only, panel itself decorative

```rust
// Root has no .picking call → cascade resolves to None.
let root = El::new().width(...).height(...);
LayoutBuilder::with_root(root).with(..., |b| {
    b.with(El::new().picking_with(SaveBtn), |b| { b.text("SAVE", style); });
});
```

One proxy (the SAVE button). Picks elsewhere on the panel pass through.

### Inherited mode for a button group

```rust
b.with(
    El::new().picking(Picking::Both),    // cascade = Both for this subtree
    |b| {
        b.with(El::new().picking_with(BtnA), |b| { ... });
        b.with(El::new().picking_with(BtnB), |b| { ... });
    },
);
```

Two button proxies, both double-sided. Mode inherited via cascade — the
buttons don't repeat `Picking::Both` on each one.

## Implementation sketch

1. **Add `Picking` enum + `Resolved<Picking>`** following
   `cascade-resolved-impl.md`. Register in `CascadePanelPlugin`.
2. **Add fields to `El`/`Element`**:
   - `picking: Option<Picking>` (own value)
   - `picking_marker: Option<Box<dyn ClonableComponent>>` (or a typed
     factory closure — see open question below)
3. **Cascade pass**: per-element `Resolved<Picking>` populated from own
   value or ancestor walk.
4. **Proxy spawn pass** (runs after layout compute, before render mesh
   spawn — or interleaved):
   - For each element, evaluate the spawn rule (top-of-region OR has
     marker).
   - Spawn proxy as direct child of the panel entity:
     - `Mesh3d(Rectangle::new(width, height))`
     - Transparent material
     - Transform at element center in panel-local coords
     - `Pickable::default()` + `RayCastBackfaces` for `Back`/`Both`
     - Marker component if `picking_with` was used
   - Despawn old proxies on layout rebuild — same lifecycle as text
     meshes. Prefer recycling-by-marker-identity if cheap; otherwise
     just despawn-all-and-respawn.
5. **Tag all render meshes non-pickable**: in panel_rtt.rs,
   panel_geometry.rs, text_renderer.rs, attach
   `Pickable { should_block_lower: false, is_hoverable: false }` on
   every spawned `Mesh3d` entity.
6. **Despawn-safety on consumer observers**: document that observer
   handlers should use `try_insert` / `try_remove` for component ops
   that target the proxy entity, since proxies are despawned on layout
   rebuild. Same pattern hana already adopted in
   `selection/drag_threshold.rs`.

## Open questions

### Marker storage

`picking_with<C: Component + Clone>(marker: C)` requires storing the
marker in `Element` until the proxy is spawned, then cloning it onto
each spawned proxy entity. Two viable shapes:

- **Trait-object box**: `Box<dyn ClonableComponent>` (a custom trait
  combining `Component + Clone + Send + Sync`). Adds an allocation per
  marked element; works for any `Component + Clone`.
- **Type-erased factory closure**: `Arc<dyn Fn(&mut EntityCommands) +
  Send + Sync>`. No allocation per spawn; cheaper to clone the closure
  pointer; works for non-`Clone` markers too if the closure captures
  the constructor.

Closure form is more flexible and probably faster. Trait-object form
maps more directly to the documented constraint.

### Cutouts

`picking(Picking::None)` on a child of a `Picking::Front` parent
*reads* like "punch a hole" but the parent's proxy is a single solid
rectangle that doesn't know about the child. Two options:

- **Document the limitation**: cutouts not supported; restructure to
  put picking on each surrounding sibling instead of the parent.
- **Implement cutouts**: spawn an inverse-Pickable proxy (or
  `should_block_lower: false` proxy that absorbs picks) at the child's
  bounds, layered in front of the parent's proxy. More complexity,
  rarely needed.

Recommend documenting the limitation for v1.

### Hover/active visual feedback

Per-element pick proxies fire `Pointer<Over>` / `Pointer<Out>` events
that consumers might want to use to drive visual state on the
*visible* mesh sibling (highlight a button background, etc.). Out of
scope for v1 — consumers can wire it themselves via observers. Future
addition could be a built-in `HoverHighlight` style on `El` that the
library applies automatically.

### Backwards compatibility

Existing examples (`panel_rendering.rs`, `units.rs`, `taa_shimmer.rs`)
attach `.observe(on_panel_clicked)` to the panel and expect picks to
work today via accidental per-mesh picking + bubble. To preserve their
behavior under the new design, either:

- Set the cascade default to `Picking::Front` instead of `None` —
  every panel without explicit configuration gets a panel-bounds
  proxy. Simplest migration; surprises decoration-mounted use cases
  (they now block underlying picks unless they opt in to `None`).
- Keep default `None` and migrate the examples to set
  `Picking::Front` on their root `El`. Cleaner, but requires touching
  every example.

Recommend default `None` + migrate examples — examples are part of the
crate, easy to update; the design intent is "explicit picking opt-in"
which is clearer for new users.

## hana usage

Two paths once this lands:

**A. Keep the StatusPlane backdrop, panel as decoration.** No changes
to layout — just don't set `picking` anywhere. Picks fall through to
the StatusPlane Rectangle, which is the `Movable`. (The despawn-safety
fixes already in `selection/drag_threshold.rs` remain useful.)

**B. Drop the backdrop, panel is the movable.** Set
`picking(Picking::Both)` on the layout root, drop the StatusPlane
backdrop entirely, register the `DiegeticPanel` entity itself as the
`Movable`. Fewer entities, simpler chain.

Decision deferred until the API ships.

## Out of scope

- Picking on `WorldText` standalone elements (not panel children).
  Same approach probably applies but tracked separately.
- Per-panel custom picking shapes (circles, polygons). v1 is
  rectangular proxies only.
- Touch / multi-pointer specifics — defers to whatever bevy_picking
  natively supports.
