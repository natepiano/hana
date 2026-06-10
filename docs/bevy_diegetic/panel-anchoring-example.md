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
2. two panels that animate toward and away from each other using their resolved
   anchor points
3. a three-panel chain that uses point anchors plus edge geometry to unwrap like
   hinged panels

## Controls

Use visible title-bar controls if this becomes a full UI example, but keep the
first code path simple:

| key | action |
|-----|--------|
| `1` | focus the anchor-selection demo |
| `2` | focus the spring pair demo |
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

## Demo 2 — anchor geometry as animation input

Scene:

- left panel and right panel are not hard-snapped by `AnchoredToPanel`
- each panel exposes or queries resolved anchor geometry
- animation target: left panel `CenterRight` approaches right panel `CenterLeft`

Behavior:

- `Space` toggles attracted vs separated state.
- On attract, panels move so the two anchor points approach each other.
- On separate, panels return to their starting positions.
- The animation reads helper-backed current anchor geometry before it writes
  transforms, so target motion earlier in the frame is visible.
- Use an elastic easing curve for the first version. A later version can replace
  it with a damped spring, but the example should not need physics to make the
  point.

Purpose:

- shows why public anchor geometry matters even when no exact attachment is
  active
- demonstrates anchor-driven animation without mutating the attachment resolver
- creates the "magnetic panel" feel the API is intended to support

## Demo 3 — chained panels and unwrap

Scene:

- three panels: `A`, `B`, `C`
- static point relations:
  - `B` `BottomCenter` anchored to `A` `TopCenter`
  - `C` `BottomCenter` anchored to `B` `TopCenter`
- edge geometry from `PanelAnchorEdge` drives hinge visuals and rotation
- the chain starts folded upward, then unwraps into a coplanar strip

Anchor intent:

```text
B BottomCenter = A TopCenter
C BottomCenter = B TopCenter
```

Behavior:

- When folded, each dependent panel keeps the point relation fixed and uses
  edge geometry for local hinge rotation around the visual shared edge.
- `Space` animates the local hinge rotations toward `0`, making the panels
  unwrap into one coplanar strip.
- Toggling again folds the chain back up.

Purpose:

- proves chained attachments resolve in order
- exercises post-alignment local rotation
- demonstrates that anchor points can define hinge edges, not just positions

## Implementation Notes

Keep the example mostly visual and focused:

- use large text labels directly on the panels
- draw optional gizmo lines between active anchor points
- keep all panels in front of the camera with a shallow angle so the hinge
  motion is readable
- avoid adding constrained sizing; every panel can use fixed or fit size
- do not make animation part of `AnchoredToPanel`; animation systems consume
  resolved anchor geometry and write transforms

## Dependencies

This example depends on the phases in [`anchor-to-panel.md`](anchor-to-panel.md):

| item | earliest phase | reason |
|------|----------------|--------|
| title/perf panel screen anchoring in `diegetic_text_stress` | Phase 1 | proves the relationship model, observer flushes, graph behavior, and same-frame screen placement |
| anchor geometry read smoke test | Phase 2 | proves public `point` and `edge` geometry is fresh enough for consumers |
| Demo 1: selectable world-space anchor points | Phase 3 | needs world-to-world attachment, target-plane math, and no-lag transform scheduling |
| Demo 2: elastic pair | Phase 4 | needs public anchor geometry plus animation ownership rules |
| Demo 3: chained unwrap | Phase 4 | needs edge geometry and post-alignment or hinge-style animation input |

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
