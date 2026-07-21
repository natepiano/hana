# hana_valence anchoring and arrangements

## What it is

`hana_valence` is the shape-agnostic anchoring layer for Bevy assemblies.
Geometry providers publish local anchor points and ordered edges on each entity;
`AnchoredTo` connects one entity's anchor to another; animation systems drive
`AnchorPose` or `Hinge`; and `resolve_anchors` converts that data into local
`Transform` values in dependency order. The same primitives support direct
entity bonds, linear tiling arrangements, and `hana_diegetic` panel anchoring
without putting panel units, window coordinates, or shape-specific vocabulary
into the core resolver.

## Architecture and data flow

```text
geometry provider
    -> ResolvedAnchorGeometry

authoring
    -> AnchoredTo
    -> optional ResolvedAnchorOffset

animation / arrangement driver
    -> AnchorPose or Hinge.angle
    -> hinge_to_pose
    -> AnchorPose

resolve_anchors
    -> dependency classification and ordering
    -> local Transform
    -> optional ResolvedAnchorWorld cache

TransformSystems::Propagate
    -> GlobalTransform
```

Consumers configure the anchoring pipeline themselves:

```text
AnchorSystems::FillGeometry
    -> AnchorSystems::AnimatePose
    -> AnchorSystems::Resolve
    -> TransformSystems::Propagate
```

`ResolveDiagnostics` must be initialized before `resolve_anchors` runs. The
core anchoring and arrangement systems are registered by the consumer rather
than by an anchoring plugin.

### Geometry contract

[`geometry.rs`](../../../crates/hana_valence/src/geometry.rs) defines the
provider-facing contract:

- `AnchorId::{Vertex(u32), EdgeMid(u32), Center}` is the stable,
  non-exhaustive anchor identifier.
- `AnchorPoint { position: Vec3, frame: Option<Quat> }` stores a point in the
  entity's local authored frame. `rotation()` is the single identity fallback
  for absent tangent frames.
- `Edge { start: AnchorId, end: AnchorId }` is ordered.
  `Edge::axis(&ResolvedAnchorGeometry) -> Result<Dir3, EdgeAxisError>` points
  from `start` to `end`, so reversing the endpoints reverses hinge direction.
- `ResolvedAnchorGeometry { points: HashMap<AnchorId, AnchorPoint>, edges:
  Vec<Edge> }` is one component per geometry-bearing entity.
- `ResolvedAnchorGeometry::validate() -> Result<(), GeometryError>` rejects
  non-finite points, missing endpoints, near-degenerate edges, and edges whose
  endpoint frames differ.

Providers emit local points in authored units. They do not bake `Transform` or
`GlobalTransform` into the geometry.

### Relationship and pose data

[`relation.rs`](../../../crates/hana_valence/src/relation.rs) defines the
relationship pair:

```rust
AnchoredTo::new(target, source_anchor, target_anchor)
    .with_offset(offset)
```

`AnchoredTo` is immutable. Retargeting uses `retargeted(target)` and replaces
the full component so Bevy's relationship hooks update `AnchoredHere`
correctly. `AnchoredHere` retains source entities in insertion order and
exposes slice-like iteration, `len`, and `is_empty`.

The relationship pair is reflected as types but deliberately does not expose
`ReflectComponent`. Reflective insertion or mutation would bypass the
relationship hooks and corrupt the reverse index. Mutable resolver inputs such
as `ResolvedAnchorOffset`, `AnchorPose`, `Hinge`, and
`ResolvedAnchorGeometry` do expose `ReflectComponent`.

`ResolvedAnchorOffset(Vec3)` is the live override for `AnchoredTo::offset`;
when present, the resolver always prefers it.

[`pose.rs`](../../../crates/hana_valence/src/pose.rs) defines:

- `AnchorPose { rotation: Quat, translation: Vec3 }`, the animation seam
  consumed by the resolver.
- `ResolvedAnchorWorld { points: HashMap<AnchorId, Vec3> }`, an opt-in cache
  refreshed on every resolve pass for entities carrying it.
- `AnchorSystems::{FillGeometry, AnimatePose, Resolve}`, the consumer-owned
  scheduling contract.

`AnchorPose` remains separate from `Transform` so animation and placement do
not compete to write one component with two meanings.

### Dependency ordering and diagnostics

[`attachment.rs`](../../../crates/hana_valence/src/attachment.rs) contains the
reusable graph layer:

```rust
resolve_attachments(
    candidates: Vec<AttachmentResolveCandidate<R>>,
    reasons: AttachmentResolveReasons<R>,
    diagnostics: &mut AttachmentResolveDiagnostics<R>,
    handle: impl FnMut(AttachmentResolveAction) -> Result<(), R>,
)
```

A coordinate-space adapter first classifies every edge as `Active` or
`Skipped`. The graph layer then:

- resolves targets before their dependents;
- routes valid edges through `AttachmentResolveAction::Place`;
- routes skipped or failed edges through `Fallback`;
- distinguishes cycles from entities merely blocked by a cycle;
- propagates skipped-dependency failures down the graph;
- accumulates bounded diagnostics.

`AttachmentResolveDiagnostics<R>` retains 128 entries by default. Entries are
keyed by source, target, and reason, retain first/last frame plus occurrence
count, and emit a tracing warning when the same skip repeats.

This layer owns ordering and failure propagation, not geometry validation or
coordinate-space placement.

### Anchor resolution

[`resolve.rs`](../../../crates/hana_valence/src/resolve.rs) classifies every
stored `AnchoredTo` and calls the generic attachment resolver. It checks entity
liveness, geometry, local/global transforms, source scale finiteness, and
supported parent transforms before placement.

For a source anchored to a target, the resolver computes:

```text
target_world = target.global * target_point.position
base         = target.global.rotation * target_point.rotation()
rotation     = base * pose.rotation * source_point.rotation().inverse()
offset       = ResolvedAnchorOffset.unwrap_or(AnchoredTo.offset)

translation  = target_world
             + base * (offset + pose.translation)
             - rotation * (source_global_scale * source_point.position)
```

It writes the resulting global placement back as a local `Transform`,
accounting for the source's transform parent. A per-pass `resolved_globals` map
carries newly resolved globals through deep chains, allowing an entire tree to
settle in one pass before normal transform propagation.

`ResolveSkip` records missing geometry or transforms, missing anchors,
despawned targets, invalid source scale, unsupported parents, skipped
dependencies, and cycles. Fallback leaves the source's current transform
unchanged.

`resolve_anchors` is the sole `Transform` writer for entities carrying
`AnchoredTo`. Drivers write `AnchorPose`, `Hinge`, or the transform of
unanchored entities.

### Hinges and animation adapters

[`hinge.rs`](../../../crates/hana_valence/src/hinge.rs) defines:

```rust
pub struct Hinge {
    pub edge: Edge,
    pub angle: f32,
}

impl Hinge {
    pub fn rotation(
        &self,
        geometry: &ResolvedAnchorGeometry,
    ) -> Result<Quat, EdgeAxisError>
}
```

`hinge_to_pose` converts the edge axis and scalar angle into `AnchorPose`. A bad
edge or non-finite result skips only that entity and warns; it does not abort
the system.

Without an optional `HingePivot`, `hinge_to_pose` writes the whole pose with
zero translation. With a pivot, it writes the compensating translation needed
to keep the external pivot line fixed. In either case, `Hinge` owns the entire
`AnchorPose` while present. Direct pose animation and hinge animation are
mutually exclusive on one entity; debug builds warn when an earlier same-frame
pose write is overwritten.

With the default `tween` feature,
[`tween.rs`](../../../crates/hana_valence/src/tween.rs) exports:

- `HingeAngleLens`, which interpolates `Hinge::angle`;
- `AnchorPoseLens`, which slerps pose rotation and lerps pose translation.

`TweenSystemSet::ApplyTween` must run inside `AnchorSystems::AnimatePose`,
before `hinge_to_pose`. Moving that set is app-global and retimes every tween
in the app.

### Arrangements

[`arrange.rs`](../../../crates/hana_valence/src/arrange.rs) builds ordered
linear assemblies on top of the anchoring primitives.

An arrangement entity carries:

- exactly one arrangement component: `Strip`, `Accordion { fold, lean }`, or
  `Coil { fold, lean }`;
- a geometry-specific rule component such as `QuadTiling`;
- an internally maintained `ArrangementMembers`.

Members carry `Member { arrangement }`. The add observer and
`assign_member_indices` assign stable, insertion-ordered `MemberIndex` values
and mark new members with `PendingMemberPlacement`.

The shape extension point is:

```rust
pub trait TilingRule {
    fn next_edge(&self, index: usize) -> (Edge, Edge);
    fn edge_anchor(&self, edge: Edge) -> Option<AnchorId>;
    fn placement(
        &self,
        target: Entity,
        index: usize,
    ) -> Option<ArrangementPlacement>;
    fn rest_delta(&self, index: usize) -> f32;
}
```

`placement` has a default implementation that converts the shared source and
target edges into an `AnchoredTo`, hinge edge, and rest angle. `QuadTiling`
supplies the built-in straight quad rule. Other shapes implement the same
contract; the triangle example demonstrates this with a rule component defined
outside the crate.

The public drivers are generic per rule component:

```rust
apply_member_placements::<R>()
drive_arrangement_hinges::<R>()
```

Each consumer must register one instantiation for every rule type it uses.
`apply_member_placements` waits until source and target geometry and transforms
exist, then inserts `AnchoredTo`, `AnchorPose`, and `Hinge`.
`drive_arrangement_hinges` writes the live hinge angle from the rule's rest
angle and the arrangement parameters. `Strip` contributes no fold angle.
`Accordion` clamps `fold` to `0..=1` and alternates adjacent hinge signs.
`Coil` also clamps `fold` to `0..=1`, but gives every hinge the same sign so
world rotations accumulate down the member set.

`ArrangementMembers` is the authoritative linear order, not `AnchoredHere`.
The first member targets the arrangement entity, and each later member targets
its predecessor. It has no public constructor and is maintained through the
member observers. Branching nets are not linear arrangements; they author their
`AnchoredTo` and `Hinge` tree directly.

### hana_diegetic integration

`hana_diegetic` composes with `hana_valence` while retaining panel-specific
authoring and coordinate math.

[`panel/valence_provider.rs`](../../../crates/hana_diegetic/src/panel/valence_provider.rs)
maps the nine panel `Anchor` values to quad `AnchorId` values and fills
world-panel geometry with nine local points and four ordered perimeter edges.
The fill system runs on `Changed<DiegeticPanel>`, never `Changed<Transform>`,
and mutates existing geometry in place to retain allocations.

[`panel/anchoring.rs`](../../../crates/hana_diegetic/src/panel/anchoring.rs)
supports attachment methods on `DiegeticPanelCommands` for Bevy `Commands`. Callers obtain checked
`PanelEntity<World>` / `PanelEntity<Screen>` handles and matching typed panel or
widget targets before attaching, retargeting, or detaching. The public
`PanelAttachment` value carries the two anchors and `PanelAnchorOffset`; the
internal `PanelAttachmentAuthored` record is consumed by two positioners:

- World panels receive a stored `hana_valence::AnchoredTo`, a per-frame lowered
  `ResolvedAnchorOffset`, and placement through `resolve_anchors`.
- Screen panels keep only the shared authoring and are placed against screen
  panels or reified screen widgets by the screen-space resolver. They do not
  carry the world relation.

Checked world/screen conversion refuses any panel that is an attachment source,
is targeted by another panel, or owns a targeted widget. Callers detach the
affected placements and queue conversions in order on the same `Commands`
value, then reacquire destination-space handles after the command fence and
reattach. World attachment insertion captures
the authored local transform, and removing the attachment restores that
transform.

World offset lowering resolves `PanelAnchorOffset` units against the live
target size and transform, then converts panel-local positive-down y to
resolver-local positive-up y. Unit conversion, DPI handling, target-relative
sizing, and this y sign change remain outside `hana_valence`.

The screen path in
[`screen_space/anchoring`](../../../crates/hana_diegetic/src/screen_space/anchoring)
uses `PanelAttachmentAuthored` to classify candidates, delegates graph ordering
and diagnostics to `hana_valence::resolve_attachments`, and performs
viewport/window placement in its own callback. Reified widget targets contribute
a private widget-to-owner dependency so the owner is placed before the widget
and its dependent. The path supports in-plane `AnchorPose` rotation and
translation without inserting a world `AnchoredTo`.

[`panel/arrangement.rs`](../../../crates/hana_diegetic/src/panel/arrangement.rs)
exposes the insert-only `ArrangedPanel` wrapper and adapts `QuadTiling`
placement for panel members. Connected screen arrangements screen-attach only
their root; member panels remain in the shared world fold frame and connect to
predecessors with raw valence relations.

`HeadlessLayoutPlugin` installs the panel provider, observers, offset lowering,
quad arrangement drivers, `hinge_to_pose`, `resolve_anchors`, and
`ResolveDiagnostics`. `ScreenSpacePlugin` separately installs screen attachment
diagnostics and placement.

## Invariants

- Provider geometry is entity-local, uses authored units, and contains no
  baked transform.
- Providers validate geometry before relying on it; ordered edge endpoints
  define fold sign.
- `AnchoredTo` is replaced, never partially retargeted, so `AnchoredHere`
  remains correct.
- Relationship components do not expose `ReflectComponent`; mutable resolver
  inputs do.
- `resolve_anchors` is the only transform writer for anchored entities.
- Animation inputs land in `AnchorSystems::AnimatePose`; resolution runs
  afterward and transform propagation runs last.
- A schedule running `resolve_anchors` initializes `ResolveDiagnostics`.
- `ResolvedAnchorOffset` overrides the static relation offset without
  reinserting the immutable relation.
- World and screen panel attachments share authoring but not their positioner
  tag. Screen panels must not carry the world `AnchoredTo`.
- Panel geometry fill is driven by panel-data changes, not transform changes.
- `ArrangementMembers` preserves insertion order and does not assume
  contiguous surviving indices.
- Arrangement members defer placement until their geometry and transforms,
  and those of the predecessor, are ready.
- A rule component lives on the arrangement entity, and each rule type gets
  its own generic driver-system registration.
- An arrangement entity carries exactly one of `Strip`, `Accordion`, or `Coil`.
- Linear arrangements do not model branching nets; tree topologies author one
  parent relation and hinge per non-root entity.
- A hinged entity already has `AnchorPose`; `hinge_to_pose` does not insert it.
- Direct `AnchorPose` animation and `Hinge` ownership are mutually exclusive.

## Calibration and gotchas

- Edges shorter than `1e-4` authored units are degenerate.
- Parent transform validation uses `1e-4` orthogonality and uniform-scale
  tolerances and rejects zero scale, non-uniform scale, shear, and reflections.
- Non-uniform child scale is documented as unsupported even though the
  source-anchor term uses component-wise global scale.
- Resolver reads of `GlobalTransform` occur before propagation. Newly resolved
  chains are coherent inside the resolver, but external same-frame global reads
  remain stale until `TransformSystems::Propagate`.
- `ResolvedAnchorWorld` has the same freshness boundary as the resolve pass; it
  is not a post-propagation cache.
- Persistent skips warn every frame after the first occurrence. Diagnostics are
  bounded, but warning output is not throttled.
- Geometry fill for a non-world panel does nothing; correctness depends on
  removing the world relation during `PanelSpace` reconciliation rather than on
  deleting any retained geometry component.
- `hinge_to_pose` writes the whole pose. A pose tween on a hinged entity is
  discarded.
- Arrangement hinge driving is unconditional for ordinary members. Members
  carrying `FoldAngles` are excluded so the fold actuation layer owns their
  angle.
- `Accordion::fold` and `Coil::fold` are clamped, but their `lean` values, rule
  rest angles, offsets, and authored geometry are otherwise caller-controlled.
- `hana_diegetic` consumes `hana_valence` with default features disabled, so its
  anchoring integration does not pull in `bevy_tween`.
- Workspace reflect auto-registration covers concrete reflected types. Generic
  monomorphizations and foreign reflected type-data patches still require
  explicit registration.

## Why it is this way

The contract is component data rather than dynamic shape dispatch so every
entity can publish its own geometry, Bevy change detection and reflection can
inspect mutable inputs, and the resolver remains independent of shape crates.
Shape-specific dispatch exists only where it belongs: `TilingRule` turns a
regular shape's adjacency into arrangement placement.

`AnchorPose` is separate from `Transform` because animation describes motion
around a bond, while the resolver owns final placement. Combining them would
create two writers with incompatible meanings.

Dependency ordering is separated from placement math so world anchors, screen
anchors, and future coordinate spaces share cycle handling, fallback behavior,
and diagnostics without sharing units or projection rules.

Panel attachment authoring is separate from the stored world relation because
one public authoring surface feeds two coordinate-space positioners. The split
prevents screen panels from entering the world resolver and producing
missing-geometry diagnostics every frame.

Arrangements emit placement data and let the consumer apply it because a plain
entity can insert `AnchoredTo` directly, while a panel consumer must preserve
panel coordinate-space policy and unit lowering.

The relationship is immutable and its reflected registration is type-only
because reverse-index correctness is more important than reflective mutation
convenience. Edge endpoint order is also deliberate: it makes fold direction
part of authored geometry instead of another shape-specific flag.
