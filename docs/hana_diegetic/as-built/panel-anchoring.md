# Panel anchoring

## What it is

Panel anchoring is declarative placement of a panel source relative to another
entity in `hana_diegetic`. Callers insert an `AnchoredToPanel` bundle that names
the source panel anchor, the target anchor, and an optional
`PanelAnchorOffset`. A world source may target a world panel or a reified world
widget. Screen widget targets remain Phase 4.5 work. The bundle feeds two
coordinate-space adapters:

- world panels lower into `hana_valence::AnchoredTo` and are placed by
  `hana_valence::resolve_anchors`;
- screen panels retain the shared panel authoring, currently require a panel
  target, and are placed by the screen-space adapter over
  `hana_valence::resolve_attachments`.

Both paths use `hana_valence::AnchorPose` for animated rotation and translation.
Attachments determine position, depth, and pose around a pin; they do not
compute panel width or height. The shape-agnostic contracts behind this adapter
are documented in
[hana_valence anchoring and arrangements](../../hana_valence/as-built/anchoring-and-arrangements.md).

The public panel-specific surface is `AnchoredToPanel`, `PanelAnchorOffset`,
`PanelSpace`, and `ArrangedPanel`. There is no panel-specific public
relationship, reverse index, pose type, or graph resolver. Read-only panel
geometry APIs such as `PanelAnchorGeometryParam`, `PanelScreenBounds`,
`PanelPlane`, and `ResolvedPanelAnchorGeometry` serve callers, while world
attachment placement reads the Hana Valence geometry component.

## How it works

### Shared authoring

[`panel/anchoring.rs`](../../../crates/hana_diegetic/src/panel/anchoring.rs)
defines the insert-only public bundle:

```rust
AnchoredToPanel::new(target, source_anchor, target_anchor)
    .with_offset(offset)
```

Inserting it writes a private immutable `PanelAttachmentAuthored` component and
a mutable `PanelAnchorOffset`. `PanelAttachmentAuthored` is the one authoring
record read by both coordinate-space adapters. Its target may be a world panel
or reified world widget on the world path; the screen path still accepts only a
screen panel. It is not a Bevy relationship and is deliberately not public or
reflectively mutable.

`AnchoredToPanel` is a bundle rather than a queryable component. Runtime
retargeting reinserts a complete replacement bundle, and removal removes the
bundle. This preserves one public authoring shape without exposing the private
screen/world lowering state.

### World panels

For a world source panel, the attachment observer converts
`PanelAttachmentAuthored` to an immutable `hana_valence::AnchoredTo`. The
conversion maps the nine diegetic `Anchor` values to Hana Valence `AnchorId`
values. The relation hooks maintain `hana_valence::AnchoredHere` on the target.

[`panel/valence_provider.rs`](../../../crates/hana_diegetic/src/panel/valence_provider.rs)
publishes `ResolvedAnchorGeometry` for world panels. The component contains nine
local anchor points and four ordered perimeter edges. Points are centered in the
panel's local authored frame, use the panel's world width and height, and do not
bake `Transform` or `GlobalTransform`. The provider runs on
`Changed<DiegeticPanel>`, not `Changed<Transform>`, and updates an existing
geometry component in place.

Reified world widgets publish `ResolvedAnchorGeometry` only while
`AnchoredHere` records world demand. The widget rectangle supplies centered
widget-local points. On reification, the widget keeps a translation-only local
`Transform` and receives one initial `GlobalTransform` composed from the owner
panel's current global transform and the widget's panel-local transform. The
seed provides the inherited scale before the child's first ordinary transform
propagation. A private widget-to-owner-panel `AnchoredTo` relation turns the
widget into a resolver candidate while demand exists. Its offset converts the
widget center from panel-local coordinates with the owner panel's effective
uniform scale, so Valence can apply current owner rotation and graph order. The
bridge still resolves an anchored owner first and places the widget during the
same resolver pass. Rectangle or demand changes update geometry; transform
changes may refresh only the private relation. Removing the final demand
retires Hana-owned geometry and the bridge independently, preserving an
application-replaced relation.

`write_panel_anchor_offsets` classifies the target as a panel or widget. A
panel target supplies its live dimensions, layout unit, and global scale. A
widget target resolves `WidgetOf`, uses the owner panel's layout unit, and uses
the owner panel's already-propagated global scale to convert layout units. The
system writes `hana_valence::ResolvedAnchorOffset`, which overrides the zero
static offset on the lowered relation for that resolve pass. Target-local
positive-down y is
converted to the resolver's positive-up y here; unit and DPI policy therefore
stay in `hana_diegetic`.

World placement follows the Hana Valence pipeline:

```text
AnchorSystems::FillGeometry
    -> AnchorSystems::AnimatePose
    -> AnchorSystems::Resolve
    -> TransformSystems::Propagate
```

`HeadlessLayoutPlugin` initializes `ResolveDiagnostics`, installs the panel
provider and offset lowering, and runs `resolve_anchors` in
`AnchorSystems::Resolve`. `resolve_anchors` is the sole `Transform` writer for a
world panel carrying the lowered `AnchoredTo`; animation systems write
`AnchorPose`, `Hinge`, or `HingePivot` instead.

When world attachment ownership begins, `AnchoredWorldPanelPose` captures the
source's authored local transform. Removing the attachment restores that
transform. Coordinate-space conversion owns its own transform handoff while
the anchoring observer adds or removes world-only relation state.

### Screen panels

Screen panels keep `PanelAttachmentAuthored` and `PanelAnchorOffset` but do not
carry the world `hana_valence::AnchoredTo`. The screen resolver currently
requires a same-window screen-panel target; adding screen widget targets is
Phase 4.5 work. It builds screen rectangles and calls
`hana_valence::resolve_attachments` for target-before-source ordering, cycle
detection, skipped-dependency propagation, fallback, and bounded diagnostics.
Its placement callback retains window, viewport, logical-pixel, and draw-depth
math inside `hana_diegetic`.

The screen adapter reads `hana_valence::AnchorPose`. Translation x/y is applied
in the flat screen plane, positive pose y is converted to screen-down
coordinates, z participates in resolved draw depth, and rotation is projected
to a single in-plane z angle. Out-of-plane rotation has no screen effect.

`ResolvedScreenPanelPosition` is the private output seam between attachment
resolution and `position_screen_space_panels`. Its optional position, depth,
and rotation fields are cleared on fallback. Depth and rotation capture the
authored `Transform` values when the resolver first takes ownership and restore
them when ownership ends.

The screen resolver runs in `Update` after final screen dimensions and observer
commands, then `position_screen_space_panels` applies its output. A screen pose
writer that needs same-frame placement must run before
`PanelSystems::ResolvePanelAttachments`. `PanelSystems::AnimateAnchorPose` is a
`PostUpdate` ordering point for the world resolver, so writes there become
visible to the screen path on the next `Update`.

### Coordinate-space changes

`DiegeticPanel.coordinate_space` remains authoritative for sizing and
conversion. Because conversion mutates that field in place, it cannot drive a
component observer directly. `PanelSpace` mirrors only the `World`/`Screen`
discriminant and is reinserted at panel spawn and conversion apply points.

`On<Insert, PanelSpace>` reconciles attachment state:

- moving to screen removes `AnchoredTo`, `ResolvedAnchorOffset`, and captured
  world-attachment pose;
- moving to world recreates the lowered relation and resumes world offset
  lowering.

Every writer of `DiegeticPanel.coordinate_space` must update `PanelSpace` in the
same operation. The possible removal of this mirrored state is tracked in the
[Hana Valence future-work backlog](../../hana_valence/future-work.md).

### Panel arrangements

[`panel/arrangement.rs`](../../../crates/hana_diegetic/src/panel/arrangement.rs)
provides the insert-only `ArrangedPanel` adapter for Hana Valence `QuadTiling`.
It uses `ArrangementMembers` insertion order and applies one placement per
member once the geometry and transforms are ready. The arrangement root carries
exactly one first-class arrangement component: `Strip`, `Accordion`, or `Coil`.

World members receive raw `hana_valence::AnchoredTo`, `AnchorPose`, and `Hinge`
components. For a connected arrangement whose root is a screen panel, only the
root participates in screen attachment authoring; the member chain remains in
one world fold frame and each member targets its predecessor through the raw
Valence relation. Linear arrangements do not represent branching panel nets.

## Invariants

- `AnchoredToPanel` is insert-only authoring, not a public relationship
  component. World and screen placement share its private authored record;
  only the world path currently accepts a reified widget target.
- A screen panel must not carry the lowered world `hana_valence::AnchoredTo`.
- A world attachment is resolved only by `hana_valence::resolve_anchors`, which
  is the sole transform writer for that anchored source.
- World panel geometry is entity-local, centered, expressed in authored units,
  and never transform-baked.
- World widget geometry and its owner-panel resolver bridge exist only while
  world demand is nonempty. Widget geometry remains widget-local and is not
  refilled by transform changes.
- Panel unit conversion, DPI handling, target-relative sizing, and y-axis
  lowering remain outside `hana_valence`.
- `PanelAnchorOffset` is mutable live input. The lowered world
  `ResolvedAnchorOffset` does not require replacing the immutable relation.
- Animation writes `hana_valence::AnchorPose`; hinge animation owns the whole
  pose while `Hinge` is present.
- Screen placement owns only the optional position/depth/rotation outputs it
  resolved and restores authored state when those outputs clear.
- Screen attachments require a live screen-panel target in the same window;
  screen widget targets remain Phase 4.5 work. Cross-space and cross-window
  edges diagnose and fall back.
- Attachments are not `ChildOf` parenting and do not couple source lifetime to
  target lifetime.
- `PanelSpace` and `DiegeticPanel.coordinate_space` must remain synchronized.

## Calibration and gotchas

- `PanelAnchorOffset` uses layout coordinates: positive x right, positive y
  down, positive z in front. World lowering negates y; screen placement also
  converts pose y from up to screen-down.
- World z offset is converted with the target's horizontal panel scale. Screen
  z is draw-order depth under the shared orthographic camera, not perspective
  distance.
- The screen camera sits at z = 1000 with far = 2000, so very large resolved
  depths clip rather than clamp.
- The screen resolver treats depth as authored when either static offset z or
  pose translation z is nonzero. Do not reduce that rule to whether their sum
  is nonzero.
- `ScreenPanelRect` caches bounds. Update its anchor position and angle through
  `with_anchor_position_and_angle` so the cache and chain placement remain
  coherent.
- `PanelScreenBounds::point` remains axis-aligned. Oriented screen anchor math
  is intentionally private to screen placement.
- Geometry fill for a non-world panel is a no-op. Correctness depends on
  `PanelSpace` reconciliation removing its world relation, not on deleting a
  retained geometry component.
- Resolver diagnostics are bounded, but repeated persistent skips currently
  emit a warning every frame.

## Why it is this way

The public bundle is separate from the stored world relation because one
panel-facing API feeds two coordinate-space positioners. Inserting a world
relation for screen panels would send them through the 3D resolver without
world geometry and produce persistent false diagnostics.

World anchoring delegates geometry-independent placement to Hana Valence so
panels, tiles, and other shapes share relationship hooks, dependency ordering,
hinges, and diagnostics. The screen adapter reuses only the graph layer because
window selection, logical pixels, draw depth, and in-plane projection do not
belong in a shape-agnostic 3D resolver.

`AnchorPose` is shared across both adapters so animation policy does not split
by panel space. It remains separate from `Transform` because placement owns the
final transform, while animation describes motion around the attachment.

Attachments are relationships rather than transform parenting because their
position depends on measured panel sizes and current coordinate space.
Parenting would double-apply window-absolute screen transforms, inherit target
scale and lifetime, and turn attachment cycles into hierarchy cycles.
