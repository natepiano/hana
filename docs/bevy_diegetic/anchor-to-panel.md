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
pub use anchoring::PanelAnchorOffset;
pub use anchoring::PanelsAnchoredHere;
pub(crate) use anchoring::ResolvedScreenPanelPosition;
```

`lib.rs` should re-export the public types:

```rust
pub use panel::AnchoredToPanel;
pub use panel::PanelAnchorOffset;
pub use panel::PanelsAnchoredHere;
```

Put the screen-space resolver beside the rest of screen-space placement logic:

```text
crates/bevy_diegetic/src/screen_space/anchoring/
```

`screen_space/mod.rs` owns the schedule ordering because it already resolves
screen dimensions and writes screen transforms.

`HeadlessLayoutPlugin` should register the public relationship types for
type-registry parity:

```rust
app.register_type::<AnchoredToPanel>()
    .register_type::<PanelAnchorOffset>()
    .register_type::<PanelsAnchoredHere>();
```

Relationship hooks come from the relationship derive; registration is not what
makes the reverse collection work. Phase 1 reflection is deliberately
type-registration-only for the relationship source and reverse index:

- do not attach `ReflectComponent` mutation for `AnchoredToPanel`
- do not attach `ReflectComponent` mutation for `PanelsAnchoredHere`
- do not allow BRP / inspector patching to insert a placeholder-target
  relationship or mutate the reverse index

If component-level reflection is needed later, add custom replacement-based
behavior that can reject in-place retargeting and reverse-index mutation.

## Public API

Use a Bevy relationship for the attachment graph:

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[component(immutable)]
#[reflect(PartialEq, Debug, FromWorld, Clone)]
#[relationship(relationship_target = PanelsAnchoredHere)]
pub struct AnchoredToPanel {
    #[relationship]
    #[entities]
    #[reflect(ignore)]
    target: Entity,
    pub source_anchor: Anchor,
    pub target_anchor: Anchor,
    pub offset: PanelAnchorOffset,
}

impl FromWorld for AnchoredToPanel {
    fn from_world(_world: &mut World) -> Self {
        Self::new(Entity::PLACEHOLDER, Anchor::Center, Anchor::Center)
    }
}
```

`target` is the target panel entity. `source_anchor` is the point on the attached
panel that should land on `target_anchor`. `offset` is applied after resolving
the target anchor.

Keep `target` private and mark `AnchoredToPanel` as an immutable component. The
combination of component immutability and private target visibility prevents
normal `Query<&mut AnchoredToPanel>` / `Relationship::set_risky` style mutation
from bypassing relationship hooks. Retarget by replacing the component:

```rust
commands.entity(panel).insert(
    existing_attachment.retargeted(new_target)
);
```

The target is also ignored by reflection in the first implementation. Reflected
component patching can mutate an existing component in place, so component-level
reflection is not exposed for this relationship in Phase 1. Scene or tooling
support for reflected retargeting can be added later with explicit
replacement-based `ReflectComponent` behavior.

Use a named offset type instead of a raw `Vec2` so the public API uses the same
dimension system as panel sizing and text sizing:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Default)]
pub struct PanelAnchorOffset {
    x: Dimension,
    y: Dimension,
}

impl PanelAnchorOffset {
    pub const ZERO: Self;

    pub fn new(x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self;
    pub const fn x(self) -> Dimension;
    pub const fn y(self) -> Dimension;
}
```

The resolver converts the stored dimensions at the point where it knows the
target panel's coordinate system:

- screen-space: target panel layout units, normally logical pixels, top-left
  origin, y down
- world-space: target panel layout units scaled onto the target panel plane,
  x right and y down

Bare `f32` values resolve in the target panel's layout unit. Explicit
`Px`/`Mm`/`Pt`/`In` values carry their units, matching `TextStyle::new` and
`LayoutBuilder::new`.

`with_offset(PanelAnchorOffset)` is the typed constructor.

Use `Vec<Entity>` for the reverse target collection:

```rust
#[derive(Component, Default, Debug, PartialEq, Eq, Reflect)]
#[reflect(FromWorld, Default)]
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
        source_anchor: Anchor,
        target_anchor: Anchor,
    ) -> Self;

    pub const fn with_offset(mut self, offset: PanelAnchorOffset) -> Self;

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
    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0)))
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
let target_point =
    target_bounds.point(attachment.target_anchor)
        + attachment.offset.to_layout_units(target_panel.layout_unit());
let self_offset = attachment.source_anchor.offset(self_panel.width(), self_panel.height());
let panel_anchor_offset = self_panel.anchor().offset(self_panel.width(), self_panel.height());

let top_left = target_point - Vec2::new(self_offset.0, self_offset.1);
let resolved_anchor_position =
    top_left + Vec2::new(panel_anchor_offset.0, panel_anchor_offset.1);
```

This lets `source_anchor` differ from `panel.anchor()`. The feature pins the
requested attachment point, while the existing screen transform still receives
the coordinate of the panel's own configured anchor.

## Resolver Algorithm

The resolver is source-owned and two-phase so query borrowing stays simple and
chained attachments can be resolved deterministically. Build the graph from
`AnchoredToPanel`, not from `PanelsAnchoredHere`; the reverse collection is for
traversal and inspection, not resolver correctness.

1. Initialize a desired-position map with `None` for every screen-space panel.
2. Scan all source-side `AnchoredToPanel` candidates before filtering. This pass
   reads the source panel, target entity, source coordinate space, target
   coordinate space when present, window references, and sizing state so invalid
   edges can be diagnosed before they disappear from the active graph.
3. Snapshot every screen-space panel whose `WindowRef` resolves to a live,
   nonzero-size concrete window entity:
   - entity
   - resolved window entity
   - configured anchor position
   - current resolved size
   - `panel.anchor()`
   - optional attachment
4. Filter attachments to same-window screen panels whose targets are also screen
   panels. Compare concrete resolved window entities, so `WindowRef::Primary`
   and `WindowRef::Entity(primary)` are treated as the same window.
5. Build a directed graph where `target -> dependent`.
6. Topologically resolve the graph:
   - roots keep their configured placement
   - each dependent reads the current target bounds
   - compute the dependent's resolved anchor position
   - update the snapshot so dependents can become targets for later nodes
7. Treat any node not topologically resolved from active same-window screen
   edges as unresolved for this frame. This includes cycle members and
   dependents blocked downstream of a cycle.
8. Write each desired position into `ResolvedScreenPanelPosition` only when it
   differs from the current component value.

Track candidate state explicitly:

```rust
enum AnchorResolveState {
    Configured,
    Resolved,
    Skipped(AnchorResolveSkip),
}
```

Outgoing edges from a skipped source do not resolve in that frame. Their
dependents fall back to configured placement and record
`BlockedBySkippedDependency`, except cycle descendants, which record
`BlockedByCycle`. This gives every fallback path one source of truth for final
placement and diagnostics.

When Phase 3 adds a world resolver, each resolver handles only the source panels
it can actually place:

- screen sources are classified and resolved only by the screen resolver
- world sources are classified and resolved only by the world resolver
- screen-to-world and world-to-screen attachments remain unsupported for now and
  are diagnosed instead of partially resolved
- each resolver clears only the private override state it owns

Dead target and target despawn should normally be handled by Bevy relationship
hooks: dependents detach, but do not despawn. The resolver still needs precise
fallback reasons for transient same-frame states, hook timing, and manually
authored invalid relationships. Resolver-owned inactive cases are missing source
panel data, missing target entity, existing targets that are not panels,
self-attachment observed before cleanup, targets in another coordinate space,
targets in another resolved window, targets whose window is missing or
zero-sized, and nodes blocked by cycles. These cases should not panic.

Use a bounded diagnostic resource keyed by `(source, target, reason)` rather
than a single callsite `warn_once!`. Reasons should include:

```rust
enum AnchorResolveSkip {
    SourceWithoutPanel,
    TargetMissing,
    TargetWithoutPanel,
    SelfAttachment,
    SourceWindowMissing,
    SourceWindowZeroSized,
    TargetWindowMissing,
    TargetWindowZeroSized,
    CrossWindow,
    MixedCoordinateSpace,
    Cycle,
    BlockedByCycle,
    BlockedBySkippedDependency,
    UnsupportedWorldParentTransform,
}
```

Store diagnostics in a bounded resource:

```rust
struct AnchorResolveDiagnostics {
    current_frame: u64,
    entries: VecDeque<AnchorResolveDiagnostic>,
    capacity: usize,
}

struct AnchorResolveDiagnostic {
    source: Entity,
    target: Entity,
    reason: AnchorResolveSkip,
    first_seen_frame: u64,
    last_seen_frame: u64,
    count: u32,
}
```

Use `(source, target, reason)` as the dedup key. On each resolver run, mark
entries seen in the current frame; if an edge becomes valid, stop refreshing
that entry. Evict oldest entries past a fixed capacity such as 128. Tests should
assert both the current-frame diagnostic set and the historical record where
history matters.

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
ScreenSpaceSystems::FlushDimensionObservers
ScreenSpaceSystems::FlushObserverCommands
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

The flush contract is precise:

1. `resolve_screen_space_panel_dimensions` queues `PanelDimensionsChanged`.
2. `FlushDimensionObservers` delivers those observers.
3. Observers may queue `AnchoredToPanel` inserts, replacements, or removals.
4. `FlushObserverCommands` applies those commands so source-side
   `AnchoredToPanel` is visible to the resolver. Any Bevy relationship hook work
   that is applied during the command flush may update `PanelsAnchoredHere`, but
   the resolver does not depend on reverse-index visibility in that frame.
5. `resolve_screen_space_panel_attachments` reads source-side
   `AnchoredToPanel`. It may inspect `PanelsAnchoredHere` in tests or tooling,
   but never relies on the reverse index for resolver correctness.

This preserves the current important property: a first-frame `Fit` panel can
resolve dimensions, fire `PanelDimensionsChanged`, allow observers to react, and
then still have screen transforms positioned correctly in the same frame.

Relationship mutation timing is part of the public contract:

- relation changes whose commands are applied before
  `PanelSystems::ResolvePanelAttachments` affect the current frame
- relation changes applied after `PanelSystems::ResolvePanelAttachments` affect
  the next frame
- systems that need same-frame attachment placement should run before the
  resolver or before `FlushObserverCommands`, depending on whether they use
  direct world mutation or `Commands`

Document the set contract as screen-first: `ResolvePanelAttachments` resolves
screen-space panel attachments before screen-space positioning. Future
world-space anchoring may need a different set or schedule path because it
interacts with transform propagation.

Add a Phase-1 screen consumer audit before replacing the title-bar observer.
Any Update-stage system that reads `GlobalTransform` for screen panels can see
stale globals after `position_screen_space_panels` writes transforms. Internal
same-frame consumers should either:

- run after `PanelSystems::PositionScreenSpace` and read the screen-bounds helper
  / `ResolvedScreenPanelPosition`
- or run after Bevy transform propagation if they truly need `GlobalTransform`

Extend the audit through `PostUpdate` child/render-prep systems. Screen
attachment placement writes local `Transform` in `Update`; text, SDF geometry,
and other panel-child systems must either build before
`TransformSystems::Propagate`, use a post-propagation correction path, or
explicitly document a one-frame delay. Add a smoke test for a newly attached
screen panel with text/SDF children rendering at the resolved transform in the
same update.

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
3. Place the dependent so its `source_anchor` lands there.
4. Copy the target panel's plane orientation by default.
5. Compose any Phase-4 post-alignment rotation after plane alignment.

World anchor geometry must use resolved visual size in world meters, not raw
layout-unit `panel.width()` / `panel.height()`. Use the shared plane helper
from Phase 2:

```rust
impl PanelPlane {
    pub fn from_panel(
        panel: &DiegeticPanel,
        transform: &GlobalTransform,
    ) -> Result<Self, PanelAnchorGeometryError>;
}
```

`PanelPlane::from_panel` uses `panel.world_width()` / `panel.world_height()`,
the panel's current `GlobalTransform`, and transform scale. Its invariants are:

- `origin()` is the world-space top-left corner of the panel
- `right`, `up`, and `normal` are unit orthonormal world directions
- `size` is the resolved visual width/height in world meters

World anchor coordinates should use `PanelPlane::point(anchor)` and
`PanelPlane::edge(edge)` directly. Do not recalculate anchor fractions in the
world resolver; the Phase 2 helper is the public contract and already accounts
for the panel's configured `panel.anchor()`, transform scale, and top-left plane
origin.

```rust
let point_world = plane.point(anchor);
```

The x axis points right. Panel-local y-down coordinates map to negative world
`up`; `PanelPlane::point(anchor)` owns that sign conversion.

The target anchor point applies the authored offset after converting it through
the target panel's layout unit and world-plane size. Panel-local positive y
points down, so world placement subtracts the target plane's `up` vector for a
positive y offset.

```rust
let target_point_world = target_plane.point(target_anchor)
    + target_plane.right * offset_meters.x
    - target_plane.up * offset_meters.y;
```

The source panel's desired global origin is:

```rust
let source_point = source_panel_local_point(source_panel, source_anchor);
let scaled_source_point = source_scale * source_point;
let desired_rotation = target_plane_rotation * post_alignment_rotation;
let desired_translation = target_point_world - desired_rotation * scaled_source_point;
```

If the source panel has a parent, convert the desired global transform back into
the parent's local space with the current parent global inverse. Preserve the
source panel's scale unless a later explicit scale mode is added. Phase 3 should
support unparented panels, rotated parents, and uniform-scale parents. Diagnose
non-uniform or sheared parent chains with `UnsupportedWorldParentTransform`
instead of decomposing an unrepresentable transform into a lossy Bevy
`Transform`.

World anchoring needs an authored-pose fallback because it writes `Transform`
directly. Phase 3 should add a crate-private record such as
`AnchoredWorldPanelPose` that captures the dependent's authored local transform
when the relation first becomes active. On relation removal, invalid target,
skipped dependency, or cycle fallback, restore the authored transform and clear
that record. While the relation is active, the world resolver writes placement
and default plane orientation; user-authored transform edits should be made by
removing the relation or by writing a separate offset/rotation component that
the world resolver reads.

Use one no-lag schedule strategy in Phase 3: run world attachment resolution in
`PostUpdate` before `TransformSystems::Propagate`, compute current target and
parent globals with `TransformHelper::compute_global_transform` or an equivalent
parent-chain helper, write the dependent's local `Transform`, and let normal
transform propagation update `GlobalTransform` and children. Do not read stale
`GlobalTransform` from the same frame as an input unless the helper has computed
it.

World-space anchoring should expose a read path for resolved anchor geometry so
external systems can animate toward an anchor point instead of snapping directly.
That read path is the foundation for spring/magnetic motion and hinged-panel
effects in
[`panel-anchoring-example.md`](panel-anchoring-example.md).

## Anchor Geometry

Keep first-pass anchor geometry crate-private. The resolver and tests should use
the same screen-bounds helper, and same-frame consumers should avoid reading
`GlobalTransform` for screen panels because screen transforms are written in
`Update` and global propagation is not current until `PostUpdate`.

Phase 2 should prefer a public query helper such as
`PanelAnchorGeometryParam`. If implementation needs a cache component, treat it
as library-owned: fields private, constructors/update methods crate-private, and
type-registration-only reflection. Do not expose `ReflectComponent` mutation for
resolved geometry.

The public read surface should be equivalent to:

```rust
pub struct ResolvedPanelAnchorGeometry {
    points: PanelAnchorPoints,
}

impl ResolvedPanelAnchorGeometry {
    pub const fn points(&self) -> &PanelAnchorPoints;
}

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

pub struct PanelScreenBounds {
    top_left: Vec2,
    size: Vec2,
}

pub struct PanelPlane {
    origin: Vec3,
    right: Vec3,
    up: Vec3,
    normal: Vec3,
    size: Vec2,
}

pub enum PanelAnchorEdge {
    Top,
    Right,
    Bottom,
    Left,
}
```

Both `PanelScreenBounds` and `PanelPlane` should expose `point(anchor)` and
`edge(edge)` helpers. Hinge-style consumers need edge geometry; do not extend
`AnchoredToPanel` itself beyond point-to-point snapping.

`PanelPlane` must keep one invariant: `right`, `up`, and `normal` are unit
orthonormal world directions, while `size` stores the resolved panel extents in
meters. Construct it through `PanelPlane::from_panel` or a checked constructor,
not by public field mutation.

Frame-validity contract:

- If Phase 2 uses cached geometry, it adds `PanelSystems::PublishAnchorGeometry`
  as the writer set for that cache.
- helper-backed screen geometry is current after
  `PanelSystems::PositionScreenSpace`; cached screen geometry is current after
  `PanelSystems::PublishAnchorGeometry` when that set exists and runs after
  positioning
- helper-backed world geometry must compute current parent/target transforms
  when used by same-frame writers; cached world geometry is current after Bevy
  transform propagation and the world geometry publisher
- same-frame transform-writing consumers should use a helper-backed
  `PanelAnchorGeometryParam` mode that computes current target geometry before
  they write transforms; cached post-propagation world geometry is otherwise a
  read-only snapshot for later systems or the next frame
- missing windows, unresolved panels, and invalid panels return `None` or an
  explicit error; do not synthesize geometry from stale transforms

Do not ship `ResolvedPanelAnchorGeometry` / `PanelAnchorPoints` / `PanelPlane` as
public API in the first screen-space anchoring PR. Phase 2 defines the read
pattern, frame-validity contract, and screen-vs-world units before animation or
world anchoring relies on them.

## Implementation Phases

### Phase 1 — screen-space point anchoring

Status: **complete**.

Keep Phase 1 small enough to implement and review in checkpoints.

#### Phase 1A — relationship API and lifecycle

1. Add `panel/anchoring.rs` with `AnchoredToPanel`, `PanelAnchorOffset`,
   `PanelsAnchoredHere`, constructors, and read-only reverse-index helpers.
   `AnchoredToPanel` is `#[component(immutable)]`, and `PanelAnchorOffset`
   stores `Dimension` values for x and y offsets.
2. Re-export public anchoring types from `panel/mod.rs` and `lib.rs`.
3. Register `AnchoredToPanel`, `PanelAnchorOffset`, and `PanelsAnchoredHere` in
   `HeadlessLayoutPlugin` for type-registry parity.
4. Keep relationship reflection type-registration-only; do not expose
   `ReflectComponent` mutation for the source or reverse component in this
   phase.
5. Add lifecycle tests for insert, replacement retarget, removal, target
   despawn, non-despawning dependents, `Relationship::from` defaults,
   AppTypeRegistry registration, component immutability, and absence of
   `ReflectComponent` type data for the relationship source and reverse index.

#### Phase 1B — private screen placement override

1. Add `ResolvedScreenPanelPosition` as a required component for
   `DiegeticPanel`.
2. Update `position_screen_space_panels` to use
   `ResolvedScreenPanelPosition::anchor_position` when present.
3. Implement desired-state reconciliation: compute every screen panel's final
   `Option<Vec2>` first, then write the component only when the value differs.
4. Assert stable frames do not trigger `Changed<ResolvedScreenPanelPosition>`.
5. Separate `Added` from `Changed` in tests: spawn-frame addition of the
   required component is not an attachment-change signal. Assert actual
   `None -> Some -> None` value transitions.
6. Assert removing or invalidating an attachment clears the override and
   returns the panel to configured placement.

#### Phase 1C — schedule and observer flushes

1. Add `PanelSystems::ResolvePanelAttachments`.
2. Split screen-space anchoring code into `screen_space/anchoring/`.
3. Convert the existing private `ScreenSpaceSystems` chain to include
   `FlushDimensionObservers` and `FlushObserverCommands`.
4. Place `resolve_screen_space_panel_attachments` after both flushes and before
   `PanelSystems::PositionScreenSpace`.
5. Add a same-`App::update` test where a `PanelDimensionsChanged` observer queues
   an `AnchoredToPanel` insert, source-side relation state is visible, and the
   dependent's final transform is positioned in that same update.
6. Add before/after resolver writer tests: relation writes applied before
   `PanelSystems::ResolvePanelAttachments` affect the current frame; writes
   applied after it affect the next frame.

#### Phase 1D — screen resolver and math

1. Add the crate-private `ScreenPanelBounds` helper and use it in both resolver
   code and tests.
2. Scan all source-side attachment candidates before filtering so invalid
   source, target, window, and sizing states are diagnosable.
3. Resolve only same-window screen-to-screen edges, comparing concrete resolved
   window entities.
4. Build the active graph from source-side `AnchoredToPanel`, not from
   `PanelsAnchoredHere`.
5. Resolve roots, chains, retargets, and independent subgraphs in one update.
6. Detect self-attachment through Bevy's relationship behavior and longer cycles
   through the resolver.
7. Convert offset dimensions through the target panel's layout unit before
   resolving an edge.
8. Add a source coordinate-space transition test: start from a valid screen
   attachment, change the source out of screen space, and assert the screen
   override clears or is ignored without stale reappearance.
9. Keep constrained sizing and public anchor geometry out of this phase.

#### Phase 1E — fallback and diagnostics

1. Add `AnchorResolveState` and `AnchorResolveSkip`.
2. Add `AnchorResolveDiagnostics` with bounded history, current-frame tracking,
   `(source, target, reason)` deduping, and oldest-entry eviction.
3. Use a non-logging concrete-window helper for the attachment resolver.
4. For every invalid-state test, start from a valid resolved attachment, flip to
   the invalid state, and assert both override clearing and the exact diagnostic
   reason.
5. Assert a localized cycle does not block unrelated valid chains.
6. Add diagnostics-lifecycle tests for repeated invalid edges coalescing,
   `count` / `last_seen_frame`, valid recovery clearing current-frame issues
   while preserving history, separate reasons for the same edge, and capacity
   eviction.

#### Phase 1F — first consumer and screen-global audit

1. Audit same-frame screen-panel `GlobalTransform` consumers before replacing
   the title-bar observer. Known starting points are IME activation/editor
   queries that read `(&DiegeticPanel, &ComputedDiegeticPanel,
   &GlobalTransform)`.
2. Record each consumer's resolution in the implementation PR: screen-panel
   reads use screen bounds / resolved anchor data after
   `PanelSystems::PositionScreenSpace`; world-panel reads use propagated globals
   or `TransformHelper`.
3. Add an IME/editor regression for an attached screen panel moving in the same
   frame.
4. Audit `PostUpdate` panel-child and render-prep ordering for text/SDF children.
5. Replace the `diegetic_text_stress` title-bar observer with
   `AnchoredToPanel`.
6. Keep the GPU/perf meter screen placement manually configured unless this
   phase also adds the needed explicit anchor relation there.

#### Retrospective

**What worked:**

- `AnchoredToPanel` and `PanelsAnchoredHere` fit Bevy relationship hooks cleanly:
  insert, retarget, removal, and target despawn all stayed source-owned.
- The two screen-space flush sets let a `PanelDimensionsChanged` observer queue
  an `AnchoredToPanel` insert and still resolve final screen placement in the
  same `App::update`.

**What deviated from the plan:**

- `AnchoredToPanel` needed `#[reflect(ignore, default = "placeholder_entity")]`
  on its private target field so type registration works without exposing
  `ReflectComponent` mutation.
- The Phase 1F IME/editor and text/SDF render smoke coverage was left as a
  remaining consumer-audit item; the implementation replaced the
  `diegetic_text_stress` title-bar observer and kept the deeper same-frame read
  path out of the first resolver patch.

**Surprises:**

- Fixed screen panel sizing overwrites direct `DiegeticPanel::set_width` /
  `set_height` test mutations during screen dimension resolution, so resize
  tests must change the authored sizing input or replace the panel component.
- Fit screen-panel text measured through the existing point-to-pixel conversion,
  so first-frame Fit tests assert positive resolved dimensions rather than raw
  authored point values.

**Implications for remaining phases:**

- Phase 2 should start by publishing a reusable screen-bounds / anchor-geometry
  read path before moving IME/editor or child-render consumers away from
  `GlobalTransform` assumptions.
- Phase 3 can reuse the source-owned graph, skip diagnostics, and change-aware
  override pattern, but world anchoring still needs its own authored-pose
  fallback because it writes `Transform` directly.

#### Phase 1 Review

- Phase 2 now starts from the Phase 1 private screen-bounds math and the
  leftover IME/editor plus text/SDF consumer audit, before adding a generic
  anchor-animation consumer.
- Phase 2 cache language now treats `PanelSystems::PublishAnchorGeometry` as
  conditional on a cached implementation; helper-only geometry reads do not need
  a cache just to satisfy the plan.
- Phase 3 now explicitly moves world source panels to a world resolver before
  enabling world-to-world attachments, so the screen resolver does not try to
  handle them.
- Phase 4 now has a release-pose handoff rule for elastic attach/detach
  animations so they do not fight Phase 3 authored-pose restoration.

### Phase 2 — public anchor geometry reads

Status: **complete**.

Expose read-only resolved anchor geometry without changing attachment behavior.
This phase should define the stable API that animation systems can consume:

1. Extract or reuse the Phase 1 private screen-bounds math instead of adding a
   second screen geometry implementation.
2. Add a public read surface equivalent to `ResolvedPanelAnchorGeometry`,
   `PanelAnchorPoints`, `PanelScreenBounds`, `PanelPlane`, and
   `PanelAnchorEdge`.
3. Prefer `PanelAnchorGeometryParam`; if a cache component is needed, keep it
   library-owned with private fields and no `ReflectComponent` mutation.
4. Add `PanelSystems::PublishAnchorGeometry` only if the implementation uses
   cached geometry writes. Helper-only geometry reads do not need a cache writer
   set.
5. Provide `point(anchor)` and `edge(edge)` helpers for screen bounds and world
   planes.
6. Publish screen-space anchor geometry in logical pixels with top-left origin
   and y down.
7. Publish world-space anchor geometry in world meters with an explicit unit
   orthonormal plane basis and resolved meter extents.
8. Define the freshness contract in the API docs:
   - cached screen geometry is current after `PanelSystems::PublishAnchorGeometry`
     following `PanelSystems::PositionScreenSpace`
   - cached world geometry is current after transform propagation and the world
     geometry publisher
   - same-frame transform writers use helper-backed current geometry rather than
     cached post-propagation snapshots
9. Return `None` or an explicit error for missing windows, unresolved panels,
   zero-sized panels, and invalid panel state; do not synthesize geometry from
   stale transforms.
10. Keep this phase read-only: no transform writes and no changes to
   `AnchoredToPanel` behavior.
11. Move the leftover Phase 1F consumer audit into this phase: classify
    IME/editor screen-panel reads and text/SDF child/render-prep reads, then
    migrate same-frame screen consumers to the public geometry read path where
    they should not depend on stale `GlobalTransform`.

The first consumer should be the IME/editor same-frame screen-panel read path or
a small regression that proves it can consume current screen geometry for an
attached panel. After that, add an example or test that animates an entity
toward a panel anchor point without using `AnchoredToPanel` as the mover. This
phase must also cover edge geometry because the chained unwrap example needs
hinge edges, not only point centers.

#### Retrospective

**What worked:**

- `panel/anchor_geometry.rs` added helper-backed public geometry reads through
  `PanelAnchorGeometryParam`, `ResolvedPanelAnchorGeometry`,
  `PanelScreenBounds`, `PanelPlane`, and `PanelAnchorEdge`.
- `screen_space/anchoring/` now uses `PanelScreenBounds`, so screen attachment
  placement and public screen geometry use the same anchor math.

**What deviated from the plan:**

- No cache component or `PanelSystems::PublishAnchorGeometry` was added; the
  implementation is helper-only.
- `ime/editor.rs` was the same-frame screen-panel consumer that needed code
  changes. `ImeSystemSet::UpdateEditorGeometry` now runs after
  `PanelSystems::ResolvePanelAttachments` and before
  `PanelSystems::PositionScreenSpace`, and screen-panel fields read
  `PanelAnchorGeometryParam`.
- The text/SDF audit did not require a migration in this phase: SDF panel-child
  build reads panel layout, and text batch transform copying remains in
  `PostUpdate` around transform propagation.

**Surprises:**

- A system that reads `PanelAnchorGeometryParam` and writes `Transform` should
  use `ParamSet`, because the helper uses Bevy's `TransformHelper` internally.
- `PanelPlane` needs to reject sheared or non-orthogonal world axes so public
  world anchor points stay meaningful.

**Implications for remaining phases:**

- Phase 3 can build world-to-world placement from `PanelPlane`, including the
  same invalid-plane check used by public geometry reads.
- Phase 4 animation systems should read geometry and write transforms through a
  `ParamSet`, or write a separate component that the world resolver reads.
- Screen-to-world and world-to-screen attachments are still out of scope until a
  later design explicitly defines them.

#### Phase 2 Review

- Phase 3 now uses the Phase 2 `PanelPlane` contract directly:
  `origin()` is top-left, and resolver math should call
  `PanelPlane::point(anchor)` instead of duplicating anchor-fraction math.
- Phase 3 now says default zero-offset world attachments must work with
  `AnchoredToPanel::new`, and non-zero offsets use the shared `Dimension`
  system rather than a separate anchor-only unit enum.
- Phase 3 now names two implementation risks before coding: diagnostics must
  not share one `current_frame` counter across two resolver runs, and invalid
  world planes need explicit skip reasons and tests.
- Phase 3 now requires an explicit world-panel IME outcome before closeout:
  helper-backed same-frame projection if implemented, or a documented one-frame
  delay if deferred.
- Phase 4 is split into animations without `AnchoredToPanel` first, then
  animations that run while `AnchoredToPanel` is active after Phase 3 adds a
  component the world resolver reads.

### Phase 3 — world-space point anchoring

Status: **complete**.

Implement world-to-world panel attachment using the same `AnchoredToPanel`
relationship:

1. Add a world-space attachment resolver with its own schedule path. Do not put
   world resolution into the screen-space set chain.
2. Before enabling world-to-world edges, make the screen resolver skip world
   source panels and make the world resolver handle them. Screen-to-world and
   world-to-screen attachments remain unsupported in this phase and should
   report a clear diagnostic instead of partially resolving.
3. Extract or share the source-side graph traversal, cycle handling,
   diagnostics, and stale-state clearing before writing the world placement
   solver. If diagnostics are shared, give screen and world resolvers either
   separate diagnostic resources or one explicit frame id; do not advance one
   `current_frame` counter twice in the same `App::update`.
4. Run the resolver in `PostUpdate` before `TransformSystems::Propagate`.
5. Use `TransformHelper::compute_global_transform` or an equivalent parent-chain
   helper so same-frame target and parent transforms are current before writing
   the dependent's local `Transform`.
6. Resolve only world-to-world edges. Screen-to-world and world-to-screen edges
   remain unsupported and should diagnose without panicking.
7. Compute target anchor points with `PanelPlane::from_panel` and
   `PanelPlane::point(anchor)`, using resolved world-meter panel size, not raw
   layout-unit dimensions. Do not duplicate the old anchor-fraction math inside
   the resolver.
8. While an `AnchoredToPanel` relation is active, the resolver owns world
   position and default plane orientation. The default orientation mode is
   "copy the target panel plane."
9. Preserve the dependent panel's chosen `source_anchor` as the point being
   pinned. Do not preserve an arbitrary authored dependent rotation while the
   relation owns orientation.
10. Capture and restore authored local pose with a crate-private record such as
   `AnchoredWorldPanelPose`.
11. Support unparented panels, rotated parents, and uniform-scale parents;
   diagnose non-uniform or sheared parent chains with
   `UnsupportedWorldParentTransform`. Map `PanelAnchorGeometryError::InvalidPanelPlane`
   to an explicit resolver skip reason and cover source-plane and target-plane
   failures in tests.
12. Keep optional post-alignment rotation as a separate component read by the
   world resolver if it is needed. Do not make animation systems fight the
   resolver by writing the same `Transform` after it.
13. Accept zero world offsets from `PanelAnchorOffset::ZERO`, so
    `AnchoredToPanel::new` works for default world attachments. Non-zero world
    offsets use `PanelAnchorOffset::new(x, y)` with bare `f32` or
    `Px`/`Mm`/`Pt`/`In` dimensions.
14. Define world-panel IME behavior before closing the phase. Preferred path:
    world fields that depend on world-anchored panels use helper-backed geometry
    after the world resolver; if that is deferred, add a test and docs that
    state the editor follows those panels one frame later.

This phase should make `examples/panel_anchoring.rs`, planned in
[`panel-anchoring-example.md`](panel-anchoring-example.md), possible for static
and keyboard-driven world anchoring.

#### Retrospective

**What worked:**

- `panel/attachment_resolver.rs` now owns dependency ordering, cycle handling,
  blocked-dependency handling, fallback requests, and bounded diagnostics for
  both screen-space and world-space attachment resolvers.
- `panel/world_anchoring.rs` adds a PostUpdate world resolver using the existing
  `AnchoredToPanel` relation and the Phase 2 `PanelPlane` helpers.
- World and screen sources are now split by source coordinate space: the screen
  resolver handles screen panels, and the world resolver handles world panels.

**What deviated from the plan:**

- The first Phase 3 pass briefly duplicated graph traversal in the world
  resolver. That was corrected before Phase 3 closeout: screen and world
  resolvers now both call the shared attachment resolver.
- No `panel_anchoring.rs` example was added in this phase. The world resolver now
  makes the static world-anchor example possible, but the example remains in
  [`panel-anchoring-example.md`](panel-anchoring-example.md).
- World-panel editor placement was not changed. It still follows the existing
  propagated-`GlobalTransform` path, so same-frame editor tracking for
  world-anchored panels is deferred.

**Surprises:**

- The screen resolver test for a source changing from screen to world had to
  stop expecting a screen diagnostic. With source-owned resolution, that edge is
  no longer a screen failure.
- Parent support is best kept to transforms that can round-trip through Bevy's
  `Transform`: no parent, rotated parent, and uniform-scale parent.

**Implications for remaining phases:**

- Phase 4 examples can use world-to-world `AnchoredToPanel` for exact snapping
  and `PanelAnchorGeometryParam` for animation-only movement.
- If an active world relation needs animation, the next phase should add a
  separate component read by the resolver rather than another system writing the
  same `Transform`.
- Screen and world attachment changes should extend
  `panel/attachment_resolver.rs` when the behavior is graph-level, and stay in
  the coordinate-specific resolver only when the behavior is screen math or
  world transform math.
- A later editor-specific pass is needed if world-anchored editable fields must
  update their popup from the just-resolved PostUpdate pose in the same frame.

#### Phase 3 Review

- Phase 4 now starts by adding `examples/panel_anchoring.rs` for static and
  keyboard-driven world-to-world anchoring before animation demos, and treats it
  as deferred Phase 3 coverage.
- Phase 4 now says active-relation animation must first add a resolver-read
  component such as `PanelAnchorPoseOffset` or `PanelAnchorPostAlignment`.
- Phase 4 now names the PostUpdate scheduling rule for same-frame animation
  inputs and requires before/after resolver tests. The review tightened this:
  Phase 4 must add or choose a named ordering point before adding resolver-read
  animation inputs.
- Phase 4 now treats world editable-field popup tracking as a separate follow-up
  unless a later test or example intentionally touches it, with any same-frame
  world editor work deferred to an editor-specific task.
- The Phase 3 closeout commands now match what shipped: `cargo check`, focused
  `world_anchoring` tests, shared `attachment_resolver` tests, and screen
  anchoring regression tests.
- Phase 3 now has a shared graph/diagnostics resolver with thin screen and world
  placement adapters.
- Phase 4 tests now avoid repeating Phase 3 mixed-space resolver coverage unless
  animation code adds a new mixed-space path.
- `panel-anchoring-example.md` now requires explicit reset/detach behavior for
  `AnchoredWorldPanelPose`.

### Phase 4 — animated anchor consumers

Build animation examples on top of the public anchor geometry rather than
changing the resolver into a physics system. Split this phase into three
passes:

1. **Static and keyboard-driven world anchoring example:** create
   `examples/panel_anchoring.rs` with the Demo 1 behavior from
   [`panel-anchoring-example.md`](panel-anchoring-example.md). This uses
   world-to-world `AnchoredToPanel` directly and proves the Phase 3 resolver in
   an example before adding animation. Treat this as deferred Phase 3 coverage:
   land it before spring, magnetic, elastic, or unwrap demos.
2. **Animations without an active attachment:** spring/magnetic attraction
   between two panels that only read each other's anchor points. These examples
   do not insert `AnchoredToPanel`; they read `PanelAnchorGeometryParam` and
   write `Transform` through `ParamSet`. Do not repeat the Phase 2 smoke test;
   cover visible convergence/separation behavior and an `anchor_animation` test
   filter.
3. **World-resolver animation input:** before any active-relation animation
   demo, add or choose a named `PostUpdate` ordering point for systems that write
   resolver-read animation inputs before the world attachment resolver. If this
   becomes a public `PanelSystems` variant, get explicit API approval during
   that implementation phase.
4. **Animations while a panel is attached:** elastic attach/detach and chained
   unwrapping where `AnchoredToPanel` is active. Start by adding a component
   such as `PanelAnchorPoseOffset` or `PanelAnchorPostAlignment`, make the world
   resolver read it, and test that the pinned `source_anchor` stays fixed while
   the component changes. Land that component and its tests before the
   active-relation animation demo.

The relationship remains the exact snap constraint. Active `AnchoredToPanel`
means the resolver writes the panel placement. Animation systems should use one
of these ownership modes:

1. no `AnchoredToPanel` component while the animation writes `Transform`
2. animation writes a separate component such as offset or post-alignment
   rotation, and the resolver remains the only transform writer
3. animation inserts or removes the relation only at state boundaries

If an animation removes an active world relation and wants to start from the
resolved attached pose, define the handoff explicitly before writing the demo.
Either keep the relation active and animate a component read by the resolver, or
add a same-frame release-pose capture protocol so authored-pose restoration from
Phase 3 does not snap the panel away before the animation starts.

Because the world resolver runs in `PostUpdate` before
`TransformSystems::Propagate`, same-frame animation inputs for an active relation
must be written before the world attachment resolver. Phase 4 must name the set
or system label that callers order against, then add tests showing writes before
the resolver affect the current frame, while writes after the resolver affect the
next frame.

World editable-field popup tracking is not part of Phase 4. If an example or
test touches editable fields on world-anchored panels, document the existing
propagated-transform timing or add a separate editor-specific fix.

Deferred follow-up: if same-frame popup tracking for world-anchored editable
fields becomes required, handle it in an editor-specific task that covers
`ime/editor.rs` after `resolve_world_space_panel_attachments`. Do not mix that
work into `panel_anchoring.rs` or the animation demos.

Hinge animations should consume `PanelAnchorEdge` geometry from Phase 2. Do not
extend `AnchoredToPanel` beyond point-to-point snapping just to model hinge
motion.

Before the hinge demo lands, define a component such as `PanelAnchorPoseOffset`
or `PanelAnchorPostAlignment` that the world resolver reads. It should compose
local rotation after target-plane alignment and then translate so the pinned
`source_anchor` remains fixed. If the demo needs edge-like behavior, model it as
`BottomCenter` / `TopCenter` point snapping plus equal-width, edge-parallel
visual hinge motion, not as a general edge constraint in `AnchoredToPanel`.

## Tests

### Phase 1 tests

- reverse relationship index is maintained when inserting, replacing, and
  removing `AnchoredToPanel`
- target despawn detaches dependents without despawning them
- `<AnchoredToPanel as Relationship>::from(target)` matches the public
  constructor defaults
- `AnchoredToPanel`, `PanelAnchorOffset`, and `PanelsAnchoredHere` are present
  in `AppTypeRegistry`
- `AnchoredToPanel` and `PanelsAnchoredHere` do not expose `ReflectComponent`
  type data; reflected patching cannot mutate target, anchors, offset, or the
  reverse index
- first-frame `Fit` target and dependent resolve before screen transform
- title-bar case places dependent top-left one pixel below target bottom-left
- table-driven anchor math uses literal expected screen coordinates and final
  `Transform` values, not only the shared helper
- literal-coordinate cases include top-left to bottom-left, center to center
  with offset, and bottom-right to top-right
- `source_anchor` can differ from `panel.anchor()`
- target resize repositions dependent in the same update
- target `ScreenPosition::At` movement repositions dependent
- window resize repositions `ScreenPosition::Screen` targets and dependents
- removing `AnchoredToPanel` returns dependent to configured placement
- source coordinate-space transition clears or ignores stale screen override
- explicit dimension offsets such as `Pt(12.0)` resolve to target-panel layout
  units before placement
- observer queued `AnchoredToPanel` insertion after `PanelDimensionsChanged`
  takes effect before attachment resolution in the same frame
- relation writes applied before `PanelSystems::ResolvePanelAttachments` affect
  the current frame, while writes after the resolver affect the next frame
- `WindowRef::Primary` and `WindowRef::Entity(primary)` are treated as the same
  window
- missing explicit window, missing primary window, and zero-sized window skip
  without panic
- cross-window attachments and unsupported screen-to-world / world-to-screen
  attachments are skipped without panic
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
- spawn-frame `Added<ResolvedScreenPanelPosition>` is not treated as resolver
  placement; tests assert actual `None -> Some -> None` value transitions
- `PanelDimensionsChanged` observer can queue `AnchoredToPanel` insertion with
  `Commands`; source-side relation state and final transform resolve in that
  same `App::update`
- diagnostics history coalesces repeated invalid edges, tracks
  `count` / `last_seen_frame`, clears current-frame issues on recovery, stores
  separate reasons for the same edge, and evicts past capacity

### Phase 2 tests

- screen `ResolvedPanelAnchorGeometry::point` returns literal coordinates for
  all nine anchors
- screen `edge` returns the expected endpoints for all four edges
- helper-backed screen geometry is current after
  `PanelSystems::PositionScreenSpace`; cached screen geometry is current after
  `PanelSystems::PublishAnchorGeometry` if Phase 2 adds that cache writer set
- geometry cache components, if any, have private fields and no
  `ReflectComponent` mutation
- helper-backed geometry can read the documented current value when a target
  moved earlier in the same frame
- IME/editor same-frame screen-panel read regression covers an attached panel
  moving in the same frame
- newly attached screen panel with text/SDF children renders at the resolved
  transform in the same update, or any intentional one-frame delay is explicitly
  documented by the test
- world `PanelPlane` basis directions are unit and orthonormal, with size in
  resolved meters
- invalid window and unresolved-panel states return `None` or the documented
  error
- a consumer can animate toward an anchor point without `AnchoredToPanel`

### Phase 3 tests

- same-frame world anchor placement works when the target moves before the
  resolver
- target rotation moves and orients the dependent in the target plane
- world anchor math uses resolved meter dimensions for `Mm`, `Pt`,
  `world_width`, `world_height`, non-center anchors, and source scale
- dependent local transform is correct under unparented, rotated-parent, and
  uniform-scale-parent cases
- non-uniform or sheared parent chains skip with
  `UnsupportedWorldParentTransform`
- removal, invalid target, skipped dependency, and cycle fallback restore the
  captured authored local pose
- chained world attachments resolve in one update
- unsupported screen-to-world and world-to-screen edges are skipped and
  diagnosed
- world cycles and cycle descendants fall back consistently
- the resolver runs before transform propagation and produces current
  `GlobalTransform` after propagation

### Phase 4 tests / examples

- elastic pair animation consumes anchor point geometry and has no active
  `AnchoredToPanel` relation while it owns transforms
- `panel_anchoring.rs` includes the static and keyboard-driven world-to-world
  anchoring demo before active-relation animation examples are added
- the static `panel_anchoring.rs` pass documents reset and detach behavior for
  `AnchoredWorldPanelPose`: whether reset removes the relation, refreshes the
  captured authored pose, or leaves the resolver-owned pose intact
- Phase 4 adds or names a `PostUpdate` ordering point for world-resolver
  animation inputs before adding `PanelAnchorPoseOffset` /
  `PanelAnchorPostAlignment`
- post-alignment rotation animation, if added, is consumed by the resolver
  rather than writing the same transform after the resolver
- writes to `PanelAnchorPoseOffset` / `PanelAnchorPostAlignment` before the
  world resolver affect the current frame; writes after it affect the next frame
- `PanelAnchorPoseOffset` / `PanelAnchorPostAlignment` keeps the pinned
  `source_anchor` fixed while composing local rotation
- chained unwrap consumes edge geometry and does not require extending
  `AnchoredToPanel` beyond point snapping
- mixed screen/world animation tests are only needed for animation-specific
  paths; do not duplicate the base resolver ownership tests from Phase 3

### Phase Closeout

Phase 1 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo check -p bevy_diegetic --example diegetic_text_stress
cargo nextest run -p bevy_diegetic panel::anchoring
cargo nextest run -p bevy_diegetic screen_space::anchoring
```

Phase 2 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic panel::anchor_geometry
cargo nextest run -p bevy_diegetic anchor_geometry_consumer
```

Phase 3 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo check -p bevy_diegetic
cargo nextest run -p bevy_diegetic world_anchoring
cargo nextest run -p bevy_diegetic attachment_resolver
cargo nextest run -p bevy_diegetic screen_space::anchoring
cargo nextest run -p bevy_diegetic
```

Phase 4 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
```

## Team Review Notes

This section records the implementation-readiness `team_review 2` findings for
this implementation plan.

### Cycle 1

Recorded in the plan:

- `PanelAnchorOffset` uses the same authored `Dimension` units as panel and
  text sizing.
- The public field name is `source_anchor`, avoiding ambiguity with
  `panel.anchor()`.
- Phase 1 reflection is type-registration-only for relationship components;
  retargeting remains replacement-based.
- Relationship source immutability is an API invariant, not an implementation
  detail.
- The resolver scans candidates before active-graph filtering so skipped edges
  still produce precise diagnostics.
- Candidate state is explicit: configured, resolved, or skipped with a reason.
- Skipped sources block downstream resolution with
  `BlockedBySkippedDependency`, while cycles use cycle-specific reasons.
- Future screen and world resolvers own edges by source coordinate space.
- Diagnostics have bounded history plus current-frame state.
- The schedule includes two explicit `ApplyDeferred` barriers so observer
  commands and relationship hooks are visible before attachment resolution.
- Same-frame screen `GlobalTransform` consumers need an audit before the first
  consumer migration.
- Phase 2 defines a concrete read surface for anchor points, screen bounds,
  world planes, and edges.
- Phase 3 has a no-lag world schedule and signed panel-plane math.
- World anchoring owns placement and default plane orientation while active.
- Phase 4 animation consumers must not write the same `Transform` as the
  resolver.
- Hinge examples consume edge geometry rather than expanding
  `AnchoredToPanel`.
- Phase 1 is split into reviewable implementation checkpoints.
- Tests are split by phase and include relationship lifecycle, schedule,
  diagnostics, geometry, world anchoring, and animation ownership coverage.
- [`panel-anchoring-example.md`](panel-anchoring-example.md) is the example plan
  for the world/animation demos.

Cycle 1 surfaced no premise challenge and no unresolved user decision.

### Cycle 2

Recorded in the plan:

- `AnchoredToPanel` is explicitly `#[component(immutable)]`, so
  replacement-only retargeting is enforceable.
- `PanelAnchorOffset` stores `Dimension` values; screen and world resolvers
  convert those values through the target panel's layout unit before placement.
- Relationship mutation timing is part of the public schedule contract:
  relation changes applied before `ResolvePanelAttachments` affect the current
  frame, and later changes affect the next frame.
- The flush contract no longer requires same-frame reverse-index visibility for
  resolver correctness.
- Skip reasons include missing source/target, transient self-attachment, and
  unsupported world parent transforms.
- Required `ResolvedScreenPanelPosition` tests distinguish spawn-frame `Added`
  from real `None -> Some -> None` resolver changes.
- Diagnostics tests now cover coalescing, counts, last-seen frames, recovery,
  separate reasons, and capacity eviction.
- The screen `GlobalTransform` audit now has concrete outcomes for IME/editor
  consumers and post-update panel-child/render-prep ordering.
- World anchor math uses `PanelPlane::from_panel` and resolved meter dimensions,
  not raw layout-unit width/height.
- World anchoring captures and restores authored local pose when the active
  relation is removed or skipped.
- Phase 3 defines supported parent chains and diagnoses non-uniform/sheared
  parent transforms.
- Public anchor geometry is read-only by construction, with a named publish set
  and helper-backed same-frame consumer mode.
- Phase 4 defines a post-alignment or pose-offset component read by the world
  resolver before hinge animation lands.
- Tests and closeout commands are split by phase with stable planned filters.
- [`panel-anchoring-example.md`](panel-anchoring-example.md) now starts at
  Phase 3, uses point snapping plus edge geometry, and has staged closeout.

Cycle 2 surfaced no premise challenge and no unresolved user decision.
