# hana_valence — future work

> **Status: BACKLOG — recorded decisions and research, not scheduled.** These
> ideas extend the anchoring and arrangement design described in
> [anchoring-and-arrangements.md](as-built/anchoring-and-arrangements.md).
> Driver systems are generic per rule, and an
> arrangement stores its rule as a component; preserve that model if any item
> below is scheduled.

## Feature extensions

- **Magnetize** — `magnetize(group)` finds nearest unpaired edges across loose
  tiles, creates `AnchoredTo`, and tweens transforms to seat the edge. Seated
  edges become hinges, so magnetize then fold composes. The work is pure edge
  math in valence.
- **Ring arrangement** (`Ring { closure }`) — follows the Phase 8 arrangement
  pattern.
- **Frame-aware hinge axis** — support folding on frame-divergent,
  curved-surface edges after the curved-surface sampler in
  [surface-panels.md](../hana_diegetic/surface-panels.md) fills
  `AnchorPoint.frame` from `SurfaceSample`.
- **Cross-space anchoring** — anchor a screen panel to a world target by
  projecting the world anchor into viewport coordinates before feeding the
  screen placer.
- **Debug gizmo module** — add an optional `debug` feature with
  `draw_anchor_geometry`, `draw_relations`, and `draw_hinge_axes` behind a
  `GizmoConfigGroup`. Feature-gate the `bevy_gizmos` dependency to protect the
  core dependency surface.
- **bevy_animation adapters** — add `AnimatableProperty` or
  `animated_field!(Hinge::angle)` behind an `animation` feature. This suits
  pre-authored choreography with graph blending but is awkward for procedural
  nets.
- **`NetClosure` validator** — optionally check that a net's topology and
  target angles close.
- **Widgets handoff** — bind Phase 1 of
  [widgets.md](../hana_diegetic/widgets.md) to valence. Widget reification
  publishes `ResolvedAnchorGeometry` on materialized widget entities only while
  they are actual targets. World demand comes from `Has<AnchoredHere>`; screen
  demand needs a private diegetic marker maintained from
  `PanelAttachmentAuthored`, because screen sources deliberately do not carry
  the world relation. Widget-side sugar mirrors `AnchoredToPanel::new` but takes
  the widget's existing `PanelElementId`, resolved internally to the stable
  entity. Reification also publishes screen rects from widget bounds plus the
  parent `ResolvedScreenPanelPosition`, feeding the screen attachment path for
  screen-space tooltips. Add a cleanup sweep when a panel leaves screen space.
- **Tetrahedron example** — add four triangles reusing the triangle geometry
  and generic `TilingRule` dispatch. This was the optional Phase 9 stretch and
  was not implemented.

## Maintenance and diagnostics

- **Repeated-skip warning throttling** — `resolve_attachments` currently emits
  `tracing::warn!` once per frame for each persistent repeated skip. Add
  throttling only if this proves noisy in practice.
- **No-default-features rustdoc links** — crate-root links to the tween-gated
  `HingeAngleLens` and `AnchorPoseLens` are unresolved under
  `cargo doc --no-default-features`. Default features include tween, and the
  current dependency-surface gate uses `cargo check`, so this remains outside
  the shipped acceptance gate.

## Verlet dynamics over the anchor graph

This is a research direction recorded 2026-07-07, not a design. Nothing in
the shipped contract depends on it, and nothing in that contract blocks it.

The valence resolver is kinematic: parent pose in, child pose out, one
direction, no state. Verlet integration is a cheap way to add dynamics to this
kind of constraint network. Each particle stores its current and previous
positions, with velocity implicit. Iterative constraint relaxation corrects
both endpoints, adding the two-way coupling that the kinematic resolver does
not provide.

The existing data already supplies the required constraint topology:

- `AnchorPoint`s are constraint attachment sites.
- `AnchoredTo` edges are distance constraints between bodies.
- A `Hinge` edge supplies two shared particles along the pivot line; free swing
  around them is a hinge without special-casing.
- The anchored-to target supplies the pinned particles from which a chain
  hangs.

A hypothetical `hana_verlet` layer would read the anchor graph, build its
particle and constraint set, simulate, and write results either into valence
inputs (`Hinge` angle or `AnchorPose`) for spring-driven secondary motion or
directly into `Transform` for fully simulated bodies. Valence would continue
to resolve the kinematic bodies.

Open research problems:

- **Rigid-body orientation.** A particle has no rotation. A standard approach
  uses three or four particles per panel with rigid mutual-distance
  constraints, then recovers position and rotation from the corner set or
  through shape matching.
- **Stiffness versus cost.** Rigidity depends on relaxation iteration count.
  Low counts create useful droop; stiff chains need enough iterations to avoid
  visible stretch.
- **Collision.** World and panel collision need additional constraint types.
- **Deformation.** Rigid quads remain rigid unless subdivided into particle
  grids, which opens a path to Verlet cloth and banner-like panels.
- **Handoff semantics.** Switching a body between kinematic resolution and a
  simulation that writes `Transform` needs an explicit ownership rule so both
  never write the entity in one frame.

The potential payoff is hanging sign chains, cables between panels, and
cloth-like banners without a physics-engine dependency. It would also create
a second consumer of the geometry contract and exercise the decision to carry
per-point frames.

## Deduplicate `PanelSpace` from `DiegeticPanel.coordinate_space`

This cleanup was recorded 2026-07-09 during the Phase 7 fix pass. It should be
weighed after the migration settles.

### Current state

The world-to-screen conversion path originally left the
`hana_valence::AnchoredTo` tag stale. The generic `resolve_anchors` query then
reported the world-only relation every frame as a missing-geometry skip.
Reconciliation now uses a native insert observer, but `On<Insert>` does not
fire when conversions mutate `DiegeticPanel.coordinate_space` in place.

Phase 7 therefore added a mirror component:

```rust
enum PanelSpace { World, Screen } // Mirrors DiegeticPanel.coordinate_space.
```

The component is synchronized at the write sites: the spawn seed uses
`On<Add, DiegeticPanel>`, conversion apply points insert the corresponding
`PanelSpace` variant, and `On<Insert, PanelSpace>` runs
`on_panel_space_changed` to reconcile the valence tag. This provides a
queryable coordinate-space tag, a native observer hook, and reflect/BRP
visibility at the cost of duplicated state.

`PanelSpace` is always derived from `DiegeticPanel.coordinate_space`. The
invariant therefore depends on every future writer of `coordinate_space` also
inserting the matching `PanelSpace`. An unpaired write silently reintroduces
the stale-tag failure.

### Options

1. **Keep the mirror.** Add a debug assertion or a test that checks
   `coordinate_space` writes for a paired `PanelSpace` insert, and accept the
   duplication as the price of the native observer.
2. **Promote `CoordinateSpace` to a standalone component.** Remove the field
   from `DiegeticPanel` and migrate its readers to `Query<&CoordinateSpace>`.
   This restores a single source of truth and makes `On<Insert>` fire for every
   conversion. It deletes `PanelSpace`, its `From` implementation, the spawn
   observer, and the manual synchronization inserts, but expands the query
   surface across the existing readers.
3. **Insert `DiegeticPanel` wholesale during conversion.** Reconstruct or clone
   the full component instead of mutating the field, allowing
   `On<Insert, DiegeticPanel>` to drive reconciliation without `PanelSpace`.
   This makes conversions heavier and broadens observer firing to other
   in-place panel changes.

Promoting `CoordinateSpace` is the clean end state if coordinate space keeps
gaining consumers. Keeping the mirror remains reasonable if it does not.
