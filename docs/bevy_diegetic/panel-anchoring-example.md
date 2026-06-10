# Panel anchoring example

Status: **started**. `examples/panel_anchoring.rs` now covers Demo 1. The
animation demos remain planned for the later Phase 4 passes described in
[`anchor-to-panel.md`](anchor-to-panel.md).

## Goal

Show that panel anchors are useful both as exact layout constraints and as
readable geometry for animation systems.

`examples/panel_anchoring.rs` starts with the Phase 3 world-anchoring behavior.
Phase 1 screen-space anchoring remains covered by `diegetic_text_stress`.

The example should grow into three compact demos in one scene:

1. a world-space panel anchored to another panel, with hot keys cycling the
   source and target anchor points
   (**implemented**)
2. an attached panel that lifts off its target along the plane normal and spins
   on its pinned anchor point via animated `PanelAnchorPose`
3. a three-panel chain that unwraps like hinged panels by animating per-link
   pose rotation about each pinned point

## Controls

Use visible title-bar controls if this becomes a full UI example, but keep the
first code path simple:

| key | action |
|-----|--------|
| `1` | focus the anchor-selection demo |
| `2` | focus the lift-and-spin pose demo |
| `3` | focus the chained unwrap demo |
| left / right arrows | cycle the dependent panel's source anchor |
| up / down arrows | cycle the target panel's target anchor |
| `Space` | toggle the active animation |
| `R` | reset the active demo |

Reset behavior must be explicit before implementation because active world
attachments capture an authored pose in `AnchoredWorldPanelPose`. For Demo 1,
`R` restores the default source and target anchor selections while leaving the
relation active. The dependent remains resolver-owned and snaps to the default
anchor pair. If a later control removes `AnchoredToPanel`, document whether it
restores the captured authored pose or first refreshes that captured pose from
the current resolved placement.

## Demo 1 — world-space point anchoring

Scene:

- target panel: blue world panel
- dependent panel: green world panel
- relation: dependent source anchor anchored to target anchor
- each world panel renders only a shaded in-panel square at its selected anchor
  position
- bottom-left info panel names the selected anchors and shows matching 3x3
  reference grids

Behavior:

- Left/right arrows cycle the dependent panel's source anchor.
- Up/down arrows cycle the target panel's target anchor.
- `R` restores the default source and target anchor pair.
- The dependent panel stays coplanar and keeps the selected source anchor pinned
  to the selected target anchor.

Purpose:

- proves world-space anchoring is not screen-position sugar
- makes all nine anchor points visible and selectable
- proves `source_anchor` and `target_anchor` are independent

## Demo 2 — animated pose while attached

Scene:

- the Demo 1 dependent panel stays attached via `AnchoredToPanel`
- a `PanelAnchorPose` component on the dependent carries the animated rotation
  and pin displacement

Behavior:

- `Space` toggles lifted vs flush state.
- On lift, the dependent translates off the target along the plane normal
  (`pose.translation.z`) and spins on its pinned anchor point (normal-axis
  `pose.rotation`).
- On settle, the pose eases back to identity and the panel sits flush again.
- The relation stays active the whole time; the animation system writes only
  `PanelAnchorPose` before the world resolver, never `Transform`.
- Use an elastic easing curve for the first version.

Purpose:

- shows the resolver-owned animation path: relation as snap constraint, pose as
  the animatable input
- makes the pivot-at-pin behavior visible — the pinned anchor point does not
  move while rotation and lift change
- exercises the z offset and rotation math from Phases 4.2 and 4.3 in one
  visual

## Demo 3 — chained panels and unwrap

Scene:

- three panels: `A`, `B`, `C`
- static point relations:
  - `B` `BottomCenter` anchored to `A` `TopCenter`
  - `C` `BottomCenter` anchored to `B` `TopCenter`
- each dependent carries a `PanelAnchorPose` whose right-axis rotation is the
  hinge angle
- the chain starts folded upward, then unwraps into a coplanar strip

Anchor intent:

```text
B BottomCenter = A TopCenter
C BottomCenter = B TopCenter
```

Behavior:

- When folded, each link's pose rotation tilts it about its pinned
  `BottomCenter` point.
- `Space` animates the pose rotations toward identity, making the panels
  unwrap into one coplanar strip.
- Toggling again folds the chain back up.
- Hinge gizmo visuals may read `PanelAnchorEdge` geometry from Phase 2;
  `AnchoredToPanel` itself stays point-to-point.

Purpose:

- proves chained attachments resolve in order with poses applied per link
- exercises hinge rotation about a pinned edge-center point
- demonstrates that point snap plus pose rotation covers hinged motion without
  an edge constraint in the relationship

## Implementation Notes

Keep the example mostly visual and focused:

- use large text labels directly on the panels
- draw optional gizmo lines between active anchor points
- keep all panels in front of the camera with a shallow angle so the hinge
  motion is readable
- avoid adding constrained sizing; every panel can use fixed or fit size
- do not make animation part of `AnchoredToPanel`; animation systems write
  `PanelAnchorPose` and the resolver remains the only transform writer for
  attached panels

## Dependencies

This example depends on the phases in [`anchor-to-panel.md`](anchor-to-panel.md):

| item | earliest phase | reason |
|------|----------------|--------|
| title/perf panel screen anchoring in `diegetic_text_stress` | Phase 1 | proves the relationship model, observer flushes, graph behavior, and same-frame screen placement |
| anchor geometry read smoke test | Phase 2 | proves public `point` and `edge` geometry is fresh enough for consumers |
| Demo 1: selectable world-space anchor points | Phase 3 | needs world-to-world attachment, target-plane math, and no-lag transform scheduling |
| Demo 2: lift-and-spin pose animation | Phase 4.4 | needs z offset (4.2), `PanelAnchorPose` rotation (4.3), and the animation ordering point (4.4) |
| Demo 3: chained unwrap | Phase 4.4 | needs `PanelAnchorPose` hinge rotation per link (4.3) and the animation ordering point (4.4) |

The example should not land as one large change. Add the static or screen-backed
pieces as soon as their phase exists, then expand the same example file as
world anchoring and animation inputs become available.

## Closeout

| phase | closeout |
|-------|----------|
| Phase 1 | `cargo check -p bevy_diegetic --example diegetic_text_stress` |
| Phase 2 | `cargo nextest run -p bevy_diegetic anchor_geometry_consumer` |
| Phase 3 | `cargo check -p bevy_diegetic --example panel_anchoring` |
| Phase 4 | `cargo check -p bevy_diegetic --example panel_anchoring` plus `cargo nextest run -p bevy_diegetic anchor_animation` |

Complete example closeout:

```sh
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
```
