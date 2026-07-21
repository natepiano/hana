# Panel anchoring

## What it is

Panel anchoring is declarative placement of a panel source relative to another
entity in `hana_diegetic`. Callers use `DiegeticPanelCommands` methods on Bevy
`Commands` with typed `PanelEntity<Space>` and `WidgetEntity<Space>` handles. `PanelAttachment` names
the source and target anchors plus an optional `PanelAnchorOffset`. The shared
`Space` parameter makes every supported source/target pair same-space before it
can reach the ECS. The checked mutation feeds two coordinate-space adapters:

- world panels lower into `hana_valence::AnchoredTo` and are placed by
  `hana_valence::resolve_anchors`;
- screen panels retain the shared panel authoring, accept same-window screen
  panels and widgets, and are placed by the screen-space adapter over
  `hana_valence::resolve_attachments`.

Both paths use `hana_valence::AnchorPose` for animated rotation and translation.
Attachments determine position, depth, and pose around a pin; they do not
compute panel width or height. The shape-agnostic contracts behind this adapter
are documented in
[hana_valence anchoring and arrangements](../../hana_valence/as-built/anchoring-and-arrangements.md).

The public attachment surface is `PanelEntity<World>`, `PanelEntity<Screen>`,
`WidgetEntity<World>`, `WidgetEntity<Screen>`, `DiegeticPanelCommands`,
`PanelAttachment`, and `PanelAnchorOffset`. `PanelEntityReader` checks a live
panel before minting a typed panel handle. `PanelWidgetReader::typed_entity`
does the same for a widget, its owner, and the owner's authoritative widget
index. A non-identical tree replacement clears that index immediately, so an
old widget handle becomes invalid before the obsolete entity is later removed
by reification. Both handle families expose
`entity()` for unrelated ECS work but have no public unchecked constructor.
There is no panel-specific public relationship, reverse index, target-handle
family, pose type, or graph resolver. `PanelSpace` remains a runtime
classification rather than the attachment authoring boundary. It is required on
every panel and synchronized on whole-component replacement and conversion so
`PanelWidgetReader` can check a typed owner without conflicting with a
simultaneous mutable `PanelWidgetWriter` query.

## How it works

### Shared authoring

[`panel/anchoring.rs`](../../../crates/hana_diegetic/src/panel/anchoring.rs)
defines the checked public mutation surface:

```rust
let authored = PanelAttachment::new(source_anchor, target_anchor)
    .with_offset(offset);
commands.attach_to_widget(source, target_widget, authored);
```

`attach_to_panel` and `attach_to_widget` require the source and target to carry
the same `Space` type. `retarget_to_panel` and `retarget_to_widget` enforce the
same rule, while `detach` accepts only the typed source. Every operation checks
that each handle still matches the live `DiegeticPanel`, `PanelWidget`, and
`WidgetOf` state and the owner's current widget index when the queued operation
applies. A stale handle or graph
conflict emits a warning and changes none of that operation's attachment state.
Attachment and conversion calls made through one `Commands` value apply in the
order written.

A successful attach writes a private immutable `PanelAttachmentAuthored`
component and a mutable `PanelAnchorOffset`. `PanelAttachmentAuthored` is the
one authoring record read by both coordinate-space adapters. It is not a public
Bevy relationship and is not reflectively mutable. Public retargeting replaces
the private target while keeping the authored anchors and offset; public detach
removes both authoring components. Direct application mutation of private
lowering components is outside the supported contract.

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
carry the world `hana_valence::AnchoredTo`. A target may be a same-window screen
panel or a widget owned by one. Widget attachment lowering installs a private
`ScreenWidgetAnchoredTo` / `ScreenWidgetAnchoredHere` relationship. The reverse
membership changes in the same queued operation as `PanelAttachmentAuthored`.
Retargeting removes the source from its old widget and adds it to its new target
before the next queued panel operation runs. The membership is screen geometry
demand and supports any number of dependent panels; removing the final source
retires only screen proxy state unless world demand still needs the shared
geometry.

Each demanded screen widget contributes a synthetic resolver edge from the
widget to its owning panel. The placement callback resolves the owner first,
then derives the widget viewport rectangle from `WidgetAnchorRect`, the
owner's current `ScreenPanelRect`, and the owner's current scale and in-plane
rotation. It does not read the widget's `GlobalTransform`. Real attachment
edges continue to target the widget entity, so
`hana_valence::resolve_attachments` orders owner panel, widget proxy, and
dependent panel in one graph pass.

The screen adapter calls `hana_valence::resolve_attachments` for
target-before-source ordering, cycle detection, skipped-dependency
propagation, fallback, and bounded diagnostics. Its placement callback retains
window, viewport, logical-pixel, and draw-depth math inside `hana_diegetic`.
Missing widget ownership, geometry, transforms, and windows are reported with
the same source/target/reason diagnostic keys as panel attachment failures.

The screen adapter reads `hana_valence::AnchorPose`. Translation x/y is applied
in the flat screen plane, positive pose y is converted to screen-down
coordinates, z participates in resolved draw depth, and rotation is projected
to a single in-plane z angle. Out-of-plane rotation has no screen effect.

`ResolvedScreenPanelPosition` is the private output boundary between attachment
resolution and `position_screen_space_panels`. Its optional position, depth,
and rotation fields are cleared on fallback. Depth and rotation capture the
authored `Transform` values when the resolver first takes ownership and restore
them when ownership ends.

The screen resolver runs in `Update` after final screen dimensions, widget
reification, and observer commands. Screen relationship synchronization is
followed by `ScreenSpaceSystems::WidgetDemandCommandsApplied`; geometry
publication and its command flush complete before
`PanelSystems::ResolvePanelAttachments`. Checked screen-widget attachment
commands also seed the initial proxy and geometry in their deferred batch, so a
newly reified target is available at that same fence.
`position_screen_space_panels` then applies the resolver output. A screen pose
writer that needs same-frame placement must run before
`PanelSystems::ResolvePanelAttachments`.
`PanelSystems::AnimateAnchorPose` is a `PostUpdate` ordering point for the world
resolver, so writes there become visible to the screen path on the next
`Update`.

### Coordinate-space changes

`DiegeticPanel.coordinate_space` remains authoritative for sizing and
conversion. Public world-to-screen and screen-to-world methods on
`DiegeticPanelCommands` take a typed source handle and return `Result` for
immediate conversion-recipe validation. The queued operation then validates
that the handle still matches the live panel before mutating it. No conversion
method returns a destination-space handle before the operation applies; callers
reacquire that handle through `PanelEntityReader` after the command fence. Raw
command-level conversion helpers are crate-private.

Conversion is rejected when the panel has an outgoing attachment, another
panel targets the panel, or another panel targets one of its widgets. Rejection
queues no conversion. The supported graph change is explicit:

```text
queue detach for affected sources
    -> queue conversions in the required order on the same Commands value
    -> command fence / conversion handoff
    -> reacquire destination handles through PanelEntityReader
    -> reattach
```

This rule keeps the lowering state single-space. Conversion therefore does not
repair cross-space authoring, restore an invalid relation, or reconcile direct
application mutation of private lowering components. Ordinary library-owned
teardown still removes Hana-owned world and screen state while preserving an
application replacement under the component ownership records.

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

- Public authoring uses `DiegeticPanelCommands` on Bevy `Commands`; raw `Entity` is not accepted
  for attachment, retarget, detach, or checked conversion.
- `PanelEntity<Space>` and `WidgetEntity<Space>` are the only public typed
  identity families. Handles are opaque, may become stale, and are revalidated
  at every checked mutation.
- World and screen placement share one private authored record and accept only
  panel or widget targets in the source's coordinate space.
- A screen panel must not carry the lowered world `hana_valence::AnchoredTo`.
- A world attachment is resolved only by `hana_valence::resolve_anchors`, which
  is the sole transform writer for that anchored source.
- World panel geometry is entity-local, centered, expressed in authored units,
  and never transform-baked.
- The world widget-to-panel resolver bridge exists only while world demand is
  nonempty. Shared widget geometry remains while either world or screen demand
  is nonempty, stays widget-local, and is not refilled by transform changes.
- Panel unit conversion, DPI handling, target-relative sizing, and y-axis
  lowering remain outside `hana_valence`.
- `PanelAnchorOffset` is mutable live input. The lowered world
  `ResolvedAnchorOffset` does not require replacing the immutable relation.
- Animation writes `hana_valence::AnchorPose`; hinge animation owns the whole
  pose while `Hinge` is present.
- Screen placement owns only the optional position/depth/rotation outputs it
  resolved and restores authored state when those outputs clear.
- Screen attachments require a live screen panel or a widget owned by one.
  Same-space authoring is enforced by typed handles; cross-window placement is
  still checked at runtime and diagnoses through the screen adapter.
- Attachments are not `ChildOf` parenting and do not couple source lifetime to
  target lifetime.
- A panel participating anywhere in an attachment graph cannot convert until
  an earlier queued operation has detached the affected placement.

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
- Geometry fill for a non-world panel is a no-op. Checked conversion and typed
  authoring prevent a supported screen source from retaining a world relation.
- Resolver diagnostics are bounded, but repeated persistent skips currently
  emit a warning every frame.

## Why it is this way

The public checked mutation is separate from the stored world relation because
one panel-facing API feeds two coordinate-space positioners. A raw entity route
could express cross-space graphs that neither resolver owns, so the authoring
boundary carries space in opaque identities and rejects stale handles before
lowering. Inserting a world relation for screen panels would send them through
the 3D resolver without world geometry and produce persistent false
diagnostics.

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
