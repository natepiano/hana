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
crates/bevy_diegetic/src/screen_space/anchoring.rs
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

Use a named offset type instead of a raw `Vec2` so the public API preserves the
unit contract before world-space anchoring ships:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Default)]
pub struct PanelAnchorOffset {
    value: Vec2,
    units: PanelAnchorOffsetUnits,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(PartialEq, Debug, Default)]
pub enum PanelAnchorOffsetUnits {
    #[default]
    ScreenPixels,
    TargetPlaneMeters,
}

impl PanelAnchorOffset {
    pub const ZERO: Self;

    pub const fn screen_pixels(offset: Vec2) -> Self;
    pub const fn target_plane_meters(offset: Vec2) -> Self;
    pub const fn units(self) -> PanelAnchorOffsetUnits;
    pub const fn as_vec2(self) -> Vec2;
}
```

The resolver must validate the stored unit tag against the source panel's
coordinate space:

- screen-space: logical pixels, top-left origin, y down
- world-space: meters in the target panel plane, x right and y up

Phase 1 screen-space anchoring accepts `ScreenPixels` offsets, including
`PanelAnchorOffset::ZERO`. If a `TargetPlaneMeters` offset is constructible
before world-space anchoring ships, the screen resolver should skip the edge
with `OffsetUnitsMismatch` instead of silently interpreting meters as pixels.

`with_offset(PanelAnchorOffset)` should be the typed constructor.
`with_screen_offset(Vec2)` can provide the ergonomic Phase 1 screen-space
shortcut while call sites still document the unit.

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

    pub const fn with_screen_offset(mut self, offset: Vec2) -> Self;

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
    .with_screen_offset(Vec2::new(0.0, 1.0))
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
    target_bounds.point(attachment.target_anchor) + attachment.offset.as_vec2();
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

When Phase 3 adds a world resolver, resolver ownership is by source coordinate
space:

- screen sources are classified and resolved only by the screen resolver
- world sources are classified and resolved only by the world resolver
- mixed-space edges are diagnosed by the source resolver
- each resolver clears only the private override state it owns

Dead target and target despawn should normally be handled by Bevy relationship
hooks: dependents detach, but do not despawn. The resolver still needs precise
fallback reasons for transient same-frame states, hook timing, and manually
authored invalid relationships. Resolver-owned inactive cases are missing source
panel data, missing target entity, existing targets that are not panels,
self-attachment observed before cleanup, targets in another coordinate space,
targets in another resolved window, targets whose window is missing or
zero-sized, offset-unit mismatch, and nodes blocked by cycles. These cases
should not panic.

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
    OffsetUnitsMismatch,
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
layout-unit `panel.width()` / `panel.height()`. Define one shared plane helper:

```rust
impl PanelPlane {
    pub(crate) fn from_panel(panel: &DiegeticPanel, global: GlobalTransform) -> Self;
}
```

`PanelPlane::from_panel` uses `panel.world_width()` / `panel.world_height()` or
the equivalent resolved meter dimensions. Its invariants are:

- `origin` is the world-space point for the panel's configured `panel.anchor()`
- `right`, `up`, and `normal` are unit orthonormal world directions
- `size` is the resolved visual width/height in world meters

World anchor coordinates use signed panel-plane coordinates. If `fx/fy` come
from `Anchor::offset_fraction()` and the panel's configured anchor is
`panel.anchor()`, then a point on the panel plane is:

```rust
let (anchor_fx, anchor_fy) = anchor.offset_fraction();
let (panel_fx, panel_fy) = panel.anchor().offset_fraction();

let offset = Vec2::new(
    (anchor_fx - panel_fx) * plane.size.x,
    (panel_fy - anchor_fy) * plane.size.y,
);

let point_world = plane.origin + plane.right * offset.x + plane.up * offset.y;
```

The x axis points right. The y axis points up in world panel space, so top
anchors have positive local y when the panel anchor is centered. This differs
from screen-space offset y, which points down.

The target anchor point applies the tagged meter offset in the target panel
plane:

```rust
let target_point_world = target_plane.point(target_anchor)
    + target_plane.right * offset_meters.x
    + target_plane.up * offset_meters.y;
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
directly. Phase 3 should add a crate-private resolver-owned record such as
`AnchoredWorldPanelPose` that captures the dependent's authored local transform
when the relation first becomes active. On relation removal, invalid target,
skipped dependency, or cycle fallback, restore the authored transform and clear
the resolver-owned record. While the relation is active, placement and default
plane orientation are resolver-owned; user-authored transform edits should be
made by removing the relation or by writing a resolver-owned offset/rotation
input.

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

- Phase 2 adds `PanelSystems::PublishAnchorGeometry` as the writer set for any
  cached geometry.
- screen geometry is current after `PanelSystems::PublishAnchorGeometry` when it
  runs after `PanelSystems::PositionScreenSpace`
- cached world geometry is current after Bevy transform propagation and the
  world geometry publisher
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

Keep Phase 1 small enough to implement and review in checkpoints.

#### Phase 1A — relationship API and lifecycle

1. Add `panel/anchoring.rs` with `AnchoredToPanel`, `PanelAnchorOffset`,
   `PanelsAnchoredHere`, constructors, and read-only reverse-index helpers.
   `AnchoredToPanel` is `#[component(immutable)]`, and `PanelAnchorOffset`
   stores both value and units.
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
2. Split screen-space anchoring code into `screen_space/anchoring.rs`.
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
7. Validate offset units before resolving an edge.
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

### Phase 2 — public anchor geometry reads

Expose read-only resolved anchor geometry without changing attachment behavior.
This phase should define the stable API that animation systems can consume:

1. Add a public read surface equivalent to `ResolvedPanelAnchorGeometry`,
   `PanelAnchorPoints`, `PanelScreenBounds`, `PanelPlane`, and
   `PanelAnchorEdge`.
2. Prefer `PanelAnchorGeometryParam`; if a cache component is needed, keep it
   library-owned with private fields and no `ReflectComponent` mutation.
3. Add `PanelSystems::PublishAnchorGeometry` for cached geometry writes.
4. Provide `point(anchor)` and `edge(edge)` helpers for screen bounds and world
   planes.
5. Publish screen-space anchor geometry in logical pixels with top-left origin
   and y down.
6. Publish world-space anchor geometry in world meters with an explicit unit
   orthonormal plane basis and resolved meter extents.
7. Define the freshness contract in the API docs:
   - cached screen geometry is current after `PanelSystems::PublishAnchorGeometry`
     following `PanelSystems::PositionScreenSpace`
   - cached world geometry is current after transform propagation and the world
     geometry publisher
   - same-frame transform writers use helper-backed current geometry rather than
     cached post-propagation snapshots
8. Return `None` or an explicit error for missing windows, unresolved panels,
   zero-sized panels, and invalid panel state; do not synthesize geometry from
   stale transforms.
9. Keep this phase read-only: no transform writes and no changes to
   `AnchoredToPanel` behavior.

The first consumer should be an example or test that animates an entity toward a
panel anchor point without using `AnchoredToPanel` as the mover. This phase must
also cover edge geometry because the chained unwrap example needs hinge edges,
not only point centers.

### Phase 3 — world-space point anchoring

Implement world-to-world panel attachment using the same `AnchoredToPanel`
relationship:

1. Add a world-space attachment resolver with its own schedule path. Do not put
   world resolution into the screen-space set chain.
2. Run the resolver in `PostUpdate` before `TransformSystems::Propagate`.
3. Use `TransformHelper::compute_global_transform` or an equivalent parent-chain
   helper so same-frame target and parent transforms are current before writing
   the dependent's local `Transform`.
4. Resolve only world-to-world source edges. Mixed screen/world edges are
   diagnosed by the source-space resolver and do not panic.
5. Compute target anchor points from `PanelPlane::from_panel`, using resolved
   world-meter panel size, not raw layout-unit dimensions.
6. While an `AnchoredToPanel` relation is active, the resolver owns world
   position and default plane orientation. The default orientation mode is
   "copy the target panel plane."
7. Preserve the dependent panel's chosen `source_anchor` as the point being
   pinned. Do not preserve an arbitrary authored dependent rotation while the
   relation owns orientation.
8. Capture and restore authored local pose with a resolver-owned record such as
   `AnchoredWorldPanelPose`.
9. Support unparented panels, rotated parents, and uniform-scale parents;
   diagnose non-uniform or sheared parent chains with
   `UnsupportedWorldParentTransform`.
10. Reuse the source-owned graph, candidate scan, cycle handling, diagnostics,
   and stale-state clearing from the screen-space resolver.
11. Keep optional post-alignment rotation as a separate resolver input if it is
   needed. Do not make animation systems fight the resolver by writing the same
   `Transform` after it.

This phase should make `examples/panel_anchoring.rs`, planned in
[`panel-anchoring-example.md`](panel-anchoring-example.md), possible for static
and keyboard-driven world anchoring.

### Phase 4 — animated anchor consumers

Build animation examples on top of the public anchor geometry rather than
changing the resolver into a physics system:

- spring/magnetic attraction between two panels using resolved anchor points
- elastic easing between attached and detached positions
- chained panel unwrapping where anchor points define hinge edges

The relationship remains the exact snap constraint. Active `AnchoredToPanel`
means resolver-owned placement. Animation systems should use one of these
ownership modes:

1. no active relation while the animation writes `Transform`
2. animation writes a separate resolver input such as offset or post-alignment
   rotation, and the resolver remains the only transform writer
3. animation inserts or removes the relation only at state boundaries

Hinge animations should consume `PanelAnchorEdge` geometry from Phase 2. Do not
extend `AnchoredToPanel` beyond point-to-point snapping just to model hinge
motion.

Before the hinge demo lands, define a resolver-owned input such as
`PanelAnchorPoseOffset` or `PanelAnchorPostAlignment`. It should compose local
rotation after target-plane alignment and then translate so the pinned
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
- `TargetPlaneMeters` offset on a screen source skips with `OffsetUnitsMismatch`
- observer queued `AnchoredToPanel` insertion after `PanelDimensionsChanged`
  takes effect before attachment resolution in the same frame
- relation writes applied before `PanelSystems::ResolvePanelAttachments` affect
  the current frame, while writes after the resolver affect the next frame
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
- spawn-frame `Added<ResolvedScreenPanelPosition>` is not treated as resolver
  placement; tests assert actual `None -> Some -> None` value transitions
- `PanelDimensionsChanged` observer can queue `AnchoredToPanel` insertion with
  `Commands`; source-side relation state and final transform resolve in that
  same `App::update`
- diagnostics history coalesces repeated invalid edges, tracks
  `count` / `last_seen_frame`, clears current-frame issues on recovery, stores
  separate reasons for the same edge, and evicts past capacity
- IME/editor same-frame screen-panel read regression covers an attached panel
  moving in the same frame
- newly attached screen panel with text/SDF children renders at the resolved
  transform in the same update, or any intentional one-frame delay is explicitly
  documented by the test

### Phase 2 tests

- screen `ResolvedPanelAnchorGeometry::point` returns literal coordinates for
  all nine anchors
- screen `edge` returns the expected endpoints for all four edges
- screen geometry is current after `PanelSystems::PublishAnchorGeometry`
  following `PanelSystems::PositionScreenSpace`
- geometry cache components, if any, have private fields and no
  `ReflectComponent` mutation
- helper-backed geometry can read the documented current value when a target
  moved earlier in the same frame
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
- mixed screen/world edges are skipped and diagnosed by the source resolver
- world cycles and cycle descendants fall back consistently
- the resolver runs before transform propagation and produces current
  `GlobalTransform` after propagation

### Phase 4 tests / examples

- elastic pair animation consumes anchor point geometry and has no active
  `AnchoredToPanel` relation while it owns transforms
- post-alignment rotation animation, if added, is consumed by the resolver
  rather than writing the same transform after the resolver
- `PanelAnchorPoseOffset` / `PanelAnchorPostAlignment` keeps the pinned
  `source_anchor` fixed while composing local rotation
- chained unwrap consumes edge geometry and does not require extending
  `AnchoredToPanel` beyond point snapping

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
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic world_anchoring
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

- `PanelAnchorOffset` makes offset units explicit before world anchoring ships.
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
- Phase 4 animation consumers must not fight resolver-owned transform writes.
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
- `PanelAnchorOffset` stores a unit tag; screen and world resolvers validate
  offset units instead of relying on constructor naming alone.
- Relationship mutation timing is part of the public schedule contract:
  relation changes applied before `ResolvePanelAttachments` affect the current
  frame, and later changes affect the next frame.
- The flush contract no longer requires same-frame reverse-index visibility for
  resolver correctness.
- Skip reasons include missing source/target, transient self-attachment,
  offset-unit mismatch, and unsupported world parent transforms.
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
- Phase 4 has an explicit resolver-owned post-alignment/pose-offset input before
  hinge animation lands.
- Tests and closeout commands are split by phase with stable planned filters.
- [`panel-anchoring-example.md`](panel-anchoring-example.md) now starts at
  Phase 3, uses point snapping plus edge geometry, and has staged closeout.

Cycle 2 surfaced no premise challenge and no unresolved user decision.
