# Panel anchoring

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Point-to-point panel
> attachment — pin one panel's anchor point to another panel's anchor point, in
> screen and world space, with optional z offset, anchored rotation
> (`PanelAnchorPose`), and animation. Separate from span-driven sizing
> ([`constrained-screen-sizing.md`](constrained-screen-sizing.md)). Phases 1–5
> shipped; Phases 6–8 are the live zone.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** `bevy_diegetic` — diegetic UI layout engine for Bevy (in-world
  panels, Clay-inspired layout).
- **Stack:** Rust + Bevy 0.19.0-rc.2; bevy_lagrange (orbit camera),
  bevy_enhanced_input, parley/ttf-parser (text).
- **Layout:**
  ```
  crates/bevy_diegetic/src/
    panel/anchoring.rs            — AnchoredToPanel, PanelAnchorOffset, PanelsAnchoredHere, ResolvedScreenPanelPosition; Phase 6 adds PanelAnchorPose, Phase 7 grows ResolvedScreenPanelPosition
    panel/world_anchoring.rs      — PostUpdate world resolver; Phase 6 composes pose rotation
    panel/attachment_resolver.rs  — shared graph/cycle/diagnostics
    panel/anchor_geometry.rs      — PanelPlane, PanelScreenBounds, PanelAnchorGeometryParam, point()/edge()
    panel/mod.rs                  — PanelSystems enum; anchor-type re-exports
    screen_space/anchoring/{mod,rect,placement,candidate,window}.rs — screen resolver; Phase 7 adds in-plane rotation
    screen_space/mod.rs           — position_screen_space_panels; ScreenSpaceSystems
    lib.rs                        — public re-exports
  crates/fairy_dust/src/{orbit_cam,camera_home}.rs, builder/mod.rs — Phase 8 camera move
  crates/bevy_diegetic/examples/panel_anchoring.rs — Phase 8 capability selector
  ```
- **Key files:**
  - `panel/anchoring.rs` — `AnchoredToPanel` (23-38, `#[component(immutable)]`),
    `PanelAnchorOffset` (96-154, x/y/z `Dimension`), `PanelsAnchoredHere`
    (157-179), `ResolvedScreenPanelPosition` (186-191:
    `anchor_position: Option<Vec2>`, `depth: Option<f32>`,
    `authored_depth: Option<f32>` — Phase 7 adds `rotation: Option<f32>`).
  - `panel/world_anchoring.rs` — `WorldAnchorReadParam::placement()` (93-141):
    ~line 126 `desired_rotation = plane_rotation(target_plane)`,
    `desired_translation = target_point - desired_rotation * source_offset`
    (Phase 6 composition site); `restore_inactive_world_panel_poses()` (26-46);
    `resolve_world_space_panel_attachments()` (49-83).
  - `panel/attachment_resolver.rs` — `resolve_panel_attachments()` (51-85),
    `AttachmentGraph` (88+), shared by both resolvers.
  - `panel/anchor_geometry.rs` — `PanelScreenBounds::point(anchor)` (343),
    `PanelPlane` (367), `PanelPlane::from_panel()` (383-411),
    `PanelPlane::point(anchor)` (463-466).
  - `screen_space/anchoring/rect.rs` — `ScreenPanelRect` (17-22: anchor_position,
    anchor, size, layout_unit) — Phase 7 adds an in-plane angle. The Phase 7
    narrative's "ScreenPanelBounds" is this `ScreenPanelRect` (resolver snapshot)
    plus `PanelScreenBounds::point` (public geometry, `anchor_geometry.rs:343`).
  - `screen_space/mod.rs` — `position_screen_space_panels()` (191-254, writes
    `Transform.translation.x/y` + conditional z; Phase 7 adds a rotation write);
    `ScreenSpaceSystems` (52-56).
  - `panel/mod.rs` — `PanelSystems` enum (94-109: … ResolvePanelAttachments,
    PositionScreenSpace, RenderGizmos); anchor re-exports (19-31).
  - `examples/panel_anchoring.rs` — current world static + keyboard anchoring demo.
  - `fairy_dust/src/orbit_cam.rs` (`FairyDustOrbitCam`), `camera_home.rs`
    (`CameraHomeTarget`, `CameraHomeEntity`, `CameraHomeConfig`), `builder/mod.rs`.
  - `PanelAnchorPose` does not yet exist — Phase 6 introduces it.
- **Build:** `cargo check -p bevy_diegetic`
- **Test:** `cargo nextest run -p bevy_diegetic` (never `cargo test`); filters:
  `world_anchoring`, `screen_space::anchoring`, `anchor_animation`.
- **Lint:** `cargo +nightly fmt --all -- --check`; `cargo clippy -p bevy_diegetic --all-targets`.
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_diegetic_gpu_meter`
- **Invariants:**
  - `AnchoredToPanel` is `#[component(immutable)]`; retarget by replacement only.
  - While a relation is active the resolver is the only `Transform` writer for
    that panel; animation writes resolver-read components (`PanelAnchorPose`),
    never `Transform`.
  - `ResolvedScreenPanelPosition` is reconciled — write a field only when its
    value changed (no stale `Changed<>`); `None` per field means "use the
    authored value."
  - Screen and world resolvers own edges by source coordinate space; no
    cross-space attachments (diagnosed, not partially resolved).
  - Screen resolver runs in `Update` after `FlushObserverCommands`; world resolver
    runs in `PostUpdate` before `TransformSystems::Propagate`.
  - `PanelAnchorOffset` dimensions are authored in target-panel layout units;
    resolvers convert to screen logical pixels or world meters.
  - No `ChildOf` parent/child for attachment; target despawn detaches dependents
    without despawning them.
  - Attachments pin one point — never compute width or height.

## Phases

### Phase 6 — anchored rotation: `PanelAnchorPose` and the world resolver · status: done (uncommitted)

#### Work Order

**Goal:** A public, user-insertable `PanelAnchorPose` component that the **world**
resolver reads beside an active `AnchoredToPanel`, rotating and displacing the
panel about its pinned anchor point while the pin stays fixed. The screen resolver
does not read it yet (Phase 7).

**Spec:**

Add a mutable component the resolver reads beside an active `AnchoredToPanel`.
`PanelAnchorPose` is a **public, user-insertable** component: a user puts it on an
entity they control to rotate and animate an attached panel. The relationship
stays the exact snap constraint — while an `AnchoredToPanel` is active the resolver
is the only transform writer for that panel, and animation writes this
resolver-read component, never `Transform` directly. This phase introduces the
component and wires the **world** resolver; the **screen** resolver reads it in
Phase 7.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Default)]
pub struct PanelAnchorPose {
    /// Rotation about the pinned anchor point, in the target-plane frame.
    pub rotation: Quat,
    /// Translation from the pinned anchor point, in plane-frame meters.
    pub translation: Vec3,
}
```

- Rotation is authored in the target-plane frame (`right`, `up`, `normal`).
  Normal-axis rotation spins the panel coplanar on its pin; right/up-axis rotation
  hinges it off the target surface; an arbitrary `Quat` covers any direction.
- The resolver composes `plane_rotation(target_plane) * pose.rotation` and keeps
  the existing translation equation `target_point - desired_rotation *
  source_offset` (`world_anchoring.rs` `placement()`, ~line 126). Because
  translation is derived from the anchor point and the rotated source offset, the
  pinned `source_anchor` stays fixed for any rotation — pivot-at-pin needs no extra
  math.
- `pose.translation` displaces the pin point in the plane frame after the static
  `PanelAnchorOffset` is applied, in meters: animation code works in world units;
  layout-unit conversion stays an authoring convenience of the static offset.
- The component is separate from `AnchoredToPanel` because the relationship is
  `#[component(immutable)]`: re-inserting it every frame to animate would churn the
  `PanelsAnchoredHere` reverse index. The relation states what the panel is pinned
  to; `PanelAnchorPose` states the pose about that pin and is mutable for
  animation.
- `PanelAnchorPose` is honored in both coordinate spaces with different
  interpretations. World (this phase): the full `Quat` in the target-plane frame —
  it can hinge the panel off the target surface. Screen (Phase 7): the pose is
  projected onto the screen plane. The component is space-agnostic; each resolver
  interprets it. Until Phase 7 lands a screen panel's pose is inert because the
  screen resolver does not yet read it — an unfinished wire completed in Phase 7,
  not a by-design ignore.

This extends the attachment contract from position-only to position plus
orientation. Attachments still never compute width or height.

**Files:**
- `panel/anchoring.rs` — add `PanelAnchorPose` next to the other anchor types.
- `panel/mod.rs`, `lib.rs` — re-export `PanelAnchorPose`.
- `panel/world_anchoring.rs` — in `WorldAnchorReadParam::placement()` (~93-141, the
  `desired_rotation`/`desired_translation` computation at ~line 126), read an
  optional `PanelAnchorPose` on the source and compose
  `plane_rotation(target_plane) * pose.rotation`; apply `pose.translation`
  (plane-frame meters) after the static `PanelAnchorOffset`. Leave
  `AnchoredWorldPanelPose` capture/restore (26-46) untouched — the pose is resolver
  input, not authored transform.
- `HeadlessLayoutPlugin` registration — add `app.register_type::<PanelAnchorPose>()`.

**Constraints from prior phases:**
- Phase 2: `PanelPlane` (`anchor_geometry.rs:367`) provides the orthonormal
  target-plane basis via `PanelPlane::from_panel` and `PanelPlane::point(anchor)`;
  it rejects sheared / non-orthonormal planes.
- Phase 3: the world resolver lives in `world_anchoring.rs`, runs in `PostUpdate`
  before `TransformSystems::Propagate`, uses `TransformHelper` for current
  target/parent globals, and owns position + default plane orientation while the
  relation is active. `AnchoredWorldPanelPose` captures/restores the authored local
  transform on detach / invalid target / skip / cycle — `PanelAnchorPose` must not
  interfere with that record.
- Phase 5: `PanelAnchorOffset` carries `z: Dimension`; the world resolver already
  displaces the pinned target point along `target_plane.normal()`. `pose.translation`
  is applied after that static offset.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic world_anchoring` green, plus:
- normal-axis `PanelAnchorPose` rotation spins the dependent coplanar while the
  pinned `source_anchor` point stays fixed (assert the world-space anchor point
  before and after rotation)
- right/up-axis rotation hinges the dependent off the target plane; pinned point
  still fixed
- `pose.translation` displaces the pin in the plane frame in meters, after the
  static `PanelAnchorOffset`
- pose composes with target rotation: a rotated target plane plus a pose rotation
  produces `plane_rotation * pose.rotation`
- removing `PanelAnchorPose` returns the dependent to the plain snapped placement
  without touching the captured `AnchoredWorldPanelPose`
- a screen panel's `PanelAnchorPose` has no effect in this phase; the screen
  resolver does not read it until Phase 7

Closeout:
```sh
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic world_anchoring
cargo nextest run -p bevy_diegetic screen_space::anchoring
```

#### Retrospective

**What worked:**
- `PanelAnchorPose` landed exactly as specified; the existing
  `target_point - desired_rotation * source_offset` equation made pivot-at-pin
  invariance fall out with no extra math. Both reviewers confirmed the pin is
  fixed under any rotation.
- The `has_pose` → `has_authored_pose` rename cleanly separated the new
  `PanelAnchorPose` query from the pre-existing `AnchoredWorldPanelPose` capture
  gate; the two stayed independent as required.

**What deviated from the plan:**
- Added `Default` to the `derive` list (`PanelAnchorPose` is `#[derive(... Default)]`).
  Required: the spec's `#[reflect(Component, PartialEq, Debug, Default)]` demands a
  `Default` impl, and the resolver reads the pose via
  `anchor_poses.get(source).copied().unwrap_or_default()` so a missing component is
  identity. The spec's derive line omitted `Default` while its reflect line named it
  — the implementation resolved that inconsistency.
- Plane-frame translation is applied via a new `plane_frame_translation(plane, v)`
  helper = `right*x + up*y + normal*z`. Note the y-sign differs from the **static**
  `PanelAnchorOffset` path, which uses `- up*y` (layout y-down). `pose.translation`
  is authored in plane-frame meters with +y = plane up; the static offset stays
  y-down layout units. Locked by `panel_anchor_pose_translation_displaces_pin_after_static_offset`.

**Surprises:**
- The world `placement()` reads the pose unconditionally (`unwrap_or_default`) rather
  than gating on `AnchoredToPanel` presence — fine because `placement()` only runs
  for active world candidates, but later phases must keep pose reads inside the
  resolver's active-candidate path, not in a free-standing system.

**Implications for remaining phases:**
- Phase 7: `PanelAnchorPose` and its world composition now exist; Phase 7 wires the
  **screen** interpretation only and must not touch `world_anchoring.rs`. The screen
  projection takes the same `Quat` and reduces it to one in-plane angle θ.
- Phase 7: the screen-inert behavior is already covered by
  `screen_attachment_ignores_panel_anchor_pose_until_screen_pose_resolution_lands`
  in `screen_space/anchoring/mod.rs` — Phase 7 must update or replace that test when
  it makes the screen pose live.
- Phase 8: the world animation demos (capabilities 2, 3) can drive `PanelAnchorPose`
  now; the resolver consumes it as the sole transform writer.

#### Phase 6 Review

- Phase 7 (struct): grew the planned `ResolvedScreenPanelPosition` to add
  `authored_rotation: Option<f32>` alongside `rotation` — the cited depth precedent
  is a restore *pair* (`depth`/`authored_depth`), so restore-on-release needs the
  matching authored field; a lone `rotation` field can't meet the "authored rotation
  restored" acceptance bullet.
- Phase 7 (Layer 1): corrected the pivot to the pinned `source_anchor` point
  (`target_point`), not `anchor_position` — the two diverge when
  `source_anchor != panel.anchor()`; added an acceptance bullet for that case.
- Phase 7 (Layer 2): named the real seam — `ScreenPanelRect` has no `point()`;
  oriented math computes in the placer to leave public `PanelScreenBounds::point`
  unchanged (flagged as an API gate if it instead grows an oriented method), and the
  chain angle threads through `with_anchor_position`.
- Phase 7 (constraints): added the Phase 6 y-sign fact (`pose.translation` is +y-up;
  screen is y-down, so `translation.y` is negated), the exact shipped `PanelAnchorPose`
  signature, and the instruction to convert the Phase-6 placeholder inert test
  `screen_attachment_ignores_panel_anchor_pose_until_screen_pose_resolution_lands`
  into a live-pose assertion.
- Phase 8: pinned the release handoff to the proven "keep relation active + animate
  `PanelAnchorPose`" path; noted capability 3's per-link hinge frame compounds on the
  parent's plane; recorded that capability 4's `R` reset depends on Phase 7's
  `authored_rotation` restore.
- No user decisions: every finding had a single determined outcome dictated by the
  shipped depth precedent or the existing screen-resolver coordinate frame.

### Phase 7 — screen in-plane rotation and oriented-rect chains · status: todo

#### Work Order

**Goal:** The screen resolver honors `PanelAnchorPose`, locked to the screen plane:
a leaf spins in place about its pin (Layer 1), and a rotated screen target
re-anchors the panels pinned to it (Layer 2), with chains resolving in one update.

**Spec:**

The screen resolver honors `PanelAnchorPose`, locked to the screen plane. A screen
panel rotates and animates in place, and a rotated screen panel re-anchors the
panels pinned to it.

**Locked-to-plane semantics.** The shared screen camera is orthographic and faces
the screen plane, so only rotation about the view normal keeps a panel in that
plane. Project the authored `pose.rotation` to its rotation-about-view-normal — a
single in-plane angle θ. Apply `pose.translation.xy` as an in-plane slide and
`pose.translation.z` through the existing draw-order depth channel (Phase 5).
Out-of-plane rotation components are dropped. Document this as "screen honors
in-plane rotation; out-of-plane rotation has no screen effect" — the panel cannot
leave the plane. It is a defined projection, not an ignore.

**Full-pose `ResolvedScreenPanelPosition`.** Grow the override component to carry
the resolved in-plane angle **and the captured authored rotation** beside the
existing fields. The depth precedent this mirrors is a *pair* — `depth` plus
`authored_depth` — so rotation needs the same pair to restore the user's authored
`Transform.rotation` on release:

```rust
pub(crate) struct ResolvedScreenPanelPosition {
    pub(crate) anchor_position:  Option<Vec2>,
    pub(crate) depth:            Option<f32>,
    pub(crate) authored_depth:   Option<f32>,
    pub(crate) rotation:         Option<f32>, // resolved in-plane angle (radians)
    pub(crate) authored_rotation: Option<f32>, // captured authored z-angle, restored on release
}
```

`position_screen_space_panels` writes `Transform.rotation =
Quat::from_rotation_z(angle)` only when the resolver produced a rotation, otherwise
the authored rotation is kept — the same `None`/`Some` fallback rule x/y/z already
follow. On release (pose removed / relation detached) the placer restores
`authored_rotation` exactly as `authored_depth` restores authored z today
(`screen_space/mod.rs` depth-restore path). The single-writer rule is preserved:
the resolver owns rotation for attached panels; an unattached screen panel's
rotation stays user-owned because placement only writes the fields the resolver
resolved.

**Layer 1 — a leaf spins on its pin.** A screen panel that nothing is pinned to
rotates about its **pinned `source_anchor` point** — the point that lands on the
target (`target_point` in the placer), NOT the panel's configured `anchor_position`.
The two coincide only when `source_anchor == panel.anchor()`; when they differ
(see `attachment_math_handles_different_panel_and_source_anchors`,
`screen_space/anchoring/mod.rs`) the pivot must still be the pinned point. The
placer applies the in-plane rotation about that pinned point: the pin stays fixed
and the panel's top-left re-derives from `pin + R2D(θ) · (top_left − pin)`. The
pinned `source_anchor` point's screen coordinate is invariant under rotation.

**Layer 2 — oriented-rect anchor math.** When a panel that *is* an anchor target
rotates, its anchor points move, so its dependents must track them. The screen
resolver snapshot (`ScreenPanelRect`, `screen_space/anchoring/rect.rs`) carries the
in-plane angle, and the placer's anchor-point lookup becomes the oriented form
`center + R2D(θ) · (anchor.offset(size) − center_offset)`. `ScreenPanelRect` has no
`point()` method today — it produces a `PanelScreenBounds` via `.bounds()` and the
placer calls `target_bounds.point(...)` (`screen_space/anchoring/placement.rs`,
~line 122), where `PanelScreenBounds::point` (`anchor_geometry.rs:343`) is
axis-aligned (`top_left + anchor_offset`). Prefer computing the oriented point in
the placer from the rect's angle + `anchor.offset(size)`, leaving the public
`PanelScreenBounds::point` signature unchanged; **if `PanelScreenBounds` instead
gains an oriented `point` method that is a public-API addition — get approval.**
The chain snapshot must thread the angle: the placer propagates resolved placement
to downstream links via `rects.insert(source, source_rect.with_anchor_position(...))`
(`placement.rs`, ~line 137), so a rotated source that becomes a downstream target
must also carry its in-plane angle into its `ScreenPanelRect` (extend
`with_anchor_position` or add an angle-carrying variant) or the next link reads an
axis-aligned target. A dependent pinned to a rotated target's anchor lands on the
rotated point; chains `A → B → C` of rotated screen panels resolve in graph order,
each link reading its target's oriented bounds.

**Files:**
- `panel/anchoring.rs` — grow `ResolvedScreenPanelPosition` (186-191) with
  `rotation: Option<f32>` and `authored_rotation: Option<f32>` (the restore pair,
  mirroring `depth`/`authored_depth`).
- `screen_space/mod.rs` — `position_screen_space_panels()` (191-254): write
  `Transform.rotation = Quat::from_rotation_z(angle)` only when the resolver
  produced a rotation; capture `authored_rotation` on first resolve and restore it
  on release, exactly as the existing depth-restore path handles `authored_depth`.
- `screen_space/anchoring/rect.rs` — `ScreenPanelRect` (17-22): carry the in-plane
  angle; thread it through `with_anchor_position` (or an angle-carrying variant) so
  a rotated source propagates its angle to downstream links.
- `screen_space/anchoring/{placement,mod}.rs` — read source `PanelAnchorPose`,
  project to in-plane angle θ, apply the Layer 1 pin rotation about the pinned
  `source_anchor` point (`target_point`, not `anchor_position`), and compute the
  Layer 2 oriented anchor point in the placer (`center + R2D(θ)·(anchor.offset(size)
  − center_offset)`), leaving public `PanelScreenBounds::point` axis-aligned.
- `screen_space/anchoring/mod.rs` (tests) — convert the Phase-6 placeholder test
  `screen_attachment_ignores_panel_anchor_pose_until_screen_pose_resolution_lands`
  into a live-pose assertion. That test currently asserts a screen panel's pose is
  inert (`transform.rotation == Quat::IDENTITY`, `anchor_position` unchanged) using
  `rotation: FRAC_PI_2`, `translation: (10,20,30)`; with Phase 7 live that panel
  gains the resolved angle and an in-plane `translation.xy` slide, so the assertion
  must flip.

**Constraints from prior phases:**
- Phase 5: the screen placer tracks depth per entity beside `desired_positions`;
  `ResolvedScreenPanelPosition` already carries `depth` and `authored_depth`, and
  `position_screen_space_panels` writes `translation.z` only when the resolver
  produced a depth. Phase 7 adds the parallel `rotation` + `authored_rotation`
  fields on the same `None`/`Some` capture-and-restore rule. The `authored_rotation`
  half is required to meet the "authored rotation is restored" acceptance bullet —
  a lone `rotation: Option<f32>` has nowhere to stash the user's authored angle.
- Phase 6: `PanelAnchorPose` shipped with the exact signature
  `#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)] { rotation:
  Quat, translation: Vec3 }`; the world resolver reads it via
  `anchor_poses.get(source).copied().unwrap_or_default()` and the screen resolver
  does **not** yet — this phase wires it. Phase 7 does not touch the world resolver.
- Phase 6 y-sign convention: `pose.translation` is authored in plane-frame meters
  with **+y = plane up** (world helper `plane_frame_translation` = `right*x + up*y +
  normal*z`). The screen resolver works in **y-down window pixels** (`anchor_position.y`
  grows downward; `screen_space/mod.rs:238` does `half_height - anchor_position.y`),
  exactly as the static `PanelAnchorOffset` path already differs (`- up*y`).
  Phase 7 must negate `pose.translation.y` when applying the in-plane slide so the
  screen interpretation matches the world +y-up authoring.
- The placeholder screen-inert test from Phase 6 (named in Files) must be converted
  by this phase, not left asserting inertness.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic screen_space::anchoring`
green (with the Phase-6 placeholder inert test converted to a live-pose assertion),
plus:
- a leaf screen panel with a normal-axis pose spins about its pinned `source_anchor`
  point; that pinned point's screen coordinate is invariant before and after
  rotation, including a case where `source_anchor != panel.anchor()`
- an out-of-plane pose rotation has no screen effect; the panel stays axis-aligned
  in the plane (documented projection, not a panic or skip)
- `pose.translation.xy` slides the panel in-plane with +y-up honored against the
  y-down screen frame; `pose.translation.z` routes through the Phase 5 depth channel
- a rotated screen target repositions its dependent onto the rotated anchor point
  (oriented point computed in the placer; public `PanelScreenBounds::point` unchanged)
- a screen chain `A → B → C` with per-link rotation resolves in one update (each
  link's `ScreenPanelRect` carries its angle through the chain snapshot)
- removing `PanelAnchorPose` restores axis-aligned placement: `rotation` returns to
  `None` and `authored_rotation` is restored, the same `None → Some → None`
  transition x/y/z follow

Closeout:
```sh
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic screen_space::anchoring
cargo nextest run -p bevy_diegetic world_anchoring
```

### Phase 8 — animation ordering and the demonstration example · status: todo

#### Work Order

**Goal:** A named `PostUpdate` ordering point for animation systems that write
resolver-read inputs, plus the capability-selector demonstration example with a
world-space explainer panel.

**Spec:**

- Name a `PostUpdate` ordering point for systems that write resolver-read animation
  inputs (`PanelAnchorPose`, relation insert/remove at state boundaries) before the
  world attachment resolver. **If this becomes a public `PanelSystems` variant, get
  explicit API approval during implementation.** Because the world resolver runs in
  `PostUpdate` before `TransformSystems::Propagate`, writes before the resolver
  affect the current frame and writes after it affect the next frame; tests must
  show both.
- Ownership modes for animation systems:
  1. no `AnchoredToPanel` component while the animation writes `Transform`
  2. animation writes `PanelAnchorPose`, and the resolver remains the only
     transform writer
  3. animation inserts or removes the relation only at state boundaries
- Release handoff: the **proven default** is to keep the relation active and animate
  `PanelAnchorPose` — Phase 6 shipped this as the sole-writer path
  (`removing_panel_anchor_pose_returns_to_plain_snap_without_recapturing_authored_pose`),
  and `AnchoredWorldPanelPose` is the authored-pose record restored only on detach
  (`restore_inactive_world_panel_poses`). Use that path. Only if a demo genuinely
  removes an active world relation mid-animation does the same-frame release-pose
  capture protocol apply, so Phase 3 authored-pose restoration does not snap the
  panel away before the animation starts.
- The demonstration grows `examples/panel_anchoring.rs` into a capability-selector
  UI, specified in
  [`panel-anchoring-example.md`](panel-anchoring-example.md): number keys pick the
  active capability, `Space` runs its animation, arrows cycle anchors, `R` resets.
  The capabilities span the feature set across both spaces:
  - `1` world anchor selection (cycle source/target anchors) — exists
  - `2` world lift & spin while attached (z offset + normal-axis pose rotation, pin
    fixed)
  - `3` world hinge chain unwrap (per-link right-axis pose rotation about each
    pinned `BottomCenter`). Each link's plane is its target's (parent's) plane —
    the resolver copies target plane rotation — so a child's `pose.rotation`
    composes on the parent's already-hinged frame; the chain's rotations compound
    down the links. Author the unwrap angles expecting that relative-to-parent frame,
    not a world-absolute one.
  - `4` screen spin-in-place, locked to the plane (Phase 7 Layer 1)
  - `5` screen rotated-target chain (Phase 7 Layer 2: a rotating screen panel drags
    the panels pinned to it)
  - Relations stay active throughout; the animation system writes only
    `PanelAnchorPose` before the resolver, never `Transform`. Do not extend
    `AnchoredToPanel` beyond point-to-point snapping to model hinge motion — point
    snap plus pose rotation covers it.
- The explainer is a **world-space** `DiegeticPanel` placed *behind* the main demo,
  carrying expansive text about the active capability. An orbit-cam control flies
  the camera to it (zoom and pan) to read, then returns to the main event; sitting
  behind the action keeps it from occluding. This likely motivates a reusable
  fairy_dust enhancement — a "look at this panel / return home" camera move usable
  beyond this example. **Get explicit API approval before landing any public
  fairy_dust surface for it.**
- Cover the demos with an `anchor_animation` test filter.

World editable-field popup tracking is not part of the animation phase. If an
example or test touches editable fields on world-anchored panels, document the
existing propagated-transform timing or add a separate editor-specific fix.
Deferred follow-up: if same-frame popup tracking for world-anchored editable fields
becomes required, handle it in an editor-specific task that covers `ime/editor.rs`
after `resolve_world_space_panel_attachments`. Do not mix that work into
`panel_anchoring.rs` or the animation demos.

**Files:**
- `panel/mod.rs` — if a public `PanelSystems` variant is added for the animation
  ordering point (API-approval gate).
- `examples/panel_anchoring.rs` — grow into the capability selector per
  `panel-anchoring-example.md`.
- `fairy_dust/src/{orbit_cam,camera_home}.rs`, `builder/mod.rs` — optional "look at
  panel / return home" camera move (API-approval gate before any public surface).

**Constraints from prior phases:**
- Phase 6: `PanelAnchorPose` exists; the world resolver composes it; the relation is
  the sole transform writer while active.
- Phase 7: the screen resolver reads `PanelAnchorPose` (Layer 1 leaf spin, Layer 2
  oriented chains), enabling capabilities `4` and `5`.
- Capability 4's `R` reset depends on Phase 7's `authored_rotation` restore: when the
  screen pose is cleared the panel must return to its authored rotation. If Phase 7
  ships without the `authored_rotation` capture/restore pair, `R` silently leaves the
  panel rotated. Verify Phase 7 landed that field before relying on cap-4 reset.
- World resolver runs in `PostUpdate` before `TransformSystems::Propagate`: writes
  before it are same-frame, writes after are next-frame.
- Two API-approval gates in this phase: a public `PanelSystems` animation-ordering
  variant, and any public fairy_dust camera "look at panel / return home" surface.

**Acceptance gate:** `cargo check -p bevy_diegetic --example panel_anchoring` and
`cargo nextest run -p bevy_diegetic anchor_animation` green, plus:
- writes to `PanelAnchorPose` before the world resolver affect the current frame;
  writes after it affect the next frame, using the named ordering point
- pose animation is consumed by the resolver rather than writing the same transform
  after the resolver
- capability `2` lift/spin keeps the relation active and never writes `Transform`
  directly
- capability `3` chained unwrap animates per-link pose rotation; relations stay
  active; no extension of `AnchoredToPanel` beyond point snapping
- the static `panel_anchoring.rs` pass documents reset and detach behavior for
  `AnchoredWorldPanelPose`: whether reset removes the relation, refreshes the
  captured authored pose, or leaves the resolver-owned pose intact
- mixed screen/world animation tests are only needed for animation-specific paths;
  do not duplicate the base resolver ownership tests from Phase 3

Closeout:
```sh
cargo +nightly fmt --all -- --check
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
cargo nextest run -p bevy_diegetic
```

## Archive — shipped design and completed phases (1–5)

Everything below is archived reference: the original design narrative for the
shipped foundation (Phases 1–5) and the completed-phase records. The live zone is
the Delegation Context and Phases 6–8 above. The Phase 6–8 narrative in
`## Implementation Phases` is the design source compiled into the Work Orders above
and is marked superseded. Full as-built rationale will be distilled by
`/plan:to_as_built` after Phase 8.

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
span constraint handled by constrained screen sizing. Later phases extend
placement with an optional z offset and a pose (rotation and displacement about
the pinned point); attachments still never compute width or height.

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
    z: Dimension, // added in Phase 5, default zero
}

impl PanelAnchorOffset {
    pub const ZERO: Self;

    pub fn new(x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self;
    pub fn with_z(self, z: impl Into<Dimension>) -> Self; // Phase 5
    pub const fn x(self) -> Dimension;
    pub const fn y(self) -> Dimension;
    pub const fn z(self) -> Dimension; // Phase 5
}
```

The resolver converts the stored dimensions at the point where it knows the
target panel's coordinate system:

- screen-space: target panel layout units, normally logical pixels, top-left
  origin, y down; z resolves to logical pixels of depth, positive toward the
  camera (in front of the target)
- world-space: target panel layout units scaled onto the target panel plane,
  x right and y down; z displaces along the plane normal, positive in front of
  the target surface

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

### Phase 4 — static and keyboard-driven world anchoring example (complete)

`examples/panel_anchoring.rs` covers the Demo 1 behavior from
[`panel-anchoring-example.md`](panel-anchoring-example.md): world-to-world
`AnchoredToPanel` with hot keys cycling the source and target anchors. This
proved the Phase 3 resolver in an example before animation work.

### Phase 5 — anchor z offset (complete)

Add `z: Dimension` to `PanelAnchorOffset`, default zero. `new(x, y)` keeps its
two-argument signature; add a `with_z(impl Into<Dimension>)` builder and a
`z()` accessor. Update `ZERO` and `to_layout_units` accordingly.

World resolution: displace the pinned target point along
`target_plane.normal()`. Convert authored units to meters with the same target
density `target_offset_meters` already uses for x
(`target_size.x / panel_size.x`); panels preserve aspect, so the x and y
densities agree. Positive z moves the source in front of the target plane.

Screen resolution: resolve z to logical pixels like x/y and write the source's
`Transform.translation.z` as target z plus offset z. The shared screen camera
is orthographic (`ScalingMode::WindowSize`), so z never changes apparent size;
it selects draw order. Positive z places the source in front of its target.
Mechanics:

- `ResolvedScreenPanelPosition` carries a resolved depth alongside
  `anchor_position`; `position_screen_space_panels` writes `translation.z`
  only when the resolver produced a depth, otherwise the authored z is kept —
  the same fallback rule x/y already follow.
- `screen_panel_rects` stays x/y-only; anchor-point geometry is unaffected by
  depth. The placer tracks depth per entity beside `desired_positions` so
  chains accumulate z and a dependent of a dependent stacks deterministically.
- Document the usable depth range instead of clamping: the screen camera sits
  at `SCREEN_SPACE_CAMERA_Z` with `far = SCREEN_SPACE_CAMERA_FAR`; offsets
  beyond that range clip.
- Implementation finding: panel children are coplanar with their backing
  (`TEXT_Z_OFFSET = 0.0`) and order via material sort biases, not z steps.
  Batched text carries a 64-unit `Transparent3d` bias (the default text
  draw layer, derived through `DrawOrdinal`),
  and the shared screen camera is a sorted (non-OIT) view — so a back
  panel's text composites over a front panel's backing until the panels'
  depths differ by more than 64 logical pixels. Backing-vs-backing order
  follows z within a few pixels (per-command biases are small). Documented
  on `PanelAnchorOffset` instead of the originally planned sub-pixel
  caveat, which assumed child z stepping that does not exist.

### Phase 6 — anchored rotation (superseded; compiled into the Phase 6 Work Order above)

Add a mutable component the resolver reads beside an active `AnchoredToPanel`.
`PanelAnchorPose` is a **public, user-insertable** component: a user puts it on
an entity they control to rotate and animate an attached panel. The relationship
stays the exact snap constraint — while an `AnchoredToPanel` is active the
resolver is the only transform writer for that panel, and animation writes this
resolver-read component, never `Transform` directly. This phase introduces the
component and wires the **world** resolver; the **screen** resolver reads it in
Phase 7.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Default)]
pub struct PanelAnchorPose {
    /// Rotation about the pinned anchor point, in the target-plane frame.
    pub rotation: Quat,
    /// Translation from the pinned anchor point, in plane-frame meters.
    pub translation: Vec3,
}
```

- Rotation is authored in the target-plane frame (`right`, `up`, `normal`).
  Normal-axis rotation spins the panel coplanar on its pin; right/up-axis
  rotation hinges it off the target surface; an arbitrary `Quat` covers any
  direction.
- The resolver composes `plane_rotation(target_plane) * pose.rotation` and
  keeps the existing translation equation
  `target_point - desired_rotation * source_offset`
  (`world_anchoring.rs::placement`). Because translation is derived from the
  anchor point and the rotated source offset, the pinned `source_anchor` stays
  fixed for any rotation — pivot-at-pin needs no extra math.
- `pose.translation` displaces the pin point in the plane frame after the
  static `PanelAnchorOffset` is applied, in meters: animation code works in
  world units; layout-unit conversion stays an authoring convenience of the
  static offset.
- The component is separate from `AnchoredToPanel` because the relationship is
  `#[component(immutable)]`: re-inserting it every frame to animate would
  churn the `PanelsAnchoredHere` reverse index. The relation states what the
  panel is pinned to; `PanelAnchorPose` states the pose about that pin and is
  mutable for animation.
- `PanelAnchorPose` is honored in both coordinate spaces with different
  interpretations. World (this phase): the full `Quat` in the target-plane
  frame, as above — it can hinge the panel off the target surface. Screen
  (Phase 7): the pose is projected onto the screen plane — in-plane rotation
  about the view normal is honored, out-of-plane rotation is projected out so
  the panel cannot leave the plane. The component is space-agnostic; each
  resolver interprets it. Until Phase 7 lands a screen panel's pose is inert
  because the screen resolver does not yet read it — an unfinished wire
  completed in Phase 7, not a by-design ignore.

This extends the attachment contract from position-only to position plus
orientation. Attachments still never compute width or height.

### Phase 7 — screen rotation (superseded; compiled into the Phase 7 Work Order above)

The screen resolver honors `PanelAnchorPose`, locked to the screen plane. A
screen panel rotates and animates in place, and a rotated screen panel
re-anchors the panels pinned to it.

**Locked-to-plane semantics.** The shared screen camera is orthographic and
faces the screen plane, so only rotation about the view normal keeps a panel in
that plane. Project the authored `pose.rotation` to its rotation-about-view-
normal — a single in-plane angle θ. Apply `pose.translation.xy` as an in-plane
slide and `pose.translation.z` through the existing draw-order depth channel
(Phase 5). Out-of-plane rotation components are dropped. Document this as
"screen honors in-plane rotation; out-of-plane rotation has no screen effect" —
the panel cannot leave the plane. It is a defined projection, not an ignore.

**Full-pose `ResolvedScreenPanelPosition`.** Grow the override component to carry
the resolved in-plane angle beside the existing fields:

```rust
pub(crate) struct ResolvedScreenPanelPosition {
    pub(crate) anchor_position: Option<Vec2>,
    pub(crate) depth:           Option<f32>,
    pub(crate) authored_depth:  Option<f32>,
    pub(crate) rotation:        Option<f32>, // in-plane angle (radians)
}
```

`position_screen_space_panels` writes `Transform.rotation =
Quat::from_rotation_z(angle)` only when the resolver produced a rotation,
otherwise the authored rotation is kept — the same `None`/`Some` fallback rule
x/y/z already follow. The single-writer rule is preserved: the resolver owns
rotation for attached panels; an unattached screen panel's rotation stays
user-owned because placement only writes the fields the resolver resolved.

**Layer 1 — a leaf spins on its pin.** A screen panel that nothing is pinned to
rotates about its own pinned anchor point. The placer applies the in-plane
rotation about the resolved pin: `anchor_position` (the pinned point) stays
fixed and the panel's top-left re-derives from `pin + R2D(θ) · (top_left −
pin)`. The pinned anchor point's screen coordinate is invariant under rotation.

**Layer 2 — oriented-rect anchor math.** When a panel that *is* an anchor target
rotates, its anchor points move, so its dependents must track them.
`ScreenPanelBounds` (`screen_space/anchoring/rect.rs`) carries the in-plane
angle, and `point(anchor)` becomes the oriented form `center + R2D(θ) ·
(anchor.offset(size) − center_offset)`. A dependent pinned to a rotated target's
anchor lands on the rotated point; chains `A → B → C` of rotated screen panels
resolve in graph order, each link reading its target's oriented bounds.

Constraints from prior phases: Phase 5 added the depth channel and per-entity
depth tracking in the screen placer; Phase 6 added `PanelAnchorPose` and the
world resolver. Phase 7 extends the screen resolver/placer
(`screen_space/anchoring/`, `screen_space/mod.rs`) and `ScreenPanelBounds`
(`screen_space/anchoring/rect.rs`); it does not touch the world resolver.

Acceptance gate (tests listed under Phase 7 tests):

- a leaf screen panel with a normal-axis pose spins about its pin; the pinned
  anchor point's screen coordinate is invariant
- an out-of-plane pose rotation has no screen effect; the panel stays in the
  plane (documented projection, not a panic or skip)
- a rotated screen target repositions its dependent onto the rotated anchor
  point
- a screen chain `A → B → C` with per-link rotation resolves in one update
- removing `PanelAnchorPose` restores axis-aligned placement: `rotation`
  returns to `None` and the authored rotation is restored, the same
  `None → Some → None` transition x/y/z follow

### Phase 8 — animation (superseded; compiled into the Phase 8 Work Order above)

- Name a `PostUpdate` ordering point for systems that write resolver-read
  animation inputs (`PanelAnchorPose`, relation insert/remove at state
  boundaries) before the world attachment resolver. If this becomes a public
  `PanelSystems` variant, get explicit API approval during implementation.
  Because the world resolver runs in `PostUpdate` before
  `TransformSystems::Propagate`, writes before the resolver affect the current
  frame and writes after it affect the next frame; tests must show both.
- Ownership modes for animation systems:
  1. no `AnchoredToPanel` component while the animation writes `Transform`
  2. animation writes `PanelAnchorPose`, and the resolver remains the only
     transform writer
  3. animation inserts or removes the relation only at state boundaries
- Release handoff: if an animation removes an active world relation and wants
  to start from the resolved attached pose, define the handoff before writing
  the demo. Either keep the relation active and animate `PanelAnchorPose`, or
  add a same-frame release-pose capture protocol so authored-pose restoration
  from Phase 3 does not snap the panel away before the animation starts.
- The demonstration grows `examples/panel_anchoring.rs` into a
  capability-selector UI, specified in
  [`panel-anchoring-example.md`](panel-anchoring-example.md): number keys pick
  the active capability, `Space` runs its animation, arrows cycle anchors, `R`
  resets. The capabilities span the feature set across both spaces:
  - `1` world anchor selection (cycle source/target anchors) — exists
  - `2` world lift & spin while attached (z offset + normal-axis pose rotation,
    pin fixed)
  - `3` world hinge chain unwrap (per-link right-axis pose rotation about each
    pinned `BottomCenter`)
  - `4` screen spin-in-place, locked to the plane (Phase 7 Layer 1)
  - `5` screen rotated-target chain (Phase 7 Layer 2: a rotating screen panel
    drags the panels pinned to it)
  - Relations stay active throughout; the animation system writes only
    `PanelAnchorPose` before the resolver, never `Transform`. Do not extend
    `AnchoredToPanel` beyond point-to-point snapping to model hinge motion —
    point snap plus pose rotation covers it.
- The explainer is a **world-space** `DiegeticPanel` placed *behind* the main
  demo, carrying expansive text about the active capability. An orbit-cam
  control flies the camera to it (zoom and pan) to read, then returns to the
  main event; sitting behind the action keeps it from occluding. This likely
  motivates a reusable fairy_dust enhancement — a "look at this panel / return
  home" camera move usable beyond this example. Get explicit API approval before
  landing any public fairy_dust surface for it.
- Cover the demos with an `anchor_animation` test filter.

World editable-field popup tracking is not part of the animation phase. If an
example or test touches editable fields on world-anchored panels, document the
existing propagated-transform timing or add a separate editor-specific fix.

Deferred follow-up: if same-frame popup tracking for world-anchored editable
fields becomes required, handle it in an editor-specific task that covers
`ime/editor.rs` after `resolve_world_space_panel_attachments`. Do not mix that
work into `panel_anchoring.rs` or the animation demos.

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

### Phase 5–8 tests / examples

Phase 5 — z offset:

- world z offset displaces the dependent along the target plane normal by the
  authored dimension converted with the target density; literal expected
  translations for `Mm` and bare layout-unit values
- world z offset composes with x/y offset and non-center anchors; the pinned
  anchor point projects back onto the authored target point
- rotated target plane: z offset follows the rotated normal, not world Z
- screen z offset writes `translation.z` relative to the target's z; positive
  offset places the source in front
- screen chains accumulate depth: `A -> B -> C` with z offsets stacks
  deterministically in one update
- removing the attachment clears the resolved depth and restores the authored
  screen z, same `None -> Some -> None` transitions as x/y
- zero z offset leaves `translation.z` untouched for unattached panels and
  panels authored without depth

Phase 6 — anchored rotation (world):

- normal-axis `PanelAnchorPose` rotation spins the dependent coplanar while
  the pinned `source_anchor` point stays fixed (assert the world-space anchor
  point before and after rotation)
- right/up-axis rotation hinges the dependent off the target plane; pinned
  point still fixed
- `pose.translation` displaces the pin in the plane frame in meters, after the
  static `PanelAnchorOffset`
- pose composes with target rotation: a rotated target plane plus a pose
  rotation produces `plane_rotation * pose.rotation`
- removing `PanelAnchorPose` returns the dependent to the plain snapped
  placement without touching the captured `AnchoredWorldPanelPose`
- a screen panel's `PanelAnchorPose` has no effect in this phase; the screen
  resolver does not read it until Phase 7

Phase 7 — screen rotation (locked to plane):

- a leaf screen panel with a normal-axis pose spins about its pin; the pinned
  anchor point's screen coordinate is invariant before and after rotation
- an out-of-plane pose rotation has no screen effect; the panel stays
  axis-aligned in the plane (documented projection, not a panic or skip)
- a rotated screen target repositions its dependent onto the rotated anchor
  point (oriented `ScreenPanelBounds::point`)
- a screen chain `A -> B -> C` with per-link rotation resolves in one update
- removing `PanelAnchorPose` restores axis-aligned placement: `rotation`
  returns to `None` and the authored rotation is restored, the same
  `None -> Some -> None` transition x/y/z follow

Phase 8 — animation:

- writes to `PanelAnchorPose` before the world resolver affect the current
  frame; writes after it affect the next frame, using the named ordering point
- pose animation is consumed by the resolver rather than writing the same
  transform after the resolver
- Demo 2 lift/spin animation keeps the relation active and never writes
  `Transform` directly
- Demo 3 chained unwrap animates per-link pose rotation; relations stay
  active; no extension of `AnchoredToPanel` beyond point snapping
- the static `panel_anchoring.rs` pass documents reset and detach behavior for
  `AnchoredWorldPanelPose`: whether reset removes the relation, refreshes the
  captured authored pose, or leaves the resolver-owned pose intact
- mixed screen/world animation tests are only needed for animation-specific
  paths; do not duplicate the base resolver ownership tests from Phase 3
- demos are covered by an `anchor_animation` test filter

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

Phase 5 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic panel::anchoring
cargo nextest run -p bevy_diegetic screen_space::anchoring
cargo nextest run -p bevy_diegetic world_anchoring
```

Phase 6 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic world_anchoring
cargo nextest run -p bevy_diegetic screen_space::anchoring
```

Phase 7 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic screen_space::anchoring
cargo nextest run -p bevy_diegetic world_anchoring
```

Phase 8 closeout:

```sh
cargo +nightly fmt --all -- --check
cargo check -p bevy_diegetic --example panel_anchoring
cargo nextest run -p bevy_diegetic anchor_animation
cargo nextest run -p bevy_diegetic
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
