# Panel anchoring example

Status: **in progress**. This doc owns the remaining `panel_anchoring` example
work. The core panel anchoring feature is documented separately in
[`as-built/panel-anchoring.md`](as-built/panel-anchoring.md).

This is a temporary working plan for an example, not a permanent as-built doc.
Delete it when the example is done.

## Goal

Show panel anchors as exact layout constraints and as readable geometry for
animation, across world and screen space, in one keyboard-driven scene.

The example should stay visual and focused: relations remain point-to-point,
animation writes `PanelAnchorPose`, and the resolver remains the only
`Transform` writer for attached panels.

## Current State

The example currently lives under
`crates/bevy_diegetic/examples/panel_anchoring/` and is split into:

- `main.rs` - app setup, title-bar wiring, system ordering
- `scene.rs` - active capability, capability switches, layout morphing
- `anchor_demo.rs` - anchor fan, depth controls, spin animation, markers
- `hinge.rs` - hinge-chain fold state and per-link pose writer
- `info_panel.rs` - screen-space info/legend panel
- `menu.rs` - capability menu

Implemented:

| key | capability | space | status |
|----|----|----|----|
| `1` | anchor selection | world | implemented |
| `2` | spin while attached | world | implemented |
| `3` | hinge chain | world | implemented |

Still planned:

| key | capability | space | status |
|----|----|----|----|
| `4` | spin in place, locked to the plane | screen | planned |
| `5` | rotated panel drags its followers | screen | planned |
| `Tab` | fly camera to explainer / return | world camera | planned |

The current code accepts only number keys `1` through `3`; add `4` and `5`
when their screen-space scenes exist.

## Controls

Current controls:

| key | action |
|-----|--------|
| `1` | select Anchor |
| `2` | select Spin; pressing it again toggles the spin envelope |
| `3` | select Hinge Chain |
| `Tab` | select which anchor-panel section the arrows edit |
| arrow keys | cycle the selected source or target anchor |
| `[` / `]` | move the depth offset along the target plane normal |
| `Ctrl` + `[` / `]` | move depth faster |
| `+` / `-` | add or remove tiles |
| `O` | show or hide anchor markers |
| `I` | toggle autofit |
| `P` / `Space` | pause or resume an in-flight spin or hinge ease |

Hinge-chain-only controls:

| key | action |
|-----|--------|
| `A` / `C` | choose accordion or coil fold pattern |
| `F` / `B` | choose front or back fold direction |
| `G` / `S` | choose glide or step travel |
| `U` / `D` | fold in one direction or its mirror |
| `R` | unwrap/reset the hinge chain |

Planned controls:

| key | action |
|-----|--------|
| `4` | select screen spin |
| `5` | select screen rotated-target chain |
| `Tab` | if the explainer is added, fly the orbit camera to it and back |

## Animation Ordering

The example's animation systems must write resolver-read inputs before world
attachment resolution:

- `drive_anchor_pose` and `drive_hinge_pose` run in
  `PanelSystems::AnimateAnchorPose`.
- `PanelSystems::AnimateAnchorPose` is ordered before
  `resolve_world_space_panel_attachments`.
- Writes before the resolver affect the current frame; writes after the resolver
  affect the next frame.
- Active relations stay active during animation. Animation writes
  `PanelAnchorPose`, not `Transform`.

The `anchor_animation` test filter covers the same-frame and next-frame ordering
contract.

## Capability 1 - World Anchor Selection

Scene:

- a world-space chain of anchor tiles
- each dependent tile is attached with `AnchoredToPanel`
- the selected source and target anchor positions are shown in the panel layout
- optional gizmo links show separated anchor points when depth is non-zero

Behavior:

- arrows cycle source and target anchors
- depth changes via `[` / `]`
- tile count changes via `+` / `-`
- markers can be hidden or shown with `O`
- relation changes are eased visually with `PanelAnchorPose`

Purpose:

- proves world anchoring is not screen-position sugar
- makes independent `source_anchor` and `target_anchor` choices visible
- shows z offset stacking across a chain

## Capability 2 - World Spin While Attached

Scene:

- the same anchor fan as capability `1`
- each anchored tile can carry `PanelAnchorPose`

Behavior:

- selecting capability `2` starts or resumes the spin envelope
- pressing `2` again reverses toward a full-turn rest state
- `P` / `Space` freezes or resumes an in-flight envelope
- depth remains under manual `[` / `]` control
- the relation stays active; spin is a normal-axis `PanelAnchorPose` rotation

Purpose:

- demonstrates resolver-owned animation
- makes pivot-at-pin behavior visible
- exercises anchor pose rotation and depth offset together

## Capability 3 - World Hinge Chain

Scene:

- the same persistent tiles morph from the fan into an edge-to-edge strip
- each dependent tile carries a right-axis `PanelAnchorPose` hinge rotation
- the chain can fold as an accordion or coil, leaning front or back

Behavior:

- `3` selects the hinge chain
- `A` / `C`, `F` / `B`, and `G` / `S` choose fold mode
- `U` and `D` fold in mirrored directions
- `R` unwraps the chain
- `P` / `Space` pauses or resumes an in-flight fold ease
- switching to or from the hinge chain uses `ModeMorph` so existing tiles glide
  between layouts instead of respawning

Purpose:

- proves chained attachments resolve with per-link pose rotation
- demonstrates hinge motion without extending `AnchoredToPanel` beyond point
  snapping
- shows child hinge rotations compounding relative to each parent's resolved
  plane

## Capability 4 - Screen Spin In Place

Status: planned.

Scene:

- a screen panel anchored to another screen panel
- the dependent is a leaf
- the dependent carries `PanelAnchorPose`

Behavior:

- selecting `4` switches to a screen-space scene
- `Space` / `P` controls a screen-plane spin
- the pinned source anchor point stays fixed on screen
- authored out-of-plane rotation is visibly inert because screen anchoring keeps
  only the in-plane z twist
- reset clears the screen pose and relies on authored-rotation restoration

Purpose:

- demonstrates the screen interpretation of `PanelAnchorPose`
- exercises Phase 7 Layer 1 behavior in an interactive scene
- makes "locked to the screen plane" visible

## Capability 5 - Screen Rotated-Target Chain

Status: planned.

Scene:

- a short screen-space chain
- a target screen panel rotates in-plane
- one or more dependents are pinned to its anchor points

Behavior:

- selecting `5` switches to the rotated-target chain
- rotating the target moves its anchor points
- dependents track the rotated anchor points in the same update
- the scene should show the target and follower anchor points clearly

Purpose:

- demonstrates Phase 7 Layer 2 oriented-rect anchor math
- proves screen chains stay correct under rotation

## Explainer Panel

Status: planned.

The intended explainer is a world-space `DiegeticPanel` behind the main demo,
with readable text for the active capability. `Tab` should fly the orbit camera
to the explainer and back without occluding the demo.

This may require a reusable fairy_dust "look at panel / return home" camera
surface. Get explicit API approval before adding public fairy_dust API.

## Boundaries

- Do not make animation part of `AnchoredToPanel`.
- Do not extend `AnchoredToPanel` beyond point-to-point snapping for hinge
  motion.
- Do not mix editable-field popup tracking into this example task. If same-frame
  popup tracking for world-anchored editable fields becomes required, handle it
  separately in `ime/editor.rs` after `resolve_world_space_panel_attachments`.
- Do not keep this doc after the example is complete; examples do not get
  permanent as-built docs.

## Closeout

Current partial closeout:

```sh
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
```

Complete example closeout after capabilities `4` and `5`:

```sh
cargo +nightly fmt --all -- --check
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
cargo nextest run -p bevy_diegetic
```
