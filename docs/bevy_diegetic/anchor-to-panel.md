# Anchor to panel

Status: **implementation plan**. This is the point-to-point attachment feature:
place one panel by pinning one of its anchor points to one anchor point on
another panel. It is intentionally separate from span-driven sizing
([`constrained-screen-sizing.md`](constrained-screen-sizing.md)).

## Intent

Make panel placement declarative:

```text
stats panel TopLeft = title panel BottomLeft + (0, 1)
```

The first implementation is screen-space to screen-space anchoring. The public
model should still leave a clean path for world-space panels later. The feature
reuses the existing nine-point `Anchor` vocabulary: top-left, top-center,
top-right, center-left, center, center-right, bottom-left, bottom-center,
bottom-right.

## Boundaries

An attachment pins one point and therefore determines position only. It does not
compute width or height. Width or height driven by two references is a separate
span constraint handled by constrained screen sizing.

Attachment is a layout relationship, not transform ownership:

- Do not use `ChildOf` or make attached panels children of their target panel.
- Do not make a target despawn its attached panels.
- Do not mutate `DiegeticPanel` every frame just to apply resolved placement.
- Do not support cross-coordinate-space attachment in the first pass.
- Do not solve multi-target magnetic layouts in the first pass.

## Module Structure

Add public relationship types in the panel module:

```text
crates/bevy_diegetic/src/panel/anchoring.rs
```

`panel/mod.rs` should include and re-export:

```rust
mod anchoring;

pub use anchoring::AnchoredToPanel;
pub use anchoring::PanelsAnchoredHere;
pub(crate) use anchoring::ResolvedScreenPanelPosition;
```

`lib.rs` should re-export the public types:

```rust
pub use panel::AnchoredToPanel;
pub use panel::PanelsAnchoredHere;
```

Put the screen-space resolver beside the rest of screen-space placement logic:

```text
crates/bevy_diegetic/src/screen_space/anchoring.rs
```

`screen_space/mod.rs` owns the schedule ordering because it already resolves
screen dimensions and writes screen transforms.

`HeadlessLayoutPlugin` should register the public relationship types for
reflection/type-registry parity:

```rust
app.register_type::<AnchoredToPanel>()
    .register_type::<PanelsAnchoredHere>();
```

Relationship hooks come from the relationship derive; registration is for
reflection, inspector, BRP, and type-registry users.

## Public API

Use a Bevy relationship for the attachment graph:

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, FromWorld, Clone)]
#[relationship(relationship_target = PanelsAnchoredHere)]
pub struct AnchoredToPanel {
    #[relationship]
    #[entities]
    #[reflect(ignore)]
    target: Entity,
    pub self_anchor: Anchor,
    pub target_anchor: Anchor,
    pub offset: Vec2,
}

impl FromWorld for AnchoredToPanel {
    fn from_world(_world: &mut World) -> Self {
        Self::new(Entity::PLACEHOLDER, Anchor::Center, Anchor::Center)
    }
}
```

`target` is the target panel entity. `self_anchor` is the point on the attached
panel that should land on `target_anchor`. `offset` is applied after resolving
the target anchor.

Keep `target` private. Mutating a relationship target field in place bypasses
Bevy's relationship hooks and can leave the reverse collection stale. Retarget
by replacing the component:

```rust
commands.entity(panel).insert(
    existing_attachment.retargeted(new_target)
);
```

The target is also ignored by reflection in the first implementation. Reflected
component patching can mutate an existing component in place, so reflected
retargeting would violate the replacement-only relationship contract. Scene or
tooling support for reflected retargeting can be added later with explicit
replacement-based `ReflectComponent` behavior.

The offset unit is coordinate-space specific:

- screen-space: logical pixels, top-left origin, y down
- world-space later: world meters in the target panel plane

Use `Vec<Entity>` for the reverse target collection:

```rust
#[derive(Component, Default, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component, FromWorld, Default)]
#[relationship_target(relationship = AnchoredToPanel)]
pub struct PanelsAnchoredHere(Vec<Entity>);
```

This matches the existing `PanelTextRuns` local pattern and gives deterministic
iteration for tests. Do not use `linked_spawn`; removing or despawning a target
should detach dependents, not despawn them.

Suggested constructors:

```rust
impl AnchoredToPanel {
    pub const fn new(
        target: Entity,
        self_anchor: Anchor,
        target_anchor: Anchor,
    ) -> Self;

    pub const fn with_offset(mut self, offset: Vec2) -> Self;

    pub const fn target(&self) -> Entity;

    pub const fn retargeted(mut self, target: Entity) -> Self;
}
```

Pin the derived relationship helper default with a test:

```rust
<AnchoredToPanel as Relationship>::from(target)
    == AnchoredToPanel::new(target, Anchor::Center, Anchor::Center)
```

This guards the fact that Bevy's derived `Relationship::from(entity)` fills
non-target fields with `Default::default()`, which currently matches
`Anchor::Center` and zero offset.

Mirror `PanelTextRuns` for reverse-index reads:

```rust
impl Deref for PanelsAnchoredHere {
    type Target = [Entity];

    fn deref(&self) -> &Self::Target;
}

impl PanelsAnchoredHere {
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

The title-bar case becomes:

```rust
AnchoredToPanel::new(title_bar, Anchor::TopLeft, Anchor::BottomLeft)
    .with_offset(Vec2::new(0.0, 1.0))
```

## Internal Placement Override

Do not call `DiegeticPanel::set_screen_position` from the attachment resolver as
the normal path. `DiegeticPanel` changes are layout inputs, so repeatedly writing
screen position would mark the panel changed and can cause unnecessary layout
work.

Add a crate-private component required by `DiegeticPanel`:

```rust
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ResolvedScreenPanelPosition {
    pub(crate) anchor_position: Option<Vec2>,
}
```

This component stores the resolved screen coordinate of the panel's own
`panel.anchor()` point for the current frame. `None` means "use the panel's
configured `ScreenPosition`." The screen-space transform system reads this
override before falling back to `CoordinateSpace::Screen { position, .. }`.

The attachment resolver owns this component exclusively in the first
implementation. It computes a desired `Option<Vec2>` for every screen panel,
defaulting to `None`, then writes the component only when the final desired
value differs from the current value. That avoids stale placement when an
attachment is removed, a target becomes invalid, a target despawns, or a cycle
is skipped, without dirtying the override component every frame.

## Screen Bounds Math

Use one crate-private screen-space bounds helper for the resolver and tests:

```rust
struct ScreenPanelBounds {
    top_left: Vec2,
    size: Vec2,
}

impl ScreenPanelBounds {
    fn point(&self, anchor: Anchor) -> Vec2 {
        let (x, y) = anchor.offset(self.size.x, self.size.y);
        self.top_left + Vec2::new(x, y)
    }
}
```

For a screen panel, first resolve the coordinate of its configured panel anchor:

```rust
let configured_anchor_position = match position {
    ScreenPosition::Screen => {
        let (fx, fy) = panel.anchor().offset_fraction();
        Vec2::new(fx * window_width, fy * window_height)
    }
    ScreenPosition::At(pos) => pos,
};
```

If `ResolvedScreenPanelPosition::anchor_position` is `Some(pos)`, use that
instead of `configured_anchor_position`.

Then compute bounds:

```rust
let panel_anchor_offset = panel.anchor().offset(panel.width(), panel.height());
let top_left = anchor_position - Vec2::new(panel_anchor_offset.0, panel_anchor_offset.1);
```

To place a dependent panel:

```rust
let target_point = target_bounds.point(attachment.target_anchor) + attachment.offset;
let self_offset = attachment.self_anchor.offset(self_panel.width(), self_panel.height());
let panel_anchor_offset = self_panel.anchor().offset(self_panel.width(), self_panel.height());

let top_left = target_point - Vec2::new(self_offset.0, self_offset.1);
let resolved_anchor_position =
    top_left + Vec2::new(panel_anchor_offset.0, panel_anchor_offset.1);
```

This lets `self_anchor` differ from `panel.anchor()`. The feature pins the
requested attachment point, while the existing screen transform still receives
the coordinate of the panel's own configured anchor.

## Resolver Algorithm

The resolver is source-owned and two-phase so query borrowing stays simple and
chained attachments can be resolved deterministically. Build the graph from
`AnchoredToPanel`, not from `PanelsAnchoredHere`; the reverse collection is for
traversal and inspection, not resolver correctness.

1. Initialize a desired-position map with `None` for every screen-space panel.
2. Snapshot every screen-space panel whose `WindowRef` resolves to a live,
   nonzero-size concrete window entity:
   - entity
   - resolved window entity
   - configured anchor position
   - current resolved size
   - `panel.anchor()`
   - optional attachment
3. Filter attachments to same-window screen panels whose targets are also screen
   panels. Compare concrete resolved window entities, so `WindowRef::Primary`
   and `WindowRef::Entity(primary)` are treated as the same window.
4. Build a directed graph where `target -> dependent`.
5. Topologically resolve the graph:
   - roots keep their configured placement
   - each dependent reads the current target bounds
   - compute the dependent's resolved anchor position
   - update the snapshot so dependents can become targets for later nodes
6. Treat any node not topologically resolved from active same-window screen
   edges as unresolved for this frame. This includes cycle members and
   dependents blocked downstream of a cycle.
7. Write each desired position into `ResolvedScreenPanelPosition` only when it
   differs from the current component value.

Dead target and target despawn should normally be handled by Bevy relationship
hooks: dependents detach, but do not despawn. Resolver-owned inactive cases are
existing targets that are not panels, targets in another coordinate space,
targets in another resolved window, targets whose window is missing or
zero-sized, and nodes blocked by cycles. These cases should not panic.

Use a bounded diagnostic resource keyed by `(source, target, reason)` rather
than a single callsite `warn_once!`. Reasons should include:

```rust
enum AnchorResolveSkip {
    TargetWithoutPanel,
    SourceWindowMissing,
    TargetWindowMissing,
    CrossWindow,
    MixedCoordinateSpace,
    Cycle,
    BlockedByCycle,
}
```

The attachment resolver should use a non-logging concrete-window helper that
returns a result plus reason, rather than calling the existing
`resolve_window_ref` path that warns at the callsite. That lets diagnostics
distinguish source-window, target-window, cross-window, and zero-size-window
failures per edge. The existing dimension and position systems can keep their
current warning helper unless they are moved to the same bounded diagnostic
model later.

The first implementation should run every frame over screen panels, with
change-aware writes to `ResolvedScreenPanelPosition`. If optimized later, the
invalidation set is: source or target panel size, configured placement,
resolved window size, relation fields, coordinate-space changes, target window
changes, and target liveness.

## Scheduling

Add one public system set:

```rust
pub enum PanelSystems {
    ApplyTreeChanges,
    ComputeLayout,
    ResolveWorldFit,
    ResolvePanelAttachments,
    PositionScreenSpace,
    RenderGizmos,
}
```

Screen-space ordering:

```text
PanelSystems::ResolveWorldFit
ScreenSpaceSystems::ResolveDimensions
ApplyDeferred to deliver PanelDimensionsChanged observers
ApplyDeferred to apply commands queued by those observers
PanelSystems::ResolvePanelAttachments
PanelSystems::PositionScreenSpace
```

Make that order concrete with private screen-space sets, not loose
`.after(...).before(...)` edges on two indistinguishable `ApplyDeferred`
systems:

```rust
enum ScreenSpaceSystems {
    ResolveDimensions,
    FlushDimensionObservers,
    FlushObserverCommands,
}

(
    resolve_screen_space_panel_dimensions
        .in_set(ScreenSpaceSystems::ResolveDimensions),
    ApplyDeferred
        .in_set(ScreenSpaceSystems::FlushDimensionObservers),
    ApplyDeferred
        .in_set(ScreenSpaceSystems::FlushObserverCommands),
    resolve_screen_space_panel_attachments
        .in_set(PanelSystems::ResolvePanelAttachments),
    position_screen_space_panels
        .in_set(PanelSystems::PositionScreenSpace),
)
```

Configure the set chain explicitly:

```text
ScreenSpaceSystems::ResolveDimensions
ScreenSpaceSystems::FlushDimensionObservers
ScreenSpaceSystems::FlushObserverCommands
PanelSystems::ResolvePanelAttachments
PanelSystems::PositionScreenSpace
```

`resolve_screen_space_panel_attachments` should run in
`PanelSystems::ResolvePanelAttachments`. `position_screen_space_panels` should
run after it and should read `ResolvedScreenPanelPosition`.

This preserves the current important property: a first-frame `Fit` panel can
resolve dimensions, fire `PanelDimensionsChanged`, allow observers to react, and
then still have screen transforms positioned correctly in the same frame.

Document the set contract as screen-first: `ResolvePanelAttachments` resolves
screen-space panel attachments before screen-space positioning. Future
world-space anchoring may need a different set or schedule path because it
interacts with transform propagation.

## Relationship Semantics

`AnchoredToPanel` is the source of truth. `PanelsAnchoredHere` is only a reverse
index and should not be mutated directly by users.

Expected behavior:

- Inserting `AnchoredToPanel` updates `PanelsAnchoredHere` on the target.
- Replacing `AnchoredToPanel` moves the dependent between target collections.
- Removing `AnchoredToPanel` clears resolved attachment placement and returns
  the panel to its configured screen position.
- Despawning the target removes `AnchoredToPanel` from dependents through
  Bevy's relationship hooks, but does not despawn the dependent panels.
- Self-attachment is invalid. Leave Bevy's default non-self relationship
  behavior in place.
- Longer cycles are detected by the resolver and skipped.

## World-Space Path

World-space anchoring should use the same public relation, but it is not part of
the first implementation. It should be a deliberate second phase after the
screen-space resolver proves the relationship model, graph handling, diagnostics,
and tests.

The world resolver should use the target panel's local plane:

1. Resolve the target anchor point in target panel-plane coordinates.
2. Transform that point into world space.
3. Place the dependent so its `self_anchor` lands there.
4. Copy the target panel's plane orientation by default.
5. Apply optional post-alignment rotation if the API grows one later.

The scheduling is more delicate than screen space because Bevy updates
`GlobalTransform` in `PostUpdate`. The world-space resolver should either run
before transform propagation with a reliable way to compute target globals, or
run after propagation with an explicit strategy that avoids a one-frame
`GlobalTransform` lag.

World-space anchoring should expose a read path for resolved anchor geometry so
external systems can animate toward an anchor point instead of snapping directly.
That read path is the foundation for spring/magnetic motion and hinged-panel
effects in [`panel_anchoring.md`](panel_anchoring.md).

## Anchor Geometry

Keep first-pass anchor geometry crate-private. The resolver and tests should use
the same screen-bounds helper, and same-frame consumers should avoid reading
`GlobalTransform` for screen panels because screen transforms are written in
`Update` and global propagation is not current until `PostUpdate`.

The eventual public animation API should be space-aware rather than a generic
`Vec3` shape:

```rust
pub enum PanelAnchorPoints {
    Screen {
        window: Entity,
        bounds: PanelScreenBounds,
    },
    World {
        resolved_size: Vec2,
        plane: PanelPlane,
    },
}
```

Do not ship `PanelAnchorPoints` / `PanelPlane` as public API in the first
screen-space anchoring PR. A later animation/spring feature can define the read
pattern, frame-validity contract, and screen-vs-world units.

## Implementation Phases

### Phase 1 — screen-space point anchoring

1. Add `panel/anchoring.rs` with `AnchoredToPanel`,
   `PanelsAnchoredHere`, constructors, and `ResolvedScreenPanelPosition`.
2. Add `ResolvedScreenPanelPosition` as a required component for
   `DiegeticPanel`.
3. Re-export public anchoring types from `panel/mod.rs` and `lib.rs`.
4. Register `AnchoredToPanel` and `PanelsAnchoredHere` in
   `HeadlessLayoutPlugin`.
5. Add `PanelSystems::ResolvePanelAttachments`.
6. Split screen-space anchoring code into `screen_space/anchoring.rs`.
7. Update `position_screen_space_panels` to use
   `ResolvedScreenPanelPosition::anchor_position` when present.
8. Implement same-window screen-to-screen resolution and cycle detection.
9. Replace the `diegetic_text_stress` title-bar observer with
   `AnchoredToPanel`.
10. Keep constrained sizing out of this change.
11. Keep public anchor geometry out of this change; use crate-private bounds
    helpers only.

### Phase 2 — public anchor geometry reads

Expose read-only resolved anchor geometry without changing attachment behavior.
This phase should define the stable API that animation systems can consume:

- screen-space anchor points in logical pixels, top-left origin, y down
- world-space anchor points in world meters, including plane basis
- frame-validity contract: when in the schedule the values are current
- no transform writes; this phase is observation only

The first consumer should be an example or test that animates an entity toward a
panel anchor point without using `AnchoredToPanel` as the mover.

### Phase 3 — world-space point anchoring

Implement world-to-world panel attachment using the same `AnchoredToPanel`
relationship:

1. Add a world-space attachment resolver with its own scheduling contract.
2. Compute target anchor points from the target panel's resolved world plane.
3. Place dependents coplanar with the target by default.
4. Preserve the dependent panel's chosen `self_anchor`.
5. Support an optional post-alignment local rotation if the API needs it for
   hinged or folded presentations.
6. Reuse the source-owned graph, cycle handling, and diagnostics from the
   screen-space resolver.
7. Add tests for same-frame world anchor placement, chained world attachments,
   target motion, target rotation, cycle fallback, and transform-propagation
   timing.

This phase should make `examples/panel_anchoring.rs` possible for static and
keyboard-driven world anchoring.

### Phase 4 — animated anchor consumers

Build animation examples on top of the public anchor geometry rather than
changing the resolver into a physics system:

- spring/magnetic attraction between two panels using resolved anchor points
- elastic easing between attached and detached positions
- chained panel unwrapping where anchor points define hinge edges

The relationship remains the exact snap constraint. Animation systems can read
the same anchor geometry and choose whether to move directly, ease, spring, or
hinge around it.

## Tests

Add focused unit tests in `screen_space` / anchoring modules:

- reverse relationship index is maintained when inserting, replacing, and
  removing `AnchoredToPanel`
- first-frame `Fit` target and dependent resolve before screen transform
- title-bar case places dependent top-left one pixel below target bottom-left
- table-driven anchor math uses literal expected screen coordinates and final
  `Transform` values, not only the shared helper
- literal-coordinate cases include top-left to bottom-left, center to center
  with offset, and bottom-right to top-right
- `self_anchor` can differ from `panel.anchor()`
- target resize repositions dependent in the same update
- target `ScreenPosition::At` movement repositions dependent
- window resize repositions `ScreenPosition::Screen` targets and dependents
- removing `AnchoredToPanel` returns dependent to configured placement
- target despawn detaches dependent without despawning it
- observer queued `AnchoredToPanel` insertion after `PanelDimensionsChanged`
  takes effect before attachment resolution in the same frame
- `WindowRef::Primary` and `WindowRef::Entity(primary)` are treated as the same
  window
- missing explicit window, missing primary window, and zero-sized window skip
  without panic
- cross-window and world/screen mixed attachments are skipped without panic
- each skip test starts from a valid resolved attachment, flips into the invalid
  state, then asserts the override clears, the final transform returns to
  configured placement, and the diagnostic resource records the exact
  `(source, target, reason)`
- cycle leaves cyclic panels at configured placement and emits a diagnostic
- descendants downstream of a cycle clear to configured placement and emit a
  blocked-by-cycle diagnostic
- localized cycle failure: `A <-> B` falls back while independent `X -> Y -> Z`
  still resolves in the same update
- chained `A -> B -> C` attachments propagate in one update
- retargeting the middle node of a chain updates downstream placement in one
  update
- resolver does not mutate `DiegeticPanel` when only resolved attachment
  placement changes
- `Changed<ResolvedScreenPanelPosition>` fires on first resolve, target
  move/resize, and relation removal, but not on identical stable frames
- `PanelDimensionsChanged` observer can queue `AnchoredToPanel` insertion with
  `Commands`; the reverse index and final transform resolve in that same
  `App::update`
- `AnchoredToPanel` and `PanelsAnchoredHere` are present in `AppTypeRegistry`
- reflective apply cannot retarget an existing `AnchoredToPanel`; retargeting is
  replacement-only
- `<AnchoredToPanel as Relationship>::from(target)` matches the public
  constructor defaults

Validation commands:

```sh
cargo +nightly fmt -p bevy_diegetic --check
cargo check -p bevy_diegetic --example diegetic_text_stress
cargo nextest run -p bevy_diegetic screen_space
cargo nextest run -p bevy_diegetic anchor
```

## Team Review Notes

This section records `team_review 2` findings for this implementation plan.

### Cycle 1

Recorded in the plan:

- `AnchoredToPanel::target` is private; retargeting replaces the component so
  Bevy relationship hooks maintain `PanelsAnchoredHere`.
- `AnchoredToPanel` / `PanelsAnchoredHere` mirror the local relationship derive,
  reflection, `FromWorld`, and read-only reverse-index patterns.
- The resolver computes desired override state for every screen panel and writes
  only when the final value differs.
- The graph is built from source-side `AnchoredToPanel`, with
  `PanelsAnchoredHere` reserved for traversal and tests.
- Same-window checks compare resolved concrete window entities.
- Scheduling includes two deferred barriers after dimension resolution: one to
  deliver dimension observers and one to apply commands queued by observers.
- Diagnostics are per edge and reason, not one callsite-wide warning.
- Public anchor geometry is deferred; first pass uses crate-private screen
  bounds helpers.
- Tests now cover chain propagation, cycle-blocked descendants, retargeting,
  observer-command ordering, concrete window identity, and stale override
  cleanup.

Cycle 1 surfaced no premise challenge and no unresolved user decision.

### Cycle 2

Recorded in the plan:

- `ResolvedScreenPanelPosition` now uses desired-state reconciliation:
  compute each panel's final desired `Option<Vec2>`, then write only when it
  differs.
- `ResolvedScreenPanelPosition` is explicitly owned by the screen-space
  anchoring resolver in the first implementation.
- The two deferred barriers are pinned to private ordered screen-space sets, so
  observer commands can affect attachment resolution in the same frame.
- `AnchoredToPanel` / `PanelsAnchoredHere` registration is assigned to
  `HeadlessLayoutPlugin`.
- `AnchoredToPanel::target` is ignored by reflection; reflected retargeting is
  intentionally unsupported until a replacement-based reflection path exists.
- The resolver uses a non-logging concrete-window helper so diagnostics remain
  per edge and per reason.
- Tests now require literal-coordinate anchor cases, invalid-state rollback
  assertions, changed-probe coverage for the resolved override, localized cycle
  failure, AppTypeRegistry registration, reflective-retarget protection, and
  `Relationship::from` default parity.

Cycle 2 surfaced no premise challenge and no unresolved user decision.
