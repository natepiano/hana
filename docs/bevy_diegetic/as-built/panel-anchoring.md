# Panel anchoring

## What It Is

Panel anchoring is declarative panel-to-panel placement for `bevy_diegetic`.
An `AnchoredToPanel` relationship pins one anchor point on a source panel to one
anchor point on a target panel, with an optional `PanelAnchorOffset` and mutable
`PanelAnchorPose`. It works for screen panels and world panels, but each
resolver owns only edges in its own coordinate space. Attachments determine
position, depth, and resolver-owned pose around a pin; they do not compute width
or height.

The public surface is exported from `panel/mod.rs` and `lib.rs`:
`AnchoredToPanel`, `PanelsAnchoredHere`, `PanelAnchorOffset`,
`PanelAnchorPose`, and read-only anchor geometry types such as
`PanelAnchorGeometryParam`, `PanelScreenBounds`, `PanelPlane`, and
`ResolvedPanelAnchorGeometry`.

## How It Works

`panel/anchoring.rs` defines the relationship and resolver-owned state:

- `AnchoredToPanel` is an immutable Bevy relationship source. It stores a
  private target entity plus `source_anchor`, `target_anchor`, and `offset`.
- `PanelsAnchoredHere` is the reverse relationship target.
- `PanelAnchorOffset` stores `x`, `y`, and `z` as `Dimension`s. Static offsets
  are authored in target-panel layout units.
- `PanelAnchorPose` is mutable resolver input for animation:
  `rotation: Quat` and `translation: Vec3`.
- `ResolvedScreenPanelPosition` is the screen resolver's private override
  component: `anchor_position`, `depth`, `authored_depth`, `rotation`, and
  `authored_rotation`.

`panel/attachment_resolver.rs` provides the shared graph resolver.
Coordinate-space-specific code classifies attachments into active or skipped
candidates, then `resolve_panel_attachments` resolves the dependency graph in
target-before-source order. Skipped candidates, skipped dependencies, and cycles
record diagnostics and receive fallback handling.

`panel/anchor_geometry.rs` provides read-only geometry. `PanelScreenBounds`
resolves axis-aligned logical-pixel bounds and anchor points. `PanelPlane`
resolves a world panel's top-left origin, right/up/normal basis, and meter size
from the current transform. `PanelAnchorGeometryParam` exposes this geometry for
callers without making either resolver's internal placement state public.

World anchoring lives in `panel/world_anchoring.rs`. It runs in `PostUpdate`
before `TransformSystems::Propagate`. The resolver ignores screen sources,
rejects mixed-space edges, computes target and source planes through
`TransformHelper`, and writes the source `Transform`. Placement is:

```text
target_point = target anchor + static offset + plane_frame_translation(pose.translation)
desired_rotation = plane_rotation(target_plane) * pose.rotation
desired_translation = target_point - desired_rotation * source_anchor_offset
```

That equation keeps the source anchor pinned while allowing arbitrary rotation
about the pin. `AnchoredWorldPanelPose` captures the authored local transform
the first time the resolver takes ownership and restores it when the relation
stops resolving.

Screen anchoring lives under `screen_space/anchoring/`. `resolve.rs` gathers
windows, panel rects, transforms, anchor poses, depth state, and desired
placements. `candidate.rs` validates screen-only, same-window edges.
`placement.rs` computes the resolved anchor position, depth, and in-plane
rotation. `rect.rs` snapshots `ScreenPanelRect` with anchor position, size,
layout unit, cached bounds, and current angle so rotated targets can anchor
downstream panels in the same update. `projection.rs` projects
`PanelAnchorPose::rotation` to a single z-twist angle with
`screen_in_plane_angle` and rotates 2D offsets with `rotate_screen_offset`.

`screen_space/mod.rs` schedules the screen resolver in `Update` after screen
panel dimensions and observer flushes, before `position_screen_space_panels`.
The placer writes transform x/y from the resolved or configured anchor position,
writes z only when the resolver produced depth, and writes
`Transform.rotation = Quat::from_rotation_z(angle)` only when the resolver
produced a rotation. Authored depth and authored rotation are captured on first
resolver ownership and restored when the resolver returns `None`.

`PanelSystems::AnimateAnchorPose` is the `PostUpdate` ordering point for systems
that write resolver-read inputs before the world resolver. Writes in that set
land in the current frame; writes after world attachment resolution land in the
next frame.

## Invariants

`AnchoredToPanel` is immutable. Retargeting happens by replacing the
relationship component, not by mutating the target in place.

An active attachment gives the resolver ownership of the resolved fields.
Animation systems write `PanelAnchorPose` or replace the relationship at a
state boundary; they do not directly fight the resolver by writing the same
`Transform` fields.

Attachments pin one point. They never compute panel width or height.

Screen and world resolvers own edges by source coordinate space. Cross-space
and cross-window screen attachments are diagnosed rather than partially
resolved.

No attachment uses `ChildOf`. A target despawn detaches or skips dependents
through relationship state and diagnostics; it does not despawn dependent panels
as children.

`ResolvedScreenPanelPosition` uses `None` to mean "use authored/configured
state." The resolver writes `Some` only for fields it owns, and fallback clears
back to `None`.

Screen rotation is in-plane only. Out-of-plane components of
`PanelAnchorPose::rotation` have no screen effect.

World planes must be finite, positive-sized, and orthonormal enough to form a
stable `PanelPlane`. Unsupported parent transforms for anchored world panels are
rejected.

## Calibration And Gotchas

Static `PanelAnchorOffset` uses layout coordinates: positive x right, positive
y down, positive z in front of the target. World static y therefore maps
through `-target_plane.up()`, while `PanelAnchorPose::translation` is
plane-frame input where positive y maps to `plane.up()`.

Screen `PanelAnchorPose::translation.y` is negated when applied because screen
logical coordinates are y-down but pose authoring treats positive y as up.

Screen depth is draw-order depth under the shared orthographic camera, not
perspective distance. The screen camera is at z = 1000 with far = 2000, so very
large resolved depths can clip.

The screen resolver treats depth ownership as authored when either static z or
pose z is individually nonzero. Do not collapse that to "sum is nonzero";
canceling z inputs still mean the resolver owns depth for that frame.

`ScreenPanelRect` caches bounds. Its `anchor_position` is visible within the
anchoring module, so future code should update it through
`with_anchor_position_and_angle` to keep the cache and threaded angle coherent.

`PanelScreenBounds::point` remains axis-aligned. Oriented screen anchor math is
intentionally local to the screen placer via `oriented_anchor_point`; extending
`PanelScreenBounds` for oriented points would be a public API change.

World placement reads `PanelAnchorPose` only inside active world candidate
placement. A standalone pose component on an unattached panel is inert.

## Why It Is This Way

The feature is a relationship, not parenting, because attachment depends on
both panels' measured sizes and current coordinate-space placement. Reparenting
would make window-absolute screen transforms double-apply, inherit target scale
chains, couple lifetimes, and turn attachment cycles into hierarchy problems.

`PanelAnchorPose` is separate from `AnchoredToPanel` because the relationship is
immutable and maintains a reverse index. Animation needs a mutable input that
can change every frame without replacing the relationship or churning
`PanelsAnchoredHere`.

The shared graph resolver keeps cycle handling, skipped dependency behavior, and
diagnostics identical for screen and world anchoring while letting each
coordinate-space adapter own validation and placement math.

World anchoring uses full quaternion pose composition because a world panel can
hinge off the target plane. Screen anchoring projects to z twist because the
shared overlay camera defines a flat screen plane; only in-plane rotation
preserves the screen panel model.

Screen depth and rotation both use captured authored values because the resolver
only temporarily owns those transform fields. Removing the relation or pose
should return the panel to the user's authored transform, not to whatever value
the resolver last wrote.
