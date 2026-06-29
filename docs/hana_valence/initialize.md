# hana_valence — initialization plan

Status: design / not started. A new shape-agnostic crate extracted from the
anchoring machinery currently living in `bevy_diegetic`.

## Name (README seed)

In chemistry, an atom's **valence** is its capacity to bond: the number and
arrangement of connection points it offers to the world. This crate gives shapes
the same thing — programmable anchor points by which they bond to one another,
assemble into larger structures, and animate as those bonds form, break, and
reconfigure: magnetizing together, folding into volumes, articulating. The
metaphor is inherently spatial, so it carries cleanly from 2D into 3D, and it
follows the workspace's naming convention of borrowing one precise term from an
outside field — diegetic (film theory), Lagrange (orbital mechanics), liminal
(anthropology), valence (chemistry).

One-liner: *hana_valence — shapes expose connection points and bond into
animatable assemblies; named for valence, an atom's capacity to bond.*

Note on vocabulary: the crate is `hana_valence`, but its types keep the
**anchor** noun (`AnchorId`, `AnchoredTo`, `AnchorPose`, `ResolvedAnchorGeometry`,
`AnchorSystems`) — an anchor *point* is the concrete connection site; *valence*
is the capacity those points add up to. Bond-flavored verbs (`magnetize`, fold,
hinge) name the behaviors built on top.

## Goal

A standalone crate that attaches **anchors** to any tileable shape (quad,
triangle, pentagon, hexagon, arbitrary polygon) and resolves
**anchor-to-anchor attachments** into transforms, composing down chains. On top
of that primitive it provides **named arrangements** (Accordion, Strip, Ring,
box/polyhedral nets) whose animations are driven by **any animator** —
bevy_tween, bevy_animation, or hand-written systems — through a single
component-write seam.

`bevy_diegetic` becomes one *provider* of anchor geometry (panels), not the
owner of the anchor system. Triangles, hexagons, and procedural shapes are other
providers. The fold/placement math lives once, in `hana_valence`, and is reused
across every shape.

## Layering — what is inside vs outside

Three concerns, kept separate (collapsing the last two is the trap):

1. **Anchor geometry** — which points a shape exposes (vertices, edge-midpoints,
   centroid) and which pairs form edges. A pure function of a shape outline.
   *Provided by each shape crate.*
2. **Pose / attachment** — a local pose primitive read by a resolver that pins
   one anchor onto another and composes the result down a chain into
   `Transform`. *Owned by `hana_valence`. Shape-agnostic.*
3. **Animation driver** — whatever changes the pose/params over time.
   *Interchangeable: bevy_tween, bevy_animation, or user systems.*

Crate dependency direction:

```
hana_valence   (geometry contract + resolver + pose + arrangements + recipes)
   ▲                    ▲
   │                    │
bevy_diegetic       (triangle / hex / procedural shape crates)
(panel provider,    (other providers)
 anchor feature)
        ▲
        │
   user / examples  (Anchor::TopLeft ergonomics + choose a driver)
```

`hana_valence` knows **nothing** about panels or any concrete shape. The contract
between a shape and the engine is a **component**, not a runtime trait call — see
"The contract is a component."

## The contract is a component (not dyn dispatch)

The resolver never calls a trait at runtime. Each shape crate runs its own system
that fills a component the resolver reads:

```rust
// hana_valence
pub enum AnchorId { Vertex(u32), EdgeMid(u32), Center }

#[derive(Component)]
pub struct ResolvedAnchorGeometry {
    pub points: HashMap<AnchorId, Vec3>,   // local frame
    pub edges:  Vec<(AnchorId, AnchorId)>, // shared-edge lookup for folding
}
```

- One `ResolvedAnchorGeometry` **per entity** (per panel, per triangle). 1000
  shapes = 1000 components, ~9 entries each. Never a global table; the resolver
  reads only a child's own component and its parent's, so cost scales per
  relation, not per total anchors.
- An optional authoring helper trait `AnchorGeometry { fn anchor_point(&self, id)
  -> Option<Vec3> }` may exist for a provider to *compute* points internally, but
  it is not a dispatch point — the resolver only reads the component.

## Core types (hana_valence)

```rust
#[derive(Component)]
pub struct AnchoredTo {            // the relation: glue source anchor onto target's anchor
    pub target:       Entity,
    pub source:       AnchorId,
    pub target_anchor: AnchorId,
    pub offset:       Vec3,
}

#[derive(Component, Default)]
pub struct AnchorPose {            // the driven local pose — the animation seam
    pub rotation:    Quat,
    pub translation: Vec3,
}

#[derive(Component)]
pub struct Hinge {                 // scalar fold about a shared edge; converter writes AnchorPose
    pub edge:  (AnchorId, AnchorId),
    pub angle: f32,
}

#[derive(Component)]              // optional cached world-space points for gizmos/UI
pub struct ResolvedAnchorWorld { /* world points */ }

#[derive(SystemSet)]
pub enum AnchorSystems {
    AnimatePose,  // ordering point: drivers write AnchorPose / Hinge.angle / Transform here
    Resolve,      // resolve_anchors reads geometry + relation + pose, writes Transform
}
```

### The resolver

```
target_world = parent.global * parent.geometry[target_anchor]
source_local = child.geometry[source]
rot          = parent.global.rotation * pose.rotation        // pose pins the anchor
child.translation = target_world + rot * offset − rot * source_local
child.rotation    = rot
```

Sole `Transform` writer for anchored entities; composes down chains. This is the
math currently in `bevy_diegetic`'s `world_anchoring.rs` resolver — it moves to
`hana_valence` unchanged in spirit.

### Hinge → pose converter (the universal fold)

```rust
fn hinge_to_pose(q: Query<(&Hinge, &ResolvedAnchorGeometry, &mut AnchorPose)>) {
    let (a, b) = (geom.points[&hinge.edge.0], geom.points[&hinge.edge.1]);
    pose.rotation = Quat::from_axis_angle((b - a).normalize(), hinge.angle);
}   // runs in AnchorSystems::AnimatePose, before Resolve
```

A fold is about an **edge**, not "top/bottom." Generalizing the old
`Quat::from_rotation_x` to `from_axis_angle(edge_axis, angle)` makes quads,
triangles, pentagons, hexagons all fold with the same code. The only per-shape
input is which two vertices form the hinge edge — and that comes from
`ResolvedAnchorGeometry.edges`.

`Hinge.angle` being a plain `f32` is deliberate: it is the easiest possible
target for any animator.

## The animation seam — "any animator"

`hana_valence` exposes **targets to write**, not the writing. Three inputs an
animator can drive, all read in `AnchorSystems::AnimatePose` and consumed by
`Resolve` the same frame:

- `AnchorPose { rotation, translation }` — direct local pose
- `Hinge { angle }` — scalar fold (converter turns it into a pose)
- `Transform` — for fly-together translation before an attachment locks

Driver options (engine unchanged for all):

- **Manual system** — mutate the component in `AnimatePose`. Same as the current
  hinge demo.
- **bevy_tween** — one `Lens` per input component is the entire integration:
  `HingeAngleLens`, `AnchorPoseLens` (slerp rot / lerp trans); `Transform` has a
  built-in lens. bevy_tween owns *when/how* (timers, easings, sequences);
  staggered start delays read as a propagating unfold "path."
- **bevy_animation** — expose the same fields as `AnimatableProperty` /
  `animated_field!(Hinge::angle)`. Fits when choreographies are authored ahead
  and want graph blending; awkward for procedurally-spawned nets (clip +
  named-hierarchy retargeting ceremony). Not the primary path, but free to add.

Published contract for animators: *(1) these components are the tweenable
targets, (2) write them in `AnchorSystems::AnimatePose`, (3) here are ready-made
Lenses/AnimatableProperties for the common animators.*

## Arrangements — named moves over an ordered set

An **Arrangement** is a named rule over an ordered set of anchored entities that
owns both placement and the drivable parameters:

- `placement(i) → (AnchoredTo, hinge edge, rest pose)` — where tile `i` seats
  relative to `i−1`. Used by spawn-markers for auto-placement.
- exposed params — the named scalars an animator drives, e.g.
  `Accordion { fold: 0..1, lean, pattern }`, `Strip {}`, `Ring { closure }`.

Both static layout and animation fall out of the same rule. Drive one number
(`fold`) and the whole set folds.

### Shape-generality split (the key insight)

- **Folding is universal** — rotate about the shared edge; identical for every
  polygon.
- **Tiling / rest-layout is per-shape** — *where* slot `i` sits and *which* edge
  is shared differs: quads share parallel edges (straight strip), triangles
  **alternate up/down** (shared edge + flip change each step), hexes zigzag. An
  Arrangement takes a small per-shape **tiling rule** (`next_edge(i)`,
  `rest_delta(i)`) supplied by the shape provider (~10 lines each).

So `Accordion` = universal fold math + a shape's tiling rule. Quad/triangle/hex
each supply a tiny rule and the ergonomic `Accordion::new(tiles)` then works
across all of them with the same drivable `fold`.

### Spawn markers (anchor intercedes for placement)

Spawn a tile with `Member { arrangement }`; an observer reads the current count,
assigns index `i`, applies `placement(i)` (relation + rest pose), and — if a fold
is in flight — seats it at the live angle. A new instance joins mid-animation
automatically and animates with the full set.

### Nets — box / polyhedral folding

A polyhedral net is a **tree** of edge-shared tiles, each with an `AnchoredTo`
pinning one edge to a parent edge and a target dihedral `Hinge.angle`. Folding =
drive every hinge to its target angle. A shipping box = 6 quads in a cross net at
90°; a tetrahedron = 4 triangles. Same resolver, same pose, same recipe — only
topology and per-edge target angles differ. `Hinge` is therefore **per-relation**
(a tile may be a child on one edge and a parent on several).

### Magnetize (edge behavior, not panel behavior)

`magnetize(group)` finds nearest unpaired edges across loose tiles, creates the
`AnchoredTo` relation, and tweens each transform to close the gap and seat the
edge — pure edge math in `hana_valence`, works for any provider. Decide one knob:
locked tiles stay rigid, or become a hinge net so "magnetize then fold" composes.

## Naming vs indexing

- The engine **never uses names** — `AnchoredTo`, `Hinge`, the resolver, the
  recipes all speak raw `AnchorId` (`Vertex(2)`, `EdgeMid(0)`).
- Names are per-shape authoring sugar, optional and cheap. Provide them only when
  a shape has a small meaningful fixed set: quad → `Anchor::TopLeft…`; triangle →
  maybe `Apex/BaseLeft/…`; hexagon → probably leave unnamed (`Vertex(0..6)`).
- **Recipes never hardcode ids** — a reusable accordion asks geometry "what edge
  do I share with my parent?" and gets `(AnchorId, AnchorId)`. Names/explicit ids
  appear only when a human hand-authors a specific net; generated nets derive
  hinge edges from adjacency.

Three tiers, pick per case: (1) generated/procedural — no names; (2)
hand-authored regular shape — names if offered; (3) one-off — raw `AnchorId`.

## bevy_diegetic as a panel provider

Behind an `anchor` feature (on for our apps, off by default for outside users; a
later semver bump can flip the default):

1. depend on `hana_valence`.
2. map vocabulary to ids:
   ```rust
   impl From<Anchor> for AnchorId {
       fn from(a: Anchor) -> AnchorId {
           match a {
               Anchor::TopLeft      => AnchorId::Vertex(0),
               Anchor::TopRight     => AnchorId::Vertex(1),
               Anchor::BottomRight  => AnchorId::Vertex(2),
               Anchor::BottomLeft   => AnchorId::Vertex(3),
               Anchor::TopCenter    => AnchorId::EdgeMid(0),
               Anchor::CenterRight  => AnchorId::EdgeMid(1),
               Anchor::BottomCenter => AnchorId::EdgeMid(2),
               Anchor::CenterLeft   => AnchorId::EdgeMid(3),
               Anchor::Center       => AnchorId::Center,
           }
       }
   }
   ```
3. the bridge system — the only thing that makes a panel "a shape":
   ```rust
   fn write_panel_anchor_geometry(
       panels: Query<(Entity, &DiegeticPanel), Or<(Changed<DiegeticPanel>, Changed<Transform>)>>,
       mut commands: Commands,
   ) {
       // from panel W×H (+ plane), compute 9 local points + 4 edges,
       // insert ResolvedAnchorGeometry { points, edges }
   }
   ```
   Replaces today's `ResolvedPanelAnchorGeometry` — same math, new component type.
4. scheduling glue: `write_panel_anchor_geometry.before(AnchorSystems::Resolve)`;
   forward the existing `PanelSystems::AnimateAnchorPose` to
   `AnchorSystems::AnimatePose` so pose drivers keep same-frame timing.
5. optional ergonomic constructor so users never touch `AnchorId`:
   ```rust
   AnchoredToPanel::new(parent, Anchor::TopLeft, Anchor::TopRight)
   // → AnchoredTo { source: a.into(), target_anchor: b.into(), .. }
   ```

Panel user code is unchanged from today — `Anchor::TopLeft` stays the public
vocabulary; `AnchorPose` is used directly from `hana_valence`. **diegetic does not
re-export or drive the pose system.**

## Worked example — two quads, TopLeft glued to TopRight

1. provider fills each panel's `ResolvedAnchorGeometry` (local frame, centered):
   `Vertex(0)=(-W/2,+H/2,0)`, `Vertex(1)=(+W/2,+H/2,0)`, …, `Center=(0,0,0)`.
2. relation: `child: AnchoredTo { target: parent, source: Vertex(0),
   target_anchor: Vertex(1), offset }`.
3. resolver places child so its TopLeft lands on parent's TopRight, then
   `AnchorPose.rotation` swings the child about that pinned point — the hinge
   fold.

The resolver only ever indexes `geometry[id]`; "TopLeft means a corner" lives
entirely in the provider (it placed `Vertex(0)`) and the `Anchor → AnchorId` map.
Swap a triangle provider in and the same resolver folds triangles unchanged.

## Migration from bevy_diegetic

The pieces to extract / generalize (already exist in some form):

- `panel/world_anchoring.rs` resolver → `hana_valence` `resolve_anchors`
  (generalize `from_rotation_x` → `from_axis_angle(edge_axis, …)`).
- `ResolvedPanelAnchorGeometry` → `ResolvedAnchorGeometry { points, edges }`.
- `PanelAnchorPose` → `AnchorPose` (re-exported, not panel-owned).
- `AnchoredToPanel` → thin sugar over `hana_valence::AnchoredTo`.
- `PanelSystems::AnimateAnchorPose` → forwards to `AnchorSystems::AnimatePose`.
- the `panel_anchoring` example's hinge/spin/morph become `hana_valence`
  arrangement recipes + a chosen driver; they stay as examples, not library.

## Open questions

- Does `magnetize` leave locked tiles rigid or convert them to a hinge net?
- `ResolvedAnchorGeometry` returns local points + entity transform applies world
  placement — confirm no shape needs world-aware geometry.
- `collect_tiles_by_order` (example) assumes contiguous orders; the general
  membership/spawn-marker observer must not rely on that invariant.
- Triangle tiling rule: confirm the up/down alternation (shared edge + flip)
  generalizes cleanly to the `next_edge(i)` / `rest_delta(i)` interface.
- Whether arrangement recipes live in `hana_valence` core or a sibling
  `hana_valence_fold` / `hana_valence_tween` glue crate.

## Team review — auto-recorded (mechanical / converged)

Findings with a single sensible in-intent outcome. These refine the plan; apply
when implementing.

- **M1 — resolver extraction is adapted, not copy-paste.** The doc's "moves
  unchanged in spirit" overstates it: the current `world_anchoring.rs` resolver
  looks up `PanelPlane.point(anchor)` and applies offsets in plane-frame basis
  (`plane.right/up/normal`). Generalizing means replacing those with
  `geometry[AnchorId]` lookups and deriving the basis from `Transform`. The math
  (pin source onto target, compose down chain, topological order) is preserved;
  the geometry access path changes.
- **M2 — `ResolvedAnchorGeometry` is a contract; make it enforce.** Provider MUST
  fill every point referenced by `edges`; the resolver and `hinge_to_pose` only
  look up ids known present. Add debug-only validation (edges reference existing,
  distinct points; points finite). The optional `AnchorGeometry` authoring trait
  returns `Result<_, GeometryError>` so a bad provider fails loudly, not with
  silent NaN geometry.
- **M3 — degenerate/scope constraints.** Require distinct hinge endpoints (a≠b,
  else `from_axis_angle` gets a zero axis → NaN). Anchors must be discrete and
  coplanar / on-surface. Defer circles & curved boundaries to future work (drop
  "maybe circles" from the headline claim). Coplanar stacked tiles need a small
  provider z-offset or separate render layers to avoid z-fighting.
- **M4 — frame/ordering contract (port existing).** Resolve must topologically
  sort so each parent resolves before its children within one pass (this is what
  `resolve_panel_attachments` already does — chaining tests confirm 3-level
  chains resolve in one frame). `hinge_to_pose` runs in `AnimatePose` strictly
  before `Resolve`; writes in `AnimatePose` land the same frame (existing test
  `pose_written_in_animation_set_lands_this_frame` validates it).
- **M5 — Transform ownership rule.** The resolver is the *sole* `Transform`
  writer for entities that have `AnchoredTo`. Drivers may write `Transform`
  directly only on **un-anchored** entities (fly-together translation before the
  `AnchoredTo` relation is added). State this so "fly-together then lock" has no
  write race.
- **M6 — hinge frame semantics (document + trace).** Edge endpoints `(a,b)` are
  child-LOCAL; `pose.rotation` folds about the local edge axis; the resolver
  composes `parent.global.rotation * pose.rotation`; the pinned point is
  preserved by subtracting `rot * source_local`. `AnchorPose.translation` is
  applied in the parent/plane frame *before* rotation. Add the explicit quad
  trace (verified correct against the existing `BottomLeft` attachment test:
  target at (3,3,0), offset (0.25,−0.5,0) → child at (3.25,2.5,0)).
- **M7 — "any animator" ≠ plug-and-play (sharpen the contract).** bevy_tween
  needs a small authored adapter, not a drop-in. Before publishing the contract,
  add a worked 5-quad staggered-unfold example with concrete `Lens` signatures,
  per-entity tween targeting, and delay composition. Keep bevy_tween as an
  implementation milestone, not a design claim.
- **M8 — define the Arrangement / tiling-rule trait before milestone 5.**
  `placement(i) → (AnchoredTo, Hinge, rest)` over a per-shape tiling rule
  (`next_edge(i)`, `rest_delta(i)`). Validate the triangle up/down alternation
  *first* (already an open question — keep). The `fold`-distribution choice
  (uniform vs cumulative) is already encoded by the existing
  `FoldPattern::Accordion` vs `Coil` in `hinge.rs` — reference it rather than
  re-inventing.
- **M9 — reframe naming tiers as workflows.** auto-generated-from-topology (no
  names) / authored-with-shape-names / one-off raw `AnchorId`, plus a one-line
  pick guide. Clarify that `From<Anchor> for AnchorId` lives in the *quad
  provider* (bevy_diegetic); other shapes supply their own names — it is not a
  generality leak in `hana_valence`.
- **M10 — migration clarity.** Add a "For existing bevy_diegetic users" note:
  `Anchor::TopLeft` and `AnchoredToPanel` are unchanged, no migration required;
  mark `AnchoredToPanel` the recommended panel entry point and `AnchoredTo` the
  shape-agnostic one.

## Team review — cycle 2 additions (auto-recorded)

- **M11 — shape-provider registration seam.** `hana_valence` publishes the
  component contract (`ResolvedAnchorGeometry`) and the system sets; it does NOT
  expose a provider trait/plugin for dispatch. Each provider (incl. bevy_diegetic
  under `anchor`) registers its own `write_<shape>_anchor_geometry` system that
  reads `Changed<Shape>` and writes the component, scheduled before resolve. Add
  a third set `AnchorSystems::FillGeometry` so multiple providers order cleanly:
  `AnimatePose` and `FillGeometry` both before `Resolve`.
- **M12 — resolver reads parent `GlobalTransform`, not just `Transform`.**
  Geometry points are local; world-framing a target anchor needs
  `parent.global_transform * geometry[id]`. Amends M1: the fold math is
  preserved, but the resolver loads `GlobalTransform` and transforms local
  geometry to world explicitly (the old plane-basis path did this implicitly).
- **M13 — schedule/propagate timing.** All geometry-fill systems run before
  `Resolve`; `Resolve` writes `Transform` in `PostUpdate` before
  `TransformSystems::Propagate`. Same-frame `GlobalTransform` reads of anchored
  entities are one frame stale by design (resolve writes local; propagate
  computes global next). Document this guarantee.
- **M14 — runtime guards, not just docs (corrected in cycle 3).**
  `hinge_to_pose` must guard `|b−a| < ε` (degenerate edge → skip + warn, not
  NaN). On scale: the existing resolver already *rejects* non-uniform **parent**
  scale (`validate_supported_parent_transform` in `world_anchoring.rs`) and only
  checks the **child/source** scale for finiteness, applying it component-wise to
  the offset (`* source_scale`). So the open shear is non-uniform **child**
  scale, not parent. Carry the parent guard forward and additionally validate (or
  document as unsupported) non-uniform child scale.
- **M15 — net closure is not enforced.** A tree net's last flap meeting the
  first is a property of topology + target angles, not a resolver invariant;
  cumulative float error can leave a gap. Hand-authored/generated nets must be
  built to close; optionally add a `NetClosure` validator. (Tree topology is the
  chosen model — see PD1 resolution below.)
- **M16 — diagnostics parity (avoid a regression).** Today's
  `WorldAnchorResolveDiagnostics` / `AttachmentResolveDiagnostics<R>` must have a
  `hana_valence` equivalent (`AnchorResolveDiagnostics<R>`, generic over
  skip-reason). Missing geometry and despawned `AnchoredTo` targets currently
  silent-fallback to the last/authored Transform — add warn-on-repeated-skip
  logging so providers can see why a tile isn't moving.
- **M17 — `Member` observer vs geometry-fill ordering.** The auto-seat observer
  runs before `AnimatePose`; if a tile's geometry is not yet filled on its spawn
  frame, it resolves at the authored pose and snaps to the live fold next frame.
  Document, or one-frame-defer first resolution until geometry exists.
- **M18 — type hardening.** Mark `AnchorId` `#[non_exhaustive]` (a future
  `Face(u32)` must not silently break provider matches). Document `Hinge.edge`
  order-significance (axis = `end − start`; swapping endpoints flips fold sign)
  and debug-assert distinct endpoints. Keep `AnchorPose` distinct from
  `Transform` (it is the local-frame seam the resolver converts; merging them
  reintroduces the write race M5 forbids) — state the rationale on the type.
- **M19 — recipe/crate boundary (resolves the doc's last open question).**
  Arrangements (`Accordion`, `Strip`, `Ring`) and the animator adapters
  (`HingeAngleLens`, `AnimatableProperty` impls) live in `hana_valence` core, the
  adapters behind optional `tween` / `animation` feature modules. No separate
  `hana_valence_fold` crate — folding is the core primitive. Procedural shape
  providers (triangle, hex) are separate crates or example code.
- **M20 — headline wording.** Drop "maybe circles" from the goal (deferred per
  M3). Move bevy_tween out of the goal sentence into a post-milestone-3
  implementation; the goal is "one animation seam," animators are swappable.
- **M21 — scalability test + `ResolvedAnchorWorld`.** Add a resolver test with a
  wide and a deep tree (>10 entities) before 1.0 (current chain test is only
  3-deep linear). Decide `ResolvedAnchorWorld`'s ownership: either drop it
  (compute world points on demand in gizmo systems) or have `Resolve` recompute
  it each frame so it can't go stale.

## Team review — cycle 3 additions (auto-recorded)

Cycle 3 verified M11–M21 against the code (M12, M13, M16, PD1-tree CONFIRMED;
M14 corrected above) and added the deliverable-surface items two cycles missed:

- **M22 — milestone 1 needs a concrete standalone test fixture.** "Unit-test
  two-quad attachment with hand-filled geometry (no diegetic dependency)" is the
  gate that proves the crate is actually decoupled. Specify it:
  `#[test] two_quads_top_left_to_top_right()` — build two entities, hand-fill
  `ResolvedAnchorGeometry`, insert `AnchoredTo`, run `resolve_anchors`, assert the
  child `Transform`. If this test needs any panel type, the layering failed.
- **M23 — `Reflect` + type registration is a contract, not optional.** The
  existing `AnchoredToPanel` / `PanelAnchorPose` derive `Reflect` and register
  `ReflectComponent` (there is a test for it). The `hana_valence` core components
  (`AnchorId`, `AnchoredTo`, `AnchorPose`, `Hinge`, `ResolvedAnchorGeometry`) must
  carry the same so BRP inspection and scene serialization of nets work. Add a
  registry round-trip test.
- **M24 — declare a minimal bevy dependency surface.** "Standalone" needs a
  stated dep contract: `hana_valence` should depend only on
  `bevy_ecs + bevy_transform + bevy_math` (not full `bevy`), gated by a
  `cargo check --no-default-features` CI step. A full-bevy dep undercuts the
  standalone claim and inflates the footprint for non-render consumers.
- **M25 — optional debug-draw module.** The panel system has anchor gizmos; the
  generalized crate should publish an optional `debug` module
  (`draw_anchor_geometry`, `draw_relations`, `draw_hinge_axes`) behind a
  `GizmoConfigGroup`, so providers get an authoring aid instead of each
  reimplementing it. Optional feature, not required.
- **M26 — publishing metadata.** Before milestone 1: verify the `hana_valence`
  name on crates.io and set `publish = false` until the API is intended to ship,
  so an accidental publish or name collision can't block integration.
- **M27 — docs deliverables.** Name where things live: `lib.rs` carries the
  public-API overview + the resolver math trace (M6) + the naming-tiers workflow
  (M9); arrangement recipes ship with worked snippets; README mirrors the
  bevy_diegetic one (quick-start, examples dir, bevy compat). `lib.rs` is the
  single source of truth for the public surface.

## Proposed user decisions

Surfaced after the final cycle. Converged items are recorded above and marked
resolved here (kept so a future run does not relitigate them).

- **PD1 — net topology — RESOLVED → tree (auto-recorded).** Cycle 2 consensus
  (Correctness, Architecture, Type System): a **tree** (one `Hinge` per entity at
  its single `AnchoredTo` parent edge) is *required* and *sufficient* — the net
  of any convex polyhedron is a simply-connected planar tree, so boxes,
  tetrahedra, octahedra all fit; DAG support forces a per-edge `HingeAngles` map
  and an extra `AnchoredTo.target_edge` field for generality outside the stated
  intent. Closure caveat recorded as M15. Not surfaced.
- **PD2 — geometry contract — RESOLVED → `HashMap` + validation (auto-recorded).**
  Typed per-shape arrays give compile-time validity but `AnchorId`'s `Vertex(u32)`
  /`EdgeMid(u32)` enum plus the "arbitrary polygon" intent defeat a const-generic
  `Polygon<N>`, reverting to runtime checks anyway. Keep `HashMap<AnchorId,Vec3>`
  + `Vec<edges>` with always-on validation (M2); providers may offer typed
  *builders* that produce it. Optional `EdgeId` indexing is an O(1) optimization,
  not a contract change. Not surfaced.
- **PD3 — multi-driver conflict — RESOLVED → last-writer-wins + debug-warn
  (auto-recorded).** The `AnimatePose`-before-`Resolve` barrier already makes the
  resolver read the final pose; the only real hazard is `Hinge` + a direct
  `AnchorPose` driver on one entity (`hinge_to_pose` overwrites every frame).
  Policy: last-writer-wins, documented ("remove `Hinge` to drive `AnchorPose`
  directly"), with a debug-only warning when both are present. Not surfaced.
- **PD4 — `anchor` feature default / backward-compat (important) — STILL OPEN,
  recommend (a) + split.** Cycle-3 facts: there is no `anchor` feature today;
  anchoring types are exported unconditionally; in-tree consumers are the
  `panel_anchoring` example (~3.3k LOC) *and* `diegetic_text_stress.rs`; both
  world-anchoring systems query **only** entities that carry `AnchoredToPanel` /
  the pose component, so gating them off is zero-overhead for non-anchored panels
  and does **not** change core panel behavior. Options:
  (a) gate panel anchoring behind `anchor`, **off** by default — matches the
  stated intent; the in-tree examples declare `required-features = ["anchor"]`;
  external default builds ship panels without anchoring (clean opt-in);
  (b) gate it, **on** by default — non-breaking now, but flipping to off later is
  a semver break;
  (c) leave panel anchoring **ungated / always-on**; `anchor` only adds the
  `hana_valence` bridge + non-panel shapes (zero migration, small always-present
  API surface).
  Also flagged: **split** PD4 from the hana_valence extraction — the extraction can
  proceed independently; the feature gate applies only to the bevy_diegetic panel
  anchoring machinery. Recommendation: **(a)**, because bevy_diegetic is a
  *provider* of anchor geometry, not the owner, and the resolver is already a
  self-contained opt-in subsystem keyed on `AnchoredToPanel`. Class:
  design-improvement (scope/behavior). Source: Risk vs Architecture (settled to a
  recommendation in cycle 3).
  **Decision: (a) — gate behind `anchor`, off by default.** The maintainer always
  enables it in both primary projects (`required-features = ["anchor"]` on the
  in-tree examples); off-by-default is for external consumers who want panels
  without anchoring. Keep the gate split from the hana_valence extraction.
- **PD5 — magnetize lock behavior (minor).** After `magnetize` seats edges, do
  locked tiles stay rigid, or convert into a hinge net so "magnetize then fold"
  composes? (Also listed in Open questions.) Class: design-improvement. Source:
  Architecture. **Decision: (b) — seated edges become hinges**, so "magnetize
  then fold" composes into one continuous motion.

- **PD6 — re-export vs compose for hana_valence types (emerged during review).**
  With `anchor` on, bevy_diegetic depends on hana_valence. Does diegetic
  `pub use hana_valence::{AnchorPose, Hinge, AnchoredTo, AnchorId}` (one import,
  but hana_valence's types join diegetic's public/semver surface), or stay
  **compose** (current plan — no re-export; the user adds `hana_valence` directly
  and writes `hana_valence::AnchorPose`; clean semver boundary)? Attachment
  vocabulary (`AnchoredToPanel`, `Anchor::TopLeft`) is diegetic-native either way;
  bevy_diegetic never *writes* `AnchorPose` (it writes `ResolvedAnchorGeometry` +
  the `AnchoredTo` relation; the animation driver writes the pose). Class:
  design-improvement (API surface). **Decision: (a) — compose, no re-export.**
  Keep a clean semver boundary; the user imports `hana_valence` directly for pose
  types. Diegetic must keep hana_valence types out of its *public* signatures
  (writing protocol components in internal systems is fine — see note below).

### Note — is it a "leak" when diegetic writes an anchor component?

No. Two different meanings of "leak":

- **API-surface leak (the bad kind):** a crate's *public* signature — a function
  return, a public struct field, a trait bound a consumer must satisfy — names a
  foreign type, forcing every consumer to depend on and know that other crate.
  This is what PD6's re-export branch would create; compose avoids it.
  `AnchoredToPanel` is diegetic-native and exposes no hana_valence type, so its
  public API stays clean.
- **Protocol participation (not a leak):** a system inserts a foreign crate's
  *component* onto an entity for that crate's system to read. In ECS, components
  are a shared public data contract, not an encapsulation boundary. bevy_diegetic
  filling `ResolvedAnchorGeometry` (or `AnchoredTo`) so hana_valence's resolver
  reads it is the same category as any crate writing `Transform` (bevy_transform)
  for transform propagation — composition, not leakage. The doc's "the contract
  is a component" is exactly this: providers fill input components, the resolver
  consumes them.

The dependency flows one way (diegetic → anchor) and the data flows one way
(diegetic produces geometry → anchor consumes it). The only thing to police is
keeping hana_valence types out of diegetic's public *signatures*; writing its
components from internal systems is the intended mechanism.

## Suggested first milestones

1. `hana_valence` crate skeleton: `AnchorId`, `ResolvedAnchorGeometry`,
   `AnchoredTo`, `AnchorPose`, `AnchorSystems`, `resolve_anchors`. Port the
   diegetic resolver. Unit-test two-quad attachment with a hand-filled geometry
   component (no diegetic dependency).
2. `Hinge` + `hinge_to_pose`; test an N-quad straight-strip accordion driven by a
   manual system.
3. bevy_tween adapters (`HingeAngleLens`, `AnchorPoseLens`); reproduce the
   staggered unfold as tween tracks.
4. `bevy_diegetic` `anchor` feature: `From<Anchor>`, `write_panel_anchor_geometry`,
   `AnchoredToPanel`, scheduling forward. Port the `panel_anchoring` example onto
   the new crate.
5. `Arrangement` trait + `Accordion`/`Strip`; `Member` spawn-marker observer.
6. Second shape provider (triangle) to validate the tiling-rule split; box net
   demo.
