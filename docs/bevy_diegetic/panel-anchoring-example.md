# Panel anchoring example

Status: **started**. `examples/panel_anchoring.rs` covers capability 1 (world
anchor selection). The remaining capabilities are the Phase 6–8 passes described
in [`panel-anchoring.md`](panel-anchoring.md).

## Goal

Show panel anchors as both exact layout constraints and as readable geometry for
animation, across **world** and **screen** space, in one scene driven by the
keyboard.

`examples/panel_anchoring.rs` is a capability selector: number keys pick the
active capability, `Space` runs its animation, arrows cycle anchors, `R` resets.
A world-space explainer panel narrates the active capability. Phase 1 screen
point anchoring stays covered by `diegetic_text_stress`.

| key | capability | space | status |
|----|----|----|----|
| `1` | anchor selection — cycle source/target anchors | world | implemented |
| `2` | lift & spin while attached | world | planned (Phase 8) |
| `3` | hinge chain unwrap | world | planned (Phase 8) |
| `4` | spin in place, locked to the plane | screen | planned (Phase 8) |
| `5` | rotated panel drags its followers | screen | planned (Phase 8) |

## Controls

| key | action |
|-----|--------|
| `1`–`5` | select the active capability |
| left / right arrows | cycle the dependent panel's source anchor |
| up / down arrows | cycle the target panel's target anchor |
| `Space` | run / toggle the active capability's animation |
| `R` | reset the active capability |
| `Tab` | fly the orbit cam to the explainer panel, then back to the main event |

Reset behavior must be explicit before implementation because active world
attachments capture an authored pose in `AnchoredWorldPanelPose`:

- Capability 1: `R` restores the default source/target anchor pair, relation
  stays active, dependent stays resolver-owned and snaps to the default pair.
- Capabilities 2–5: `R` returns the active `PanelAnchorPose` to identity
  (flush / folded / unrotated). Document whether any control that removes
  `AnchoredToPanel` restores the captured authored pose or first refreshes that
  captured pose from the current resolved placement.

## Explainer panel

The explainer is a **world-space** `DiegeticPanel` placed *behind* the main demo
carrying expansive text about the active capability — what you are seeing, the
active anchor pair / pose values, and which keys apply right now. Because it is a
full world panel and not a cramped screen HUD, it can hold real paragraphs.

`Tab` flies the orbit cam to the explainer (zoom and pan) so it fills the view,
then returns to the main event. Sitting behind the action means the explainer
never occludes the demo. This likely motivates a reusable fairy_dust enhancement
— a "look at this panel / return home" camera move usable beyond this example.
Get explicit API approval before landing any public fairy_dust surface for it.

This example dogfoods world anchoring to lay out its own explanatory UI.

## Capability 1 — world point anchoring (implemented)

Scene:

- target panel: blue world panel
- dependent panel: green world panel
- relation: dependent source anchor anchored to target anchor
- each world panel renders only a shaded in-panel square at its selected anchor
  position
- the explainer names the selected anchors and shows matching 3x3 reference
  grids

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

## Capability 2 — world lift & spin while attached

Scene:

- the capability 1 dependent panel stays attached via `AnchoredToPanel`
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
- exercises the z offset (Phase 5) and rotation (Phase 6) math in one visual

## Capability 3 — world hinge chain unwrap

Scene:

- three world panels: `A`, `B`, `C`
- static point relations:
  - `B` `BottomCenter` anchored to `A` `TopCenter`
  - `C` `BottomCenter` anchored to `B` `TopCenter`
- each dependent carries a `PanelAnchorPose` whose right-axis rotation is the
  hinge angle
- the chain starts folded upward, then unwraps into a coplanar strip

```text
B BottomCenter = A TopCenter
C BottomCenter = B TopCenter
```

Behavior:

- When folded, each link's pose rotation tilts it about its pinned
  `BottomCenter` point.
- `Space` animates the pose rotations toward identity, making the panels unwrap
  into one coplanar strip. Toggling again folds the chain back up.
- Hinge gizmo visuals may read `PanelAnchorEdge` geometry from Phase 2;
  `AnchoredToPanel` itself stays point-to-point.

Purpose:

- proves chained attachments resolve in order with poses applied per link
- exercises hinge rotation about a pinned edge-center point
- demonstrates that point snap plus pose rotation covers hinged motion without
  an edge constraint in the relationship

## Capability 4 — screen spin in place, locked to the plane

Scene:

- a screen panel anchored to another screen panel; the dependent is a leaf
  (nothing is pinned to it)
- a `PanelAnchorPose` on the dependent carries a normal-axis rotation

Behavior:

- `Space` spins the dependent about its pinned anchor point, in the screen
  plane. The pinned anchor point stays fixed on screen; the panel rotates around
  it.
- The panel cannot leave the screen plane: an authored out-of-plane rotation has
  no effect (a control that sets one is visibly inert), demonstrating the
  locked-to-plane projection from Phase 7.

Purpose:

- shows `PanelAnchorPose` is honored on screen, not ignored — it is the same
  component as the world demos, projected onto the screen plane
- exercises Phase 7 Layer 1 (a leaf spins on its pin) and the full-pose
  `ResolvedScreenPanelPosition`

## Capability 5 — screen rotated-target chain

Scene:

- a short screen chain: a root screen panel with one or two dependents pinned to
  its anchor points
- the root carries a `PanelAnchorPose` rotation

Behavior:

- `Space` rotates the root in the screen plane. Its anchor points move with it,
  and the dependents track to the rotated anchor points — the followers swing to
  follow the root.
- Resolves in one update across the chain.

Purpose:

- exercises Phase 7 Layer 2 (oriented-rect anchor math): a rotating screen
  *target* re-anchors the panels pinned to it
- proves screen chains stay correct under rotation, the screen analog of the
  world hinge chain

## Implementation notes

Keep the example mostly visual and focused:

- use large text labels directly on the panels
- draw optional gizmo lines between active anchor points
- keep the world panels in front of the camera at a shallow angle so the hinge
  motion is readable; keep the explainer behind them
- avoid constrained sizing; every panel can use fixed or fit size
- do not make animation part of `AnchoredToPanel`; animation systems write
  `PanelAnchorPose` and the resolver stays the only transform writer for
  attached panels

## Dependencies

This example depends on the phases in [`panel-anchoring.md`](panel-anchoring.md):

| item | earliest phase | reason |
|------|----------------|--------|
| title/perf panel screen anchoring in `diegetic_text_stress` | Phase 1 | proves the relationship model, observer flushes, graph behavior, and same-frame screen placement |
| anchor geometry read smoke test | Phase 2 | proves public `point` and `edge` geometry is fresh enough for consumers |
| Capability 1: selectable world anchor points | Phase 3 | world-to-world attachment, target-plane math, no-lag transform scheduling |
| Capability 2: world lift & spin | Phase 8 | z offset (5), `PanelAnchorPose` rotation (6), animation ordering (8) |
| Capability 3: world hinge chain | Phase 8 | `PanelAnchorPose` hinge per link (6), animation ordering (8) |
| Capability 4: screen spin in place | Phase 8 | screen in-plane rotation (7), animation ordering (8) |
| Capability 5: screen rotated-target chain | Phase 8 | screen oriented-rect chains (7), animation ordering (8) |
| world explainer + orbit-cam fly-to | Phase 8 | the demonstration UI; may add a reusable fairy_dust camera enhancement |

The example should not land as one large change. Add each capability as soon as
its phase exists, expanding the same example file.

## Closeout

| phase | closeout |
|-------|----------|
| Phase 1 | `cargo check -p bevy_diegetic --example diegetic_text_stress` |
| Phase 2 | `cargo nextest run -p bevy_diegetic anchor_geometry_consumer` |
| Phase 3 | `cargo check -p bevy_diegetic --example panel_anchoring` |
| Phase 8 | `cargo check -p bevy_diegetic --example panel_anchoring` plus `cargo nextest run -p bevy_diegetic anchor_animation` |

Complete example closeout:

```sh
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
```
