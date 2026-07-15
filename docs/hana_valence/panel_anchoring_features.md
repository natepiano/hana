# Panel anchoring feature plan

## Purpose

The `bevy_diegetic` `panel_anchoring` example contains behavior that is useful
beyond panels. This document inventories that behavior, records the API and
feature review decisions, and accumulates the resulting implementation work in
dependency order. It is the source plan that will later be converted into a
phased work plan after in-scope and out-of-scope work is decided.

## Review preservation contract

The detailed review ledger in this document is authoritative. The cohesion
overview is an additional view over that ledger, not its replacement.

- Do not delete, merge, renumber, or materially rewrite an existing review
  item or recorded decision while constructing the overview.
- Map every overview statement back to the review items that support it so
  omitted concerns remain visible.
- Record possible type removal, type merging, or responsibility movement as a
  proposal. It changes the ledger only after an explicit user decision.
- Preserve a superseded decision in the ledger and point to its replacement
  instead of erasing its history.
- Resume the paused one-item review only after the overview has been checked
  against every foundational, feature, and mutual-exclusion item.
- Introduce each type with one succinct sentence stating what data it contains
  and, when relevant, the exact function or system that reads or writes it.
- Do not use aggregate verbs such as "derive," "drive," or "own" as a substitute
  for naming the components read, the components written, and the function or
  system performing the operation.
- Present only one new type or one exact data-flow step at a time. Do not ask
  for confirmation of a diagram containing undefined types or operations.
- Do not presume that an existing type deserves to survive merely because the
  cohesion overview can explain it.
- Before accepting a new type, state which existing type, field, query branch,
  or system it replaces and report the net change in concepts and systems.
- Compare every proposal with a simpler alternative that uses fewer public
  types or fewer writers. Record removals as proposals until the user decides
  them explicitly under this preservation contract.

The overview may therefore become much smaller than the ledger without making
the work list smaller.

## Cohesion overview status

The cohesion overview is being rebuilt from the smallest end-to-end model of
what an author supplies, which systems read and write that data, what changes
at runtime, and which confirmed type contains each value. A6.1 remains pending
while this work is in progress.

### Simplification gate

The cohesion pass must reduce accidental choices rather than only document
them. These overlaps are visible in the current implementation:

- `ArrangementPlacement` and `MemberPlacement` differ by one live angle value;
  A9.4 removes both placement wrappers, while A8 selects the shared `Angle`
  value used by their surviving connection and hinge data. A6 still decides
  the final connection-result and connected-payload names.
- `TilingRule::rest_delta()`, `Accordion::lean`, and `Coil::lean` contribute to
  the two `Angle` endpoint values that A9.1 decided will live on `Hinge`; A9.2
  must still decide how those values are calculated without adding another
  placement wrapper.
- `drive_arrangement_hinges()`, `actuate_fold_hinges()`, and
  `HingeAngleLens` can all write `Hinge::angle`. A9.1 decided to remove stored
  current-angle state and derive the angle in `hinge_to_pose()`; A9.3 must
  decide how external progress enters the remaining sequence path.
- `Accordion` and `Coil` each mix a folding policy (`lean` and sign behavior)
  with live transport state (`fold`). A9 must test whether separating those
  responsibilities simplifies the whole API before introducing another type.
- `Member` / `Members` and `FoldMember` / `FoldMembers` are two relationship
  pairs with different intended membership meanings. A9 must decide whether
  both meanings are necessary and, if so, make their names unambiguous.

A9.1 resolves current-angle ownership. The remaining candidates are mandatory
comparison points for their later atomic review items, not removal decisions.

### Folding simplification investigation

One review cycle compared the live implementation, Bevy ECS ownership, public
API surface, and every demonstrated fold behavior. Three of four read-only
expert reviews completed; the minimal-model review stalled while waiting on its
own nested review and produced no findings, so the synthesis uses the three
complete reviews plus the local code inventory.

#### Converged evidence

- `drive_arrangement_hinges()`, `actuate_fold_hinges()`, and
  `HingeAngleLens` expose three ways to change the same physical result.
  `Without<FoldAngles>` prevents only one pair of writers from overlapping.
- A member carrying `FoldAngles` without a valid `FoldMember` and ready
  `FoldSequenceState` is excluded from the arrangement writer while the
  sequence writer cannot update it. Component presence therefore selects no
  guaranteed writer.
- `Accordion::fold`, `Coil::fold`, and `FoldSequenceState::position()` are
  three locations for live fold progress. The triangle example carries an
  `Accordion` with `fold == 0.0` but performs all live motion through
  `FoldSequence`; the arrangement progress is present only to satisfy the
  parallel placement API.
- A one-stage `FoldSequence` already represents simultaneous motion: assigning
  every entity `FoldStage(0)` gives every entity the same eased fraction and a
  duration independent of member count. Repeated stages already support the
  box example's simultaneous walls, while distinct stages support staggered
  folding.
- `Member` and `FoldMember` currently identify different controllers, but no
  shipped example requires those controllers to be different. The box proves
  that physical connection order and fold order differ; retaining `FoldStage`
  preserves that distinction without necessarily retaining a second
  membership relationship.
- `Edge`, unfolded angle, folded angle, and resolved progress are sufficient to
  calculate one entity's fold angle. They are not a complete transport:
  pause, target, direction, step/play motion, and pending neutral-state
  reconfiguration remain controller concerns.

#### Box counterexample and unifying direction

The `box.rs` example is not a `Strip`, `Accordion`, or `Coil`. It directly
authors a branched `AnchoredTo` graph and per-connection hinge endpoints, then
uses `FoldSequenceBuilder` to assign the lid to `FoldStage(0)` and all four
walls to `FoldStage(1)`. The center face is fixed, and the lid targets the north
wall through `AnchoredTo` even though the lid moves before that wall. The
example currently carries no `Arrangement`, `Member`, `Strip`, `Accordion`,
`Coil`, or `TilingRule`.

This is a counterexample to treating the three built-in folding components as
the exhaustive kinds of foldable assembly. The proposed unifying principle is
that the resolved assembly graph is primary:

- `Arrangement` identifies one assembly controller;
- `Member` / `Members` identifies every face in that assembly;
- `AnchoredTo` records the physical connection graph independently of member
  order;
- `Hinge` records the two calibrated endpoint `Angle` values for one foldable
  connection;
- `FoldStage` records when that connection moves; and
- `FoldSequence` plus `FoldSequenceState` controls progress through the stages.

Under this direction, `Strip`, `Accordion`, `Coil`, and future polyhedron
folding policies are optional recipes that materialize `Hinge` endpoints. They
are not exhaustive arrangement kinds. A topology provider (`name TBD`) or
direct authoring materializes `AnchoredTo` and the hinge edge. Direct authoring
and recipes converge on the same resolved components.

One controller and one `Member` / `Members` relationship replaces
`FoldMember` / `FoldMembers`: the controller carries the optional
`FoldSequence`, and participating `Member` entities carry `FoldStage` as a
component. A fixed face remains a `Member` without `FoldStage`. This deliberately
supports one active sequence per arrangement; a separate sequence-membership
relationship should return only if a demonstrated feature needs independent
sequence controllers over the same arrangement.

`FoldSequence` remains a separate optional component colocated
with `Arrangement`, rather than an optional field inside it. Component presence
then expresses the playback capability without making `Arrangement` contain an
`Option`, and Bevy can track authored `FoldSequence` changes independently of
arrangement identity.

`FoldSequenceState` and `AnchorPose` are not duplicate per-frame state.
`FoldSequenceState` is one controller-level transport state containing
readiness, progress, target, direction, and motion. `AnchorPose` is a
per-member geometric output containing rotation and translation. One
`FoldSequenceState` can therefore produce many different `AnchorPose` values
through each member's `FoldStage` and `Hinge` endpoints.

Easing is authored as a cascade but is applied only once, while
`hinge_to_pose()` derives a `Member`'s angle. `FoldSequence` supplies the
sequence default and stores any override for one `FoldStage`; an optional
component on one `Member` (`name TBD`) supplies the most-specific override.
The precedence is member override, then `FoldStage` override, then
`FoldSequence` default. `FoldSequenceState` retains raw transport progress and
does not copy or store authored easing. Members with the same `FoldStage` may
therefore use different curves while retaining the same start and end time.
The sequence default and each `FoldStage` may use one plain timing value
(`name TBD`) containing `Duration` and `EaseFunction`. `FoldStage` owns the
shared clock: an individual fold receives distinct timing by occupying a
one-member `FoldStage`, while a `Member` inside a shared stage may override
only easing. Exact type and builder method names remain open under A9.3.

`FoldSequenceState` remains Hana's canonical fold transport. Playback is
initiated and controlled through `FoldCommandEvent`, so an application or an
external animation timeline such as `bevy_tween` can trigger the same command
path. A future continuous external-driver adapter may feed raw sequence
progress into that transport, but it must explicitly replace Hana's native
progress writer while active and must not apply easing a second time. Hana does
not replace its fold-specific transport with `bevy_tween` or
`bevy_time_runner`.

`FoldCommandEvent` carries one of five explicit `FoldCommand` variants:
`Step(FoldDirection)`, `PlayTo(FoldEndpoint)`, `Pause`, `Resume`, or `Reverse`.
`PlayTo` names its terminal destination and is idempotent when repeated.
`Pause` retains the current journey, `Resume` continues it, and `Reverse`
retargets it without changing the raw position or whether it is a step or
continuous play. Reversing while paused leaves the sequence paused; reversing
while idle does nothing. A private paused flag is orthogonal to the retained
`FoldMotion`, and `FoldSequenceState::is_paused()` exposes that state.

An endpoint recipe reconciles only when the recipe, membership, connection, or
relevant geometry changes. Initial reconciliation must materialize valid
`Hinge` endpoints before sequence validation and advancement. Later recipe
changes while folded use the neutral-state reconfiguration tracked by F9
rather than replacing endpoints in the middle of motion.

The smallest candidate for the built-in chain recipes is one optional sum
component (`name TBD`) with `Strip`, `Accordion`, and `Coil` variants or values.
That type would enforce at most one built-in recipe structurally and eliminate
the lifecycle-observer mutex machinery. Its narrow scope would not claim to
enumerate custom or polyhedral endpoint generation, which can materialize the
same `Hinge` data directly.

**Settled constraint:** Keep `FoldStage` as a named newtype because the value
means a playback stage rather than a member position, and equal values express
simultaneous motion. Under the proposed unified membership model it becomes a
component on participating `Member` entities.

**Decided within A9.3:** Resolve easing in this order: optional per-`Member`
override (`name TBD`), per-`FoldStage` override stored once in `FoldSequence`,
then the `FoldSequence` default. Keep raw progress, rather than easing, in
`FoldSequenceState`; `hinge_to_pose()` applies the resolved curve when deriving
the current angle.

**Decided within A9.3:** Keep `FoldSequenceState` as Hana's canonical
transport and use `FoldCommandEvent` as its request path. Other systems may
initiate that transport by triggering commands. Defer continuous
`bevy_tween` progress driving to an adapter that selects one progress writer
and supplies raw progress for Hana's easing cascade.

**Decided within A9.3:** A plain stage-timing value (`name TBD`) contains
`Duration` and `EaseFunction`. `FoldSequence` supplies the default, and one
`FoldStage` may override it. A fold needing its own duration occupies a
one-member `FoldStage`; a `Member` within a shared stage may override only
easing. The authoring API must expose those valid scopes rather than accept and
reject per-member durations at runtime.

**Decided within A9.3:** Replace the context-sensitive directionless
`FoldCommand::Play` with `PlayTo(FoldEndpoint)`. Keep
`Step(FoldDirection)` and add `Pause`, `Resume`, and `Reverse`; all remain
values carried by the single `FoldCommandEvent`. Preserve the current journey
across pause and preserve raw position across reversal.

**Decided within A9.3:** Keep `FoldSequence` as a separate optional component
colocated with `Arrangement`, not an `Option<FoldSequence>` field inside it.
`Arrangement` identifies the controller; `FoldSequence` component presence
adds staged playback and retains independent lifecycle and change detection.

**Decided:** A4.2 no longer requires a complete arrangement to carry `Strip`,
`Accordion`, or `Coil`; directly authored hinge endpoints are valid.

**Still proposed:** Represent the three built-in chain recipes with one
optional sum component (`name TBD`). This consequence is not recorded as
decided yet.

#### Candidate comparison

| Candidate | Result |
|---|---|
| Keep mutable `Hinge::angle` and add disjoint arrangement, sequence, and external writers | Makes ownership explicit but adds an external-angle component and a third writer; it does not reduce the number of mechanisms. |
| Keep `FoldAngles` and add one common angle writer with an explicit progress-source sum type (`name TBD`) | Removes same-field writer overlap but retains two angle-bearing components and adds a source concept. It is viable, but not minimal. |
| Store the two endpoints on `Hinge` and calculate the current angle inside `hinge_to_pose()` | Removes the stored current angle, `FoldAngles`, both built-in angle-writing systems, and the angle-writing tween adapter. It adds no Hana-specific progress or angle-writer type and was selected under A9.1; A8 separately selects shared `bevy_kana::Angle` for the endpoint values. |

The A9.1 decision changes `Hinge` from mutable current state into authored
hinge calibration. The endpoint field names remain undecided:

```rust
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
}
```

`hinge_to_pose()` reads `Hinge` plus the member's optional `FoldStage` and the
`FoldSequenceState` on the controller targeted by `Member::root_entity`. It
calculates the current angle without storing it:

```rust
let fraction = fold_stage
    .and_then(|stage| sequence_states.get(member.root_entity).ok()
        .map(|state| state.stage_progress(*stage))) // method name TBD
    .unwrap_or(0.0);

let angle = hinge.unfolded_angle
    + (hinge.folded_angle - hinge.unfolded_angle) * fraction;
```

A `Hinge` on a `Member` without `FoldStage` remains at its unfolded endpoint. A
fixed hinge, including `Strip`, uses equal endpoints. Simultaneous `Accordion`
and `Coil` motion assigns every controlled member `FoldStage(0)`; staged motion
assigns different or repeated stages. `FoldCommandEvent` controls ordinary
playback through `FoldSequenceState`; A9.3 defers continuous external scalar
control to a future single-writer raw-progress adapter.

The resulting built-in data flow is:

```text
topology provider + folding recipe
    -> AnchoredTo + Hinge { edge, unfolded angle, folded angle }

FoldCommand -> FoldSequenceState position
Member FoldStage + FoldSequenceState position + Hinge
    -> hinge_to_pose() -> AnchorPose
    -> resolve_anchors() -> Transform
```

The core reduction is:

- Hana Valence angle-bearing components and adapters: `Hinge`, `FoldAngles`,
  and `HingeAngleLens` to `Hinge` alone, using the shared `bevy_kana::Angle`
  value;
- live progress locations: `Accordion::fold`, `Coil::fold`, and
  `FoldSequenceState::position()` to `FoldSequenceState::position()` alone;
- angle/pose stages: `drive_arrangement_hinges()`,
  `actuate_fold_hinges()`, and `hinge_to_pose()` to `hinge_to_pose()` alone;
- placement wrappers: `ArrangementPlacement` plus `MemberPlacement` to the
  connected payload of one named A6 connection result (`name TBD`);
- new public types: one shared `bevy_kana::Angle`; no role-specific Hana
  Valence angle wrapper.

This reduction does not remove the transport, pivot-offset geometry,
`AnchorPose`, `AnchoredTo`, arrangement output suspension, or safe neutral-state
reconfiguration. A9.4 later folds the one-field `HingePivot` component into
`Hinge::pivot_offset`; the capability remains while the separate type does not.

#### Investigation decision status

- **A9.1 — Derived hinge angle:** Replace mutable `Hinge::angle` and
  `FoldAngles` with unfolded and folded endpoint fields on `Hinge`; merge angle
  actuation into `hinge_to_pose()` and remove the two built-in angle writers and
  `HingeAngleLens`. The endpoint field names remain undecided. **Status:
  decided.**
- **A9.3 — One high-level transport:** Remove `Accordion::fold` and
  `Coil::fold`; use `FoldSequence` for both one-stage simultaneous and staged
  progress. Add external seeking as a `FoldCommand` variant rather than another
  angle or progress component. **Status: proposed.**
- **A9.2 — Fold-recipe authoring:** The POC roles of `Strip`, `Accordion`, and
  `Coil` are not presumed to be the final model. A9.2 is decomposed into seven
  decisions covering transient recipes, ordered fold groups, the optional
  topology-provider capability, compatibility, Accordion semantics,
  Coil/Wrap clearance, and safe initial or runtime application. **Status:
  pending A6; decomposed into A9.2.1-A9.2.7.**
- **A9.4 — Further removals:** `MemberPlacement`, `ArrangementPlacement`,
  `FoldFromArrangement`, and the snapshot diagnostic family are removed. The
  approved model has neither a stored live angle for `MemberPlacement` nor a
  second membership graph for `FoldFromArrangement` to populate;
  `ArrangementPlacement` is subsumed by the connected payload of the A6
  result. Use the shared `bevy_kana::Angle` for the fixed connection angle,
  recipe offsets, and `Hinge` endpoints. Remove `FoldSystems::Actuate` with
  `actuate_fold_hinges()`; retain `FoldSystems::Advance` as the public
  controller-transport boundary before `hinge_to_pose()`. Remove the obsolete
  `FoldAngleInvalidReason`, `FoldAngleDiagnostic`, and `FoldAngleDiagnostics`
  family; F13 must review unified hinge and anchor diagnostics, including
  non-finite `Angle` failures and warning throttling. Remove the public
  `Hinge::rotation()` method without replacement so `hinge_to_pose()` remains
  the only fold-angle evaluation path. Remove `HingePivot::reference_angle` and
  define its offset at the unfolded `Hinge` endpoint, then merge the remaining
  offset into `Hinge::pivot_offset` and remove `HingePivot`. Declare
  `AnchorPose` as a required component of `Hinge`. Remove the unused
  `AnchorPoseLens` and the resulting empty tween integration. Add the nonempty,
  authoring-only `SharedFoldStage` value. This removes eleven public types and
  adds one, for a net reduction of ten public types; it also removes one public
  system-set variant, one public method, and one unused optional integration,
  without adding a runtime ECS component or relationship. **Status: decided.**

## Current arrangement model

This is the canonical short account of the reviewed design. Keep it current as
the detailed decisions below change. Some approved types and methods described
here are planned API and are not implemented yet.

`ResolvedAnchorGeometry` is the existing per-entity catalog of local anchor
geometry. Its `points: HashMap<AnchorId, AnchorPoint>` maps each existing
`AnchorId` key to an attachment position and orientation; under A7.1, the
planned `AnchorPoint` fields are `position: Vec3` and
`orientation: AnchorOrientation`. Its `edges: Vec<Edge>` lists available local
axes. An `Edge` is an existing value type—not a component or relationship—and
its `start: AnchorId` and `end: AnchorId` refer to two entries in the same
`points` map. The list catalogs available axes; it does not connect entities or
establish arrangement membership.

`AnchoredTo` is the existing immutable Bevy relationship on a source entity
that records one point-to-point attachment. It contains the entity returned by
`AnchoredTo::target()`, `source_anchor: AnchorId`,
`target_anchor: AnchorId`, and `offset: Vec3`; it contains no `Edge`, `Hinge`,
or folding angle. Bevy maintains `AnchoredHere` on the target entity as the
collection of source entities whose `AnchoredTo::target()` points there.
Replacing `AnchoredTo` rather than mutating one field lets Bevy update that
reverse relationship correctly.

`resolve_anchors()` reads the source and target `ResolvedAnchorGeometry`, uses
the two `AnchorId` fields to select their `AnchorPoint` values, applies
`AnchoredTo::offset`, and writes the source entity's `Transform`. It also reads
the optional `AnchorPose` contribution.

`AnchorPose` is the existing resolver-input component containing
`rotation: Quat` and `translation: Vec3`. Animation systems construct or write
`AnchorPose`; `resolve_anchors()` reads it, combines it with the point-to-point
placement, and writes `Transform`. When the component is absent,
`resolve_anchors()` uses `AnchorPose::default()`, which contributes no
additional rotation or translation. This separates animation output from the
final anchored `Transform`: animation systems do not compete with
`resolve_anchors()` for the same component. `hinge_to_pose()` is one exact
`AnchorPose` writer; the next cohesion-overview step defines how `Hinge`
supplies its input.

Geometry adapters write `ResolvedAnchorGeometry`; for panels, the exact writer
is `write_panel_anchor_geometry()`. The current `TilingRule` selects a source
`Edge` and target `Edge`, maps each edge to an `AnchorId`, and uses those two
anchors to construct `AnchoredTo`. Only the source `Edge` is retained in
`Hinge` today. Under A9.1 and A9.4, `Hinge` retains that `Edge`, the unfolded and
folded endpoint `Angle` values (`field names TBD`), and `pivot_offset: Vec3`.
`Vec3::ZERO` means no pivot translation; a nonzero value locates an offset
physical pivot line in the source anchor's tangent frame. `hinge_to_pose()`
calls `Edge::axis()` against the source entity's `ResolvedAnchorGeometry`,
derives the current angle from the endpoints and selected fold progress, and
writes both rotation and pivot-compensating translation to `AnchorPose`.

Under A9.4, `Hinge` declares `AnchorPose` as a Bevy required component. Inserting
`Hinge` therefore inserts `AnchorPose::default()` only when the entity does not
already carry an explicit pose. Ordinary removal of `Hinge` leaves
`AnchorPose` available for another animation writer. `AnchoredTo`,
`ResolvedAnchorGeometry`, `Member`, and `FoldStage` are not required components
of `Hinge`; those capabilities may be absent or arrive later.

`AnchorPoseLens` and `HingeAngleLens` are both removed under A9.4 and A9.1.
No shipped code uses `AnchorPoseLens`, and it cannot safely share an entity with
`Hinge` because `hinge_to_pose()` writes the complete `AnchorPose`. Direct
non-hinge animation remains supported by systems writing `AnchorPose`; future
F2, F3, or F11 adapters must be designed around their demonstrated capture,
coordination, or continuous-motion needs rather than preserving the unused
generic lens.

`Angle` is the planned shared `bevy_kana` value for a signed, unwrapped angular
displacement stored in radians. It is not an ECS component and does not
normalize its value. Explicit constructors and accessors preserve unit meaning;
implicit `f32` conversion and `Deref` are intentionally absent. Spatial
orientation remains represented by `AnchorOrientation`, `Quat`, or `Rot2`, and
dimensionless fold progress remains `f32`.

`TilingRule` is the trait that exists today. Concrete components such as
`QuadTiling` and the example's `TriangleTiling` use it to choose the two tile
edges and anchors that meet, plus the fixed starting `Angle` that aligns them.
The current arrangement code separately inserts `AnchoredTo` on each tile with
the preceding tile as `AnchoredTo::target()`, so it only constructs a linear
chain. A6.0 classifies the redesigned responsibility as a topology provider
(`name TBD`). It owns a complete logical pattern and materializes the physical
connection graph. A6.1-A6.6 decide its exact contract and final type names.

`Strip`, `Accordion`, and `Coil` are existing, separate ECS components. They do
not choose `Member` entities, `AnchoredTo::target()` values, or edges. Under
A9.1, they contribute the endpoint `Angle` values stored on the `Hinge` created from
the connection information: equal endpoints for `Strip`, alternating folded
directions for `Accordion`, and one folded direction for `Coil`. A9.2 decides
the exact endpoint-calculation contract. This folding responsibility remains
separate from the topology provider (`name TBD`) so that one connection
topology can be reused with different folding behavior.

The planned `Arrangement` component identifies a non-spatial assembly
controller entity. A4.2 no longer requires that controller to carry one of
`Strip`, `Accordion`, or `Coil`; an arrangement whose connection and `Hinge`
data is authored directly remains valid. A9.2 decides whether the three
built-in chain recipes become one optional sum component (`name TBD`). A4.3
retains the `commands.spawn_arrangement(...)` extension in principle, but its
parameters must be revised after the optional recipe representation is settled.

Every physical tile carries the planned `Member` relationship pointing to its
arrangement controller, and Bevy maintains `Members` on that controller.
`Member` / `Members` describes arrangement membership only. Physical tile
connections are separate `AnchoredTo` relationships and may form branching
acyclic trees. One or more `Member` entities may have no `AnchoredTo`.

`FoldSequence` is the existing authored timing and easing component and is
colocated with `Arrangement` under A9.3. `FoldStage` is the existing zero-based
stage value; under A9.5 it becomes an optional component on a `Member`, and
members with the same value move simultaneously. `FoldSequenceState` remains
the derived runtime state on the controller. The currently implemented
`FoldMember` / `FoldMembers` relationship and `FoldFromArrangement` request are
removed by A9.5 and A9.4 respectively.

`SharedFoldStage` is the planned nonempty, authoring-only collection of
`Member` entities that should receive one shared `FoldStage`; it is not an ECS
component. `FoldSequenceBuilder::stage()` consumes any value implementing
`Into<SharedFoldStage>`, while fallible construction from runtime collections
rejects emptiness. `finish()` detects duplicate `Entity` values before queuing
any `FoldStage` insertions.

## Review status

- Foundational API review items: 33
- Feature candidates: 15
- Mutual-exclusion decisions: 6, recorded in
  [`mutex_arrangements.md`](mutex_arrangements.md)
- Complete confirmed decisions: 16
- Reopened decision: A4.2; `Arrangement` identity and optional built-in recipe
  cardinality are settled, while the complete validity contract remains open
- Active work: A6.2, how a topology provider identifies source and target
  `Member` entities
- A9.2 fold-recipe authoring: decomposed into A9.2.1-A9.2.7 and pending the A6
  topology-provider contract
- F1, arrangement activation and suspension: paused until the
  foundational arrangement API is settled
- Scope selection: pending review completion
- Phased work plan: not yet created

Foundational API questions discovered during the review are decided here before
the feature candidates resume. Exploratory discussion is not a recorded
decision.

### Decision reporting

Do not report the current decision table while a proposed decision is still
awaiting acknowledgment. After the user acknowledges the proposal and the
decision is recorded, report every current review item in one table with
columns for ID, description, state, and decision. Leave the decision cell blank
for every undecided item, then advance to the next active item.

Every proposed decision includes a small code sample showing the new behavior.
Do not show the existing or before code unless the user requests it.

Before explaining an item, provide the context required to understand it
without relying on memory of the earlier conversation:

1. list only the confirmed decisions that the item depends on;
2. identify relevant types that already exist and distinguish them from names
   or types merely being proposed;
3. define the terms used in the explanation;
4. state the one question being decided now;
5. state which nearby questions are explicitly deferred to later review items.

Only then present the proposed structure and its new-behavior code sample.

Always use the exact type and method names that already exist or have been
confirmed by this review. Do not substitute an untyped shorthand such as
"parent" or "root" for a type-level statement. If a needed type or method name
has not been decided, mark it `(name TBD)` immediately in prose and with a
`name TBD` comment at its first use in a code sample. Describe an existing
relationship through its actual API—for example, the entity returned by
`AnchoredTo::target()`—rather than assigning it an unconfirmed role name.

### Foundational item boundaries

Only the item marked active is under review. The other items are parked so that
explaining one decision cannot silently expand its scope.

| ID | Question | Status |
|---|---|---|
| A1 | Should Bevy relationships maintain arrangement membership? | Decided: yes |
| A2.1 | What should the membership relationship target field be called? | Decided: `root_entity` |
| A2.2 | What should the membership relationship pair be called? | Decided: `Member` / `Members` |
| A3 | Is member order derived, and is `MemberIndex` still needed? | Decided: derive order; omit `MemberIndex` |
| A4.1 | Is the arrangement root a controller entity or the first physical tile? | Decided: controller entity |
| A4.2 | Which component set makes an arrangement root valid? | Reopened: `Arrangement` identity retained; no built-in folding recipe is required; remaining validity contract pending |
| A4.3 | What happens while an arrangement root is incomplete? | Decided in principle: explicit spawn helper and inert diagnosed incompleteness; helper signature pending A4.2 and A9.2 |
| A5 | Where is the boundary between connection rules and fold patterns? | Decided: connection structure and folding behavior remain separate |
| A6.0 | Is the proposed connection-rule responsibility better modeled as a topology provider (`name TBD`) that materializes connections and may expose optional capabilities such as ordered fold groups? | Decided: yes; a topology provider owns a logical pattern, materializes its root-or-connected physical graph, and may expose recipe-compatible fold directions; regular triangle, quad, and hex sheet providers should support arbitrary dimensions and Accordion, Coil, and Wrap, with a strip as the one-dimensional degenerate case |
| A6.1 | Must the topology provider (`name TBD`) decide whether each `Member` has `AnchoredTo` and select its `AnchoredTo::target()`, instead of Hana always selecting the preceding `Member`? | Decided: yes; for every `Member`, the provider chooses either no `AnchoredTo` or one `AnchoredTo` whose target it selects; Hana never infers the preceding `Member` |
| A6.2 | How does the topology provider (`name TBD`) identify `Member` entities and refer to the target `Member` used by `AnchoredTo`? | Active |
| A6.3 | What result type (`name TBD`) represents either a `Member` without `AnchoredTo` or the data needed to add `AnchoredTo`? | Partial: use a named root-or-connected sum type rather than `Option`; exact type, variant, payload, and failure names pending |
| A6.4 | What methods and failure contract does the topology-provider trait (`name TBD`) expose? | Pending |
| A6.5 | How are concrete topology providers (`name TBD`) discovered and scheduled? | Pending |
| A6.6 | What should the topology-provider trait and its related concepts be called? | Pending |
| A7 | What do `AnchorId`, `AnchorPoint`, `Edge`, and `AnchoredTo` mean and which names should change? | Pending |
| A7.1 | How is an anchor's local orientation represented on `AnchorPoint`? | Decided: `orientation: AnchorOrientation`; non-optional identity default |
| A7.2 | Should the `AnchorPoint` type itself be renamed now that it stores position and orientation? | Pending |
| A8 | Are `ArrangementPlacement`, `MemberPlacement`, and raw angle values still needed? | Decided: remove both placement types; use shared `bevy_kana::Angle` for the A6 connected payload's fixed connection angle, recipe offsets, and `Hinge` endpoints; A9.4 later supersedes the planned `HingePivot::reference_angle` conversion by merging the remaining pivot offset into `Hinge` and removing `HingePivot` |
| A9 | How do folding policies, angle ownership, transport timing, and staged `FoldSequence` ordering form one coherent fold pipeline? | Pending; split into A9.1-A9.5 |
| A9.1 | Should `Hinge` store unfolded and folded endpoints while `hinge_to_pose()` derives the current angle instead of storing `Hinge::angle`? | Decided: yes; remove `FoldAngles` and stored current-angle state; endpoint field names remain pending |
| A9.2 | How do folding recipes obtain compatible topology, materialize complete `Hinge` data, and support both initial authoring and runtime re-authoring? | Pending; decomposed into A9.2.1-A9.2.7 and blocked by A6 |
| A9.2.1 | Are folding recipes transient authoring operations whose durable result is complete `Hinge` data rather than a recipe ECS component? | Pending |
| A9.2.2 | What authoring value (`name TBD`) represents ordered, consistently oriented groups of connections that fold together? | Pending |
| A9.2.3 | What optional topology-provider capability (`name TBD`) returns those fold groups, and what must remain available for runtime re-authoring? | Pending |
| A9.2.4 | Which compatibility invariants and failures govern applying a folding recipe to returned fold groups? | Pending |
| A9.2.5 | What geometry-independent behavior defines Accordion across strips, grids, sheets, and other compatible topologies? | Pending |
| A9.2.6 | What distinct behaviors define Coil and Wrap, and what clearance or pivot calibration must a provider guarantee? | Pending |
| A9.2.7 | How do initial recipe application and runtime replacement commit atomically and interact with neutral-state travel? | Pending; coordinated with F9 and A10 |
| A9.3 | How should `FoldSequence`, `FoldSequenceState`, `FoldMember`, and `FoldStage` jointly represent simultaneous, staged, and externally controlled progress, and can any be combined or removed? | Partial; decided within item: keep `FoldSequence` as a separate optional component colocated with `Arrangement`; keep `FoldStage`; use one duration-plus-easing value (`name TBD`) for the sequence default and stage overrides; a distinct-duration fold occupies a one-member stage; only easing may vary per member; keep raw progress and canonical transport in `FoldSequenceState`; use `FoldCommandEvent` carrying `Step(FoldDirection)`, `PlayTo(FoldEndpoint)`, `Pause`, `Resume`, or `Reverse`; defer continuous external driving to a single-writer raw-progress adapter; exact names and remaining questions pending |
| A9.4 | Which existing fold types, fields, and systems can the A9 pipeline combine or remove, and what is its net concept count? | Decided: remove `MemberPlacement`, `ArrangementPlacement`, `FoldFromArrangement`, both obsolete diagnostic families, `FoldSystems::Actuate`, public `Hinge::rotation()`, `HingePivot`, and `AnchorPoseLens` plus its empty tween integration; retain `FoldPlugin`, `FoldSystems::Advance`, `FoldSequenceBuilder`, and `FoldAuthorError`; require `AnchorPose` with `Hinge`; add nonempty authoring-only `SharedFoldStage` and accept it through `Into`; F13 owns replacement diagnostic policy; net ten fewer public types, one fewer public system-set variant, one fewer public method, and one fewer optional integration, with no new runtime ECS component or relationship |
| A9.5 | Is sequence membership distinct from arrangement membership, and if so, what should `FoldMember` / `FoldMembers` be called? | Decided: use only `Member` / `Members`; put optional `FoldStage` directly on participating `Member` entities; remove `FoldMember` / `FoldMembers` |
| A10 | How are connection topology and fold sequencing validated together? | Pending |

### A1. Arrangement membership storage

**Current implementation:** A member carries `Member { arrangement }`.
`ArrangementMembers(Vec<Entity>)` is maintained separately by custom add/remove
observers and index-assignment systems.

**Decision:** Represent arrangement membership as a Bevy relationship. Bevy
maintains the reverse collection when membership is inserted, removed,
retargeted, or discarded. Remove Hana's custom reverse-list maintenance.

The relationship pair is named `Member` / `Members` under A2.2. A3 derives
member position from relationship order and omits `MemberIndex`.

**Status:** Decided.

### A2.1. Membership relationship target field

**Decision:** Name the `Entity` field that targets the arrangement root
`root_entity`.

```rust
pub struct Member {
    #[relationship]
    root_entity: Entity,
}
```

The relationship types are named `Member` and `Members` under A2.2.

**Status:** Decided.

### A2.2. Membership relationship type names

**Decision:** Name the relationship source `Member` and its Bevy-maintained
relationship target `Members`.

```rust
#[relationship(relationship_target = Members)]
pub struct Member {
    #[relationship]
    root_entity: Entity,
}

#[relationship_target(relationship = Member)]
pub struct Members(Vec<Entity>);
```

**Status:** Decided.

### A3. Member order and `MemberIndex`

**Decision:** Derive current member position by enumerating the ordered
Bevy-maintained `Members` relationship target. Do not store a `MemberIndex`
component. Reconsider a stored index only when a concrete future feature
requires semantics that cannot be derived from relationship order.

```rust
for (position, member_entity) in members.iter().enumerate() {
    place_member(member_entity, position);
}
```

Removing a member allows later positions to shift naturally while preserving
the remaining entities' relative order.

**Status:** Decided.

### A4.1. Arrangement identity and physical connection roots

**Decision:** The arrangement root is a controller entity, not the first
physical tile. Every physical tile, including every root of the physical
connection graph, carries `Member` targeting that controller. `Members`
therefore describes arrangement membership only; its order does not choose a
tile's physical predecessor.

Physical connections are represented separately by `AnchoredTo`. They may
branch and may have one or more physical connection roots. A connected net
normally has one such root, while a compound can place multiple disconnected
connection trees under one arrangement controller.

```rust
let arrangement_root = commands
    .spawn((Arrangement, connection_rule, Accordion::default()))
    .id();

for tile in [center, north, south, east, west, lid] {
    commands
        .entity(tile)
        .insert(Member::of(arrangement_root));
}

// Physical topology is independent of membership order.
commands.entity(north).insert(AnchoredTo::new(
    center,
    north_inner_anchor,
    center_north_anchor,
));
commands.entity(south).insert(AnchoredTo::new(
    center,
    south_inner_anchor,
    center_south_anchor,
));
commands.entity(lid).insert(AnchoredTo::new(
    north,
    lid_inner_anchor,
    north_outer_anchor,
));
```

This model can represent ordinary, concave, star, and compound polyhedra when
their geometry providers supply the required face anchors and their connection
rules supply the explicit connection trees. It does not yet represent every
model in the [Polyhedra.net catalog](https://www.polyhedra.net/en/): a moving
closed ring such as a kaleidocycle requires the future closed-loop capability
tracked by F14. `AnchoredTo` remains acyclic; a closure constraint must not be
implemented by creating a relationship cycle.

A6.1-A6.6 will decide how a topology provider (`name TBD`) expresses branches
and multiple physical roots. A9 will decide authored fold stages, including
simultaneous folds. A10 will validate their interaction: connection dependency
order remains separate from animation order, all sequenced entities belong to
the arrangement, and the connection graph can be resolved regardless of the
chosen fold stages.

**Status:** Decided.

### A4.2. Arrangement identity and validity

**Retained decision:** Add an `Arrangement` marker component that identifies an
assembly controller independently of its current members and optional
capabilities.

**Correction decided under A9:** A valid `Arrangement` does not require
`Strip`, `Accordion`, or `Coil`. Those built-in endpoint-generating recipes are
optional, and directly authored `AnchoredTo` and `Hinge` data is equally valid.
The three recipes therefore do not define the exhaustive kinds of arrangement.

```rust
#[derive(Component, Default)]
pub struct Arrangement;

let root_entity = commands.spawn(Arrangement).id();
```

`Members` is Bevy-maintained derived storage rather than an authored validity
component. A complete arrangement may have zero members and therefore no
`Members` component yet. Members can be attached later without coupling their
lifecycle to creation of the controller.

The controller does not require geometry, anchors, spatial transforms,
`AnchoredTo`, or `Hinge`; those components belong to physical members. Whether
a selected topology provider (`name TBD`) has enough input to materialize its
graph remains part of the open validity contract, but a provider is not
required when connections are authored directly. `FoldSequence`, when present,
lives on this same controller as a separate optional ECS capability rather than
as an `Arrangement` field or on a second controller entity.

A4.3 provides explicit construction without required-component defaults and
defines how incomplete controllers behave.

**Status:** Reopened. `Arrangement` identity and the absence of a required
built-in folding recipe are decided; the remaining validity contract is
pending A6 and A9.2.

### A4.3. Construction and incomplete-controller behavior

The `spawn_arrangement` helper and inert diagnostic behavior remain approved,
but the helper's previously proposed topology-provider and folding-policy
parameters are no longer final because A4.2 has reopened.

**Decision:** Do not attach default connection or folding policies as required
components of `Arrangement`. There is no shape-agnostic default connection
topology, and silently choosing `Strip`, `Accordion`, or `Coil` would hide author
intent without preventing later invalid component edits.

Provide `commands.spawn_arrangement(...)` through a `Commands` extension trait
as the guided construction path. Its final parameters are pending: it must
support a controller whose connections and hinge endpoints are authored
directly, as well as one using optional connection and endpoint recipes. A6
will supply the final topology-provider bound and representation; A9.2 will
supply the built-in endpoint-recipe representation.

```rust
let root_entity = commands.spawn_arrangement(/* parameters TBD */);
```

Direct ECS composition, scene loading, and later component editing remain
supported. While a selected authoring capability lacks the inputs it needs or
the resolved graph is invalid:

- preserve `Arrangement`, `Member`, and `Members`;
- perform no arrangement placement or hinge writes;
- preserve connections and hinges already emitted instead of deleting
  components whose ownership may be unclear;
- report the missing or conflicting responsibility without repeated log spam;
- never panic, invent a default, or choose one policy arbitrarily.

When the required data becomes valid again, request member reconciliation and
resume automatically. A9.2 decides whether folding recipes persist as ECS
state at all and therefore whether a folding-policy exclusivity problem
remains; F1 owns intentional suspension, and F12 owns the reconciliation
mechanism.

When `FoldSequence` is present, its `initial: FoldEndpoint` selects the initial
conformation as members become resolvable. The source of the initial hinge
endpoint for an arrangement with two distinct `Hinge` endpoints but no
`FoldSequence` remains part of the pending constructor and validity contract;
`Arrangement` identity alone must not choose one silently.

**Status:** Decided in principle; constructor signature pending A4.2 and A9.2.

### A5. Connection structure and folding behavior

**Decision:** Keep connection structure and folding behavior separate because
different connection topologies can work with different folding behaviors.

The topology provider (`name TBD`) decides whether
each `Member` has `AnchoredTo`, selects its `AnchoredTo::target()`, and owns the
paired tile edges and anchors plus the fixed connection `Angle` needed to align
a connection. The original POC represented folding behavior through `Strip`,
`Accordion`, and `Coil` ECS components. A9.2 has reopened that representation;
the durable A5 decision is only that topology construction and folding behavior
remain separate responsibilities.

```rust
let root_entity = commands.spawn_arrangement(
    triangle_connections,
    Accordion::default(),
);
```

The same `triangle_connections` component can be reused with different folding
behavior without redefining membership or physical connections. `FoldSequence`
remains optional orchestration; its ownership and membership model are decided
under A9.

**Status:** Decided.

### A6.0. Topology-provider classification

A6 is the prerequisite for A9.2.

**Decision:** Model this responsibility as a topology provider (`name TBD`),
not as a connection rule. A topology provider owns a complete logical pattern
and materializes the root-or-connected physical graph used by Hana. It may
expose structural capabilities for recipe-compatible fold directions. Each
direction can provide ordered, consistently oriented connection groups and
additional calibration required by compatible folding recipes, while topology
construction and folding behavior remain separate under A5.

A provider may know logical adjacencies that do not each become an
`AnchoredTo` relationship. The materialized `AnchoredTo` relationships remain
the acyclic, one-target-per-`Member` graph used for transform evaluation; the
provider is responsible for guaranteeing that its generated fold groups remain
coherent with the complete logical pattern.

Regular triangle-sheet, quad-sheet, and hex-sheet providers (exact public type
names TBD) should derive recipe-compatible directions for arbitrary finite
dimensions and support Accordion, Coil, and Wrap behavior. A strip is the
one-dimensional degenerate case in which each connection group contains one
connection. Wrap may require more provider-supplied geometry or calibration
than Accordion or Coil, but that requirement does not make regular sheets
inherently bespoke.

```rust
let sheet = TriangleSheet::new(8, 12); // provider name TBD
let groups = sheet.fold_groups(TriangleDirection::A)?; // names TBD

commands.apply_fold(
    arrangement,
    Wrap::new(groups), // recipe representation TBD under A9.2
)?;
```

A6.1-A6.6 still decide `AnchoredTo` target ownership, member references, the
root-or-connected result, trait methods and failures, discovery and scheduling,
and final terminology. A9.2 still decides the fold-group value and capability
types, compatibility checks, precise Accordion/Coil/Wrap behavior, wrap
calibration, and application lifecycle.

**Closure gate:** A6 closes only when its exact types and methods can materialize
a branching or multi-root connection graph without mentioning Accordion, Coil,
Wrap, or `FoldSequence`, and direct `AnchoredTo` / `Hinge` authoring remains
valid without a provider.

**Status:** Decided.

### A6.1. `AnchoredTo` presence and target ownership

**Decision:** For every `Member`, the topology provider (`name TBD`) decides
whether the physical member has no `AnchoredTo` or has one `AnchoredTo`, and it
selects the entity returned by `AnchoredTo::target()`. A `Member` without
`AnchoredTo` is a physical root. Hana validates and materializes the provider's
graph but never infers the preceding `Member` as a default target.

This ownership supports multiple physical roots, branching connection trees,
sheets, and polyhedral nets while keeping `Member` / `Members` order separate
from physical topology. Direct authoring may produce the same ordinary
components without using a topology provider. `AnchoredTo` remains acyclic;
F14 separately owns future closed-loop constraints.

```rust
commands.materialize_topology(arrangement, topology)?; // method name TBD

// Result selected by the topology provider:
// center: no AnchoredTo
// north:  AnchoredTo::target() == center
// south:  AnchoredTo::target() == center
// lid:    AnchoredTo::target() == north
```

A6.2 still decides how provider input and output identify these `Member`
entities. A6.3 still defines the named root-or-connected result, and A6.4
defines validation and failures.

**Status:** Decided.

### A7.1. Anchor orientation representation

**Decision:** Replace the optional quaternion representation with a named,
non-optional value: `AnchorPoint::orientation: AnchorOrientation`.

`AnchorOrientation` contains the `Quat` that rotates the entity's local axes
into the anchor's local axes. Its identity/default value means that the anchor
uses the entity's own local orientation. `AnchorPoint` therefore always has an
orientation; the API does not encode entity-local orientation as `None`.

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct AnchorOrientation(Quat);

pub struct AnchorPoint {
    pub position: Vec3,
    pub orientation: AnchorOrientation,
}

let anchor = AnchorPoint {
    position: Vec3::ZERO,
    orientation: AnchorOrientation::default(),
};
```

`anchor_placement()` reads the source and target `AnchorPoint::orientation`
values when `resolve_anchors()` resolves an `AnchoredTo` relationship.
`Edge::axis()` reads both endpoint orientations when checking whether an
edge's local axes are compatible, and `ResolvedAnchorGeometry::validate()`
performs the same compatibility check before resolver systems consume provider
geometry. The constructor and validation contract for `AnchorOrientation`
remain undecided under A7.

This decision removes the ambiguous distinction between a missing quaternion
and an explicit identity quaternion. A7.2 separately decides whether
`AnchorPoint` itself should be renamed; no type-name decision is implied here.

**Status:** Decided.

### A8. Placement wrappers and angle representation

**Decision:** Remove `ArrangementPlacement` and `MemberPlacement`. The named A6
root-or-connected result (`name TBD`) retains one connected payload
(`name TBD`) containing `AnchoredTo`, the member-local `Edge`, and the fixed
connection `Angle`. Reconciliation combines that fixed angle with the built-in
recipe's endpoint offsets and stores only the resulting two `Angle` endpoints
on `Hinge`.

Add `Angle` to `bevy_kana`'s default math API as a shared, plain value stored in
radians. It represents a signed, unwrapped angular displacement; it is not an
ECS component, an orientation, a normalized rotation, or dimensionless
progress.

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Reflect)]
pub struct Angle(f32);

impl Angle {
    pub const ZERO: Self = Self(0.0);

    pub const fn from_radians(radians: f32) -> Self;
    pub fn from_degrees(degrees: f32) -> Self;

    pub const fn as_radians(self) -> f32;
    pub fn as_degrees(self) -> f32;
    pub const fn is_finite(self) -> bool;
}
```

Provide angle addition and subtraction, negation, and multiplication by `f32`.
Do not provide `From<f32>`, `Into<f32>`, `Deref`, a public tuple field, or
automatic wrapping. Conversion occurs explicitly at APIs that require radians:

```rust
let rotation = Quat::from_axis_angle(axis, angle.as_radians());
```

Finite multi-turn values are intentional. For example,
`Angle::from_degrees(720.0)` represents two complete turns; scalar interpolation
preserves both turns even though its final quaternion orientation equals the
initial orientation. Do not impose a magnitude range or normalize finite
angles. A10 owns rejection of non-finite resolved hinge calibration, while F13
owns how that failure is reported.

Use this shared type for Hana Valence's fixed connection angles, recipe angle
offsets, and `Hinge` endpoints. The original A8 decision also selected `Angle`
for `HingePivot::reference_angle`; A9.4 subsequently removes that field as
redundant with the unfolded `Hinge` endpoint and then merges the remaining
pivot offset into `Hinge`. This preserves the decision history while
superseding the field conversion and separate `HingePivot` type. Keep fold
progress and easing fractions as `f32`; keep spatial orientation as
`AnchorOrientation`, `Quat`, or `Rot2`. After Hana Valence adopts `Angle`, audit
the rest of the workspace for scalar angular-displacement fields. Do not
blanket-convert orientations, progress values, or angular rates.

**Status:** Decided.

### A9.1. Hinge endpoint ownership and derived angle

**Current implementation:** `Hinge` stores `edge` plus one mutable current
`angle`. `FoldAngles` separately stores `unfolded` and `folded` endpoints.
`drive_arrangement_hinges()`, `actuate_fold_hinges()`, and `HingeAngleLens`
can all change the physical angle observed by `hinge_to_pose()`.

**Decision:** Make `Hinge` the one angle-bearing component. It stores its
`Edge` and its unfolded and folded endpoint `Angle` values. `hinge_to_pose()`
derives the current angle from those endpoints and the selected fold progress,
then writes `AnchorPose`; the current angle is not stored as component state.

```rust
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
}

let fraction = sequence_state.stage_progress(*fold_stage); // method name TBD
let angle = hinge.unfolded_angle
    + (hinge.folded_angle - hinge.unfolded_angle) * fraction;
```

Remove `FoldAngles`, `Hinge::angle`, `drive_arrangement_hinges()`, and
`actuate_fold_hinges()`. `HingeAngleLens` cannot remain an angle writer once
the current angle is derived; A9.3 decides how external progress enters the
sequence model before its replacement is specified. Keep `AnchorPose`,
`FoldSequence`, `FoldStage`, and `FoldSequenceState`; they have separate
responsibilities pending their collective review under A9.3. A9.4 subsequently
merges the demonstrated `HingePivot` data into `Hinge::pivot_offset`, while A9.5
removes `FoldMember` / `FoldMembers` and puts `FoldStage` directly on
participating `Member` entities.

The endpoint field names remain undecided. A9.2 decides how compatible,
potentially transient folding recipes materialize those endpoint values without
presuming that the POC `Strip`, `Accordion`, and `Coil` components survive.
A9.4 audits the remaining fold types and reports the final net concept count.

**Status:** Decided.

### A9.2. Fold-recipe authoring and topology capabilities

A9.2 no longer assumes that the POC `Strip`, `Accordion`, and `Coil` components
survive. It is parked until A6 defines the topology-provider boundary, then it
will resolve these decisions in order:

1. **A9.2.1 — Durable result:** Decide whether a folding recipe is transient
   authoring input whose durable ECS result is complete `Hinge` data.
2. **A9.2.2 — Fold-group value:** Define the authoring-only value (`name TBD`)
   containing ordered, consistently oriented groups of connections that fold
   together. These spatial groups are not `FoldStage` timing groups.
3. **A9.2.3 — Provider capability:** Define the optional topology-provider
   capability (`name TBD`) that returns fold groups and what provider data must
   remain available for runtime re-authoring.
4. **A9.2.4 — Compatibility:** Define nonempty, membership, connectivity,
   orientation, uniqueness, finite-calibration, overlap, and failure
   invariants.
5. **A9.2.5 — Accordion:** Define alternating physical fold sense without
   reducing it to raw local-angle sign parity.
6. **A9.2.6 — Coil and Wrap:** Separate cumulative turning from physically
   exterior wrapping and define required clearance or pivot calibration.
7. **A9.2.7 — Application lifecycle:** Define initial application, runtime
   replacement, neutral-state travel, and all-or-nothing commit behavior with
   F9 and A10.

**Closure gate:** A9.2 closes only after final, non-TBD API examples cover a
triangle strip, quad grid, hex grid, direct-authored box or polyhedral net that
implements no fold-group capability, and one incompatible request that writes
nothing. The final type inventory must distinguish persistent ECS state from
authoring-only values and report the net public-type cost.

**Status:** Pending A6.

### A9.3. Fold transport, stage timing, and easing

**Existing types:** `FoldSequence` contains authored step duration, easing, and
initial endpoint. `FoldSequenceState` contains validated runtime transport
state and currently copies the authored easing. `FoldStage` is the zero-based
stage value used to calculate simultaneous member progress. `FoldCommand` is
carried by the entity-targeted `FoldCommandEvent` and changes
`FoldSequenceState`.

**Decisions within this active item:** Keep `FoldStage` as a named newtype.
Keep one raw controller-level position in `FoldSequenceState`. Apply easing
only while `hinge_to_pose()` derives a member's angle, using this precedence:

```text
optional easing component on this Member (name TBD)
    > override for this FoldStage stored once in FoldSequence
    > FoldSequence default easing
```

The per-member override changes the curve but not the shared stage clock, so
members with equal `FoldStage` values still start and finish together. Remove
the copied easing field from `FoldSequenceState`; its stage-progress accessor
(`name TBD`) returns raw progress in `0.0..=1.0`.

```rust
let progress = state.stage_progress(stage); // name TBD
let easing = member_easing
    .map(|easing| easing.0)
    .unwrap_or_else(|| sequence.easing_for(stage)); // name TBD
let fraction = easing.sample_clamped(progress);
```

**Stage timing decision:** Use one plain value (`name TBD`) containing
`Duration` and `EaseFunction` for the `FoldSequence` default and for a
`FoldStage` override. `FoldStage` owns that clock. A fold needing distinct
duration occupies a one-member `FoldStage`; a `Member` inside a shared stage
may override only easing. The builder must make the distinction structural—for
example, by accepting timing when adding a stage—rather than accepting an
invalid per-member duration. Exact type, field, and builder method names remain
pending. Advancement must consume elapsed time stage by stage, including
crossing multiple differently timed stages in one frame; it can no longer
divide the entire frame delta by one sequence-wide `step_seconds` value.

**Command decision:** Keep all playback requests in the existing
entity-targeted `FoldCommandEvent`, carrying this command surface:

```rust
pub enum FoldCommand {
    Step(FoldDirection),
    PlayTo(FoldEndpoint),
    Pause,
    Resume,
    Reverse,
}
```

`Step` targets an adjacent stage boundary. `PlayTo` targets the named terminal
endpoint and is idempotent when repeated. `Pause` retains raw position, target,
direction, and whether the journey is a step or continuous play; `Resume`
continues that exact journey. `Reverse` preserves raw position and journey
kind while selecting the opposite adjacent boundary or terminal endpoint. It
leaves a paused journey paused and is a no-op while idle. Keep `FoldMotion` as
`Idle`, `Step`, or `Play`; represent pausing with one private orthogonal flag
and expose it through `FoldSequenceState::is_paused()`. Remove the current
context-sensitive directionless `FoldCommand::Play`.

**Component-boundary decision:** Keep `FoldSequence` as a separate optional
component on the same controller entity as `Arrangement`. Do not put
`Option<FoldSequence>` inside `Arrangement`: nesting removes no concept, makes
playback systems inspect an option, broadens `Changed<Arrangement>`, and loses
the direct insert/remove/change lifecycle for `FoldSequence`. Component
presence denotes staged-playback capability; `FoldSequenceState` exists only
while that capability is present. `spawn_arrangement(...)` supplies the guided
co-location path, while the A4 validity contract decides how to diagnose a
stray `FoldSequence`.

**Still pending within A9.3:** Decide the representation of a future external
raw-progress adapter, whether overshooting easing output may exceed the
calibrated `Hinge` endpoints, and exact timing and easing names. A9.4 owns the
remaining cleanup audit; it has already removed `FoldFromArrangement`.

**Canonical transport decision:** Keep `FoldSequenceState` and its
fold-specific target, direction, stage-boundary, step/play, readiness, and
revision semantics rather than replacing them with
`bevy_time_runner::TimeRunner`. `FoldCommandEvent` is the common initiation and
control path. A `bevy_tween` timeline can trigger a command without an adapter;
continuously driving progress remains a future adapter concern. Such an
adapter must select external ownership even while paused and must supply raw
progress if Hana retains the easing cascade.

**Status:** Partial; stage timing, easing ownership and precedence, component
boundaries, commands, and canonical transport decided. Exact names,
overshooting easing, and the future external-driver representation remain
pending.

### A9.4. Fold-pipeline simplification audit

**Decision within this active item:** Remove `FoldFromArrangement`, its
retained-request marker, snapshot system, snapshot diagnostic types, and the
validation exception that waits for the request. Its existing job is to copy
all arrangement members into the duplicate `FoldMember` relationship with one
member per consecutive `FoldStage`; A9.5 removes that relationship, and
optional `FoldStage` means not every `Member` participates in playback.

Keep `FoldSequenceBuilder`, retargeted to the `Arrangement` controller, and
have it insert `FoldStage` directly on the explicitly selected `Member`
entities. The triangle example collects its moving members and authors one
single-member stage per member. A newly added `Member` remains fixed until an
author explicitly assigns `FoldStage`; member insertion must not silently
renumber existing stages.

```rust
let mut builder = FoldSequenceBuilder::new(&mut commands, controller);
for member in folding_members {
    builder = builder.stage(member);
}
builder.finish()?;
```

A serial-stage convenience (`name TBD`) may be considered under F15 after the
primary model is settled; it is not another ECS request component.

**Decision within this active item:** Remove `MemberPlacement`. It is an
existing plain value that wraps `ArrangementPlacement` and adds only the live
hinge `angle`. A9.1 removes that stored live angle and derives it in
`hinge_to_pose()`, so the wrapper has no remaining data or responsibility.

**Decision within this active item:** Remove the `ArrangementPlacement` name
and standalone public value. Its `AnchoredTo`, member-local `Edge`, and fixed
connection `Angle` remain together as the connected payload of the A6 result
(`name TBD`) because changing any one invalidates the others. Reconciliation
combines the fixed connection `Angle` with two endpoint-recipe `Angle` offsets
and stores only the resulting endpoint `Angle` values on `Hinge`; it does not
store a third base angle. A6.3 uses a named root-or-connected sum type rather
than `Option`, while its exact type, variant, and connected-payload names remain
pending.

**Decision within this active item:** Remove the public
`FoldSystems::Actuate` system-set variant. Its only system is
`actuate_fold_hinges()`, which A9.1 removes together with `FoldAngles` and
mutable `Hinge::angle`. The redesigned `hinge_to_pose()` directly reads the
member's `FoldStage`, the controller's `FoldSequenceState`, and the member's
endpoint-bearing `Hinge`, then writes `AnchorPose`; no separate actuation phase
remains.

Retain `FoldSystems::Advance` as the public boundary after controller transport
advancement. Existing consumers use it to read the newly advanced
`FoldSequenceState` or update data before `hinge_to_pose()`, and the future
single-writer raw-progress adapter will need the same ordering boundary. Order
`hinge_to_pose()` after `FoldSystems::Advance` without introducing a replacement
actuation set.

```rust
#[derive(SystemSet, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum FoldSystems {
    Advance,
}

app.configure_sets(
    PostUpdate,
    FoldSystems::Advance
        .in_set(AnchorSystems::AnimatePose)
        .before(hana_valence::hinge_to_pose),
);
```

**Decision within this active item:** Remove `FoldAngleInvalidReason`,
`FoldAngleDiagnostic`, and `FoldAngleDiagnostics`, including the resource
initialization in `FoldPlugin` and the now-empty `fold::hinge` adapter module.
Their only producer is the removed `actuate_fold_hinges()`, and no runtime
system reads the retained history to affect behavior. Do not merge these
member-local hinge failures into controller-level `FoldDiagnostics`.

`hinge_to_pose()` continues the existing safe-skip behavior: it checks the
derived `Angle` before converting it to radians or writing `AnchorPose`, emits a
trace warning for a non-finite result, and leaves the existing pose unchanged.
F13 must later review one coherent anchor-and-hinge diagnostic API, including
whether failures need retained history, warning deduplication or throttling,
and structured reasons for non-finite `Angle` endpoints and interpolation.

```rust
let angle = hinge.unfolded_angle
    + (hinge.folded_angle - hinge.unfolded_angle) * fraction;

if !angle.is_finite() {
    tracing::warn!(entity = ?entity, "hinge angle is non-finite");
    continue;
}

let rotation = Quat::from_axis_angle(*axis, angle.as_radians());
```

**Decision within this active item:** Remove the public `Hinge::rotation()`
method without replacement. The current method has one unambiguous result only
because the implemented `Hinge` stores one mutable current angle. Under A9.1,
`Hinge` instead stores two endpoint `Angle` values, so selecting a current
rotation requires the member's `FoldStage`, the controller's
`FoldSequenceState`, and resolved easing.

Do not add `rotation_at()` or `rotation_at_progress()`: either would recreate a
second fold-angle evaluation path beside `hinge_to_pose()`. Authors supply
calibration and playback intent, while `hinge_to_pose()` remains the one system
that selects the current angle and writes `AnchorPose`. `Edge::axis()` remains
available for non-fold geometry use.

```rust
commands.entity(lid).insert((
    Member::of(box_arrangement),
    FoldStage(0),
    Hinge {
        edge: lid_edge,
        unfolded_angle: Angle::ZERO,             // field name TBD
        folded_angle: Angle::from_degrees(90.0), // field name TBD
    },
));

commands.trigger(FoldCommandEvent::new(
    box_arrangement,
    FoldCommand::PlayTo(FoldEndpoint::Folded),
));
```

**Decision within this active item:** Keep `HingePivot` as the optional
component for an external pivot-line offset, but remove
`HingePivot::reference_angle`. Define `HingePivot::offset` in the conformation
represented by the unfolded `Hinge` endpoint (`field name TBD`).
`hinge_to_pose()` calculates pivot translation from the difference between the
derived current angle and that endpoint.

Every shipped example that combines `HingePivot` with fold endpoints already
uses the unfolded endpoint as its reference angle. The new invariant removes
one duplicated `Angle` field and prevents the pivot reference from disagreeing
with the unfolded conformation. It intentionally removes undemonstrated support
for declaring zero pivot translation at an arbitrary non-endpoint angle.

```rust
pub struct HingePivot {
    pub offset: Vec3,
}

let pivot_delta =
    current_angle - hinge.unfolded_angle; // endpoint field name TBD
let pivot_rotation =
    Quat::from_axis_angle(*axis, pivot_delta.as_radians());
let translation = pivot.offset - pivot_rotation * pivot.offset;
```

This intermediate decision to retain a separate, offset-only `HingePivot` is
superseded by the later A9.4 merge below. Its removal of `reference_angle` and
unfolded-endpoint calibration remain part of the merged representation.

**Decision within this active item:** Declare `AnchorPose` as a Bevy required
component of `Hinge`. After removal of public `Hinge::rotation()`,
`hinge_to_pose()` is the supported interpretation path and requires mutable
`AnchorPose`; a missing pose would otherwise silently exclude the entity from
the system. Every surviving production path and direct-authoring example
already inserts `AnchorPose::default()` beside `Hinge`.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[require(AnchorPose)]
#[reflect(Component)]
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
}
```

Bevy inserts `AnchorPose::default()` only when no explicit `AnchorPose` is
supplied. Ordinary `remove::<Hinge>()` leaves `AnchorPose` in place so another
animation system can take ownership; a caller deliberately wanting both gone
can use Bevy's required-component removal operation. This insertion guarantee
does not prevent a caller from explicitly removing `AnchorPose` later, so F13
and A10 still own diagnostics and broader validity policy.

Do not require `AnchoredTo`, `ResolvedAnchorGeometry`, `Member`, or `FoldStage`:
physical connection roots, directly authored or fixed hinges, and geometry that
arrives later remain valid.

**Decision within this active item:** Merge the remaining `HingePivot::offset`
into `Hinge::pivot_offset` and remove the `HingePivot` component. `Vec3::ZERO`
is the exact identity for no pivot compensation; a small nonzero value remains
meaningful geometry and must not be collapsed through an epsilon or `Option`.
`hinge_to_pose()` is the only runtime reader of both values, and every
meaningful authoring site supplies them together. The merge makes endpoint and
pivot calibration one atomic `Hinge` update.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[require(AnchorPose)]
#[reflect(Component)]
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
    pub pivot_offset: Vec3,
}

let thin_hinge = Hinge {
    edge,
    unfolded_angle: Angle::ZERO,             // field name TBD
    folded_angle: Angle::from_degrees(90.0), // field name TBD
    pivot_offset: Vec3::ZERO,
};
```

For every `Hinge` carrying `AnchoredTo`, `hinge_to_pose()` converts the edge
axis through the `AnchoredTo::source_anchor` tangent frame; `pivot_offset`
controls translation only. A physical root without `AnchoredTo` may use
`Vec3::ZERO`; a nonzero offset without a source anchor frame is invalid and is
handled by A10 and F13.

**Decision within this active item:** Remove `AnchorPoseLens`, the resulting
empty `tween` module and crate feature, and Hana Valence's optional
`bevy_tween` dependency. No production path, example, or test registers or
constructs the lens. It cannot safely share an entity with `Hinge` because
`hinge_to_pose()` writes the complete `AnchorPose`, and it is not a fold
transport adapter because direct pose writes bypass `FoldSequenceState`,
`FoldStage`, calibrated endpoints, easing, and pivot compensation.

Keep `AnchorPose` and direct non-hinge pose animation. The panel fan and morph
demonstrate purpose-built systems writing `AnchorPose`, while F2, F3, and F11
require capture, coordinated relationship commits, or continuous multi-turn
composition that the generic lens does not provide. A9.3 retains the future
single-writer raw-progress adapter as the correct continuous fold integration.

**Decision within this active item:** Keep `FoldStage(usize)` as the ECS
component stored on each participating `Member`. Add `SharedFoldStage` as one
plain, authoring-only value containing a nonempty collection of `Entity` values
that will all receive the same `FoldStage`. It is not a component or
relationship.

`FoldSequenceBuilder::stage()` accepts `impl Into<SharedFoldStage>`. Implement
`From<Entity>` for a one-member stage and `From<(Entity, I)>` for a required
first member plus any `IntoIterator<Item = Entity>` remainder; Rust supplies
the corresponding `Into` implementations automatically.

```rust
pub struct SharedFoldStage(Vec<Entity>);

impl From<Entity> for SharedFoldStage {
    fn from(member: Entity) -> Self {
        Self(vec![member])
    }
}

impl<I> From<(Entity, I)> for SharedFoldStage
where
    I: IntoIterator<Item = Entity>,
{
    fn from((first, remaining): (Entity, I)) -> Self {
        let mut members = vec![first];
        members.extend(remaining);
        Self(members)
    }
}

pub fn stage(mut self, stage: impl Into<SharedFoldStage>) -> Self {
    self.stages.push(stage.into());
    self
}
```

Runtime collections use `SharedFoldStage::try_from_iter()` or
`TryFrom<Vec<Entity>>`; an empty collection returns
`FoldAuthorError::EmptyStage` before a stage can enter the builder. Do not
implement infallible `From<Vec<Entity>>` or `FromIterator<Entity>`, because both
can receive zero entities.

```rust
FoldSequenceBuilder::new(&mut commands, arrangement)
    .stage(lid)
    .stage((north, [south, east, west]))
    .finish()?;
```

Keep `FoldAuthorError::DuplicateMember(Entity)`: entity equality is runtime
data and cannot be proven by typestate. `finish()` validates all stages before
queuing any command, so a duplicate never creates partial ECS state. A
zero-stage builder remains valid so an arrangement may receive stages later.
No typestate markers or secondary stage builder are added. A9.3 may later add
stage-level timing to `SharedFoldStage` without changing its nonempty invariant.

The retained public concepts now each have one responsibility: `FoldPlugin`
installs validation, command handling, and playback; `FoldSystems::Advance`
marks the controller-transport scheduling boundary; `FoldSequenceBuilder` and
`FoldAuthorError` provide transactional stage authoring; and
`SharedFoldStage` names a nonempty authoring group without becoming ECS state.

The final reduction removes eleven public types and adds one authoring-only
public type, for a net reduction of ten public types. It additionally removes
one public system-set variant, one public method, and one optional feature,
module, and dependency. It adds no runtime ECS component or relationship.
Direct purpose-built `AnchorPose` writers remain supported, and F13 owns the
replacement diagnostic policy.

**Status:** Decided.

### A9.5. Sequence participation and arrangement membership

**Decision:** Remove the `FoldMember` / `FoldMembers` relationship. Every
physical entity carries the one `Member` relationship targeting its
`Arrangement` controller. A folding member additionally carries `FoldStage`;
a fixed member does not.

```rust
commands.entity(fixed_face).insert(Member::of(controller));

commands.entity(wall).insert((
    Member::of(controller),
    FoldStage(0),
));
```

The optional `FoldSequence` is colocated with `Arrangement` on that controller.
Sequence validation iterates the Bevy-maintained `Members` collection and
selects entities carrying `FoldStage`. This removes a duplicate membership
graph while retaining independent physical topology through `AnchoredTo` and
independent animation order through `FoldStage`.

This model supports one active `FoldSequence` per `Arrangement`. The existing
`FoldMember` already permits only one sequence assignment per entity, so the
decision removes no demonstrated multi-sequence behavior. A new assignment
model should be introduced only if a future feature requires overlapping
independent sequences.

`FoldFromArrangement` depended on the duplicate relationship and is removed
under A9.4. `FoldSequenceBuilder` remains the explicit authoring helper.

**Status:** Decided.

The boundary used here is:

- `hana_valence` owns relationships, ordered arrangements, pose and hinge
  motion, transitions between authored relationships, and related diagnostics;
- adapters such as `bevy_diegetic` provide geometry and coordinate-space
  conversion;
- applications own controls, labels, colors, camera presentation, and teaching
  UI.

## Existing foundation

Several demonstrated capabilities are already present and should be reused:

- `AnchoredTo` and `AnchoredHere` maintain source-to-target relationships.
- `ResolvedAnchorOffset` supplies a live offset without replacing the authored
  relationship.
- `AnchorPose` and `Hinge` separate animation from resolved transforms.
- `AnchorPoseLens` and `HingeAngleLens` currently provide tween adapters; A9.4
  and A9.1 remove both unused or conflicting adapters.
- `ArrangementMembers` and `MemberIndex` maintain insertion order as members
  are added and removed.
- `Strip`, `Accordion`, `Coil`, and `TilingRule` describe linear arrangements.
- `FoldSequence` provides staged fold playback with step and continuous play
  commands.

The candidates below concern behavior still implemented by the example or
behavior whose existing API is not sufficient for the example.

## Candidate features

### F1. Arrangement activation and suspension

**Demonstrated behavior:** One persistent entity set alternates between a
freely authored anchor fan and an arrangement-driven hinge chain. Arrangement
placement and hinge writing must stop while the fan or a transition owns the
same entities, then resume without losing member order.

**Candidate:** Add an explicit active/suspended state for an arrangement. A
suspended arrangement would retain its root, members, and relationship order while
its placement and hinge systems perform no writes. Reactivation would mark
members for placement again and resume angle driving.

**Why it belongs:** Writer ownership and member lifecycle are properties of the
arrangement runtime, not of panel UI.

**Review questions:**

- Should suspension retain `Hinge` and `AnchoredTo`, or remove runtime-owned
  outputs and reconstruct them on activation?
- Should activation be a component, an entity-targeted event, or both?
- How should activation interact with `Hinge` endpoint materialization and
  optional `FoldSequence` playback?

### F2. Smooth relationship replacement

**Demonstrated behavior:** Changing either the source or target anchor moves an
attached entity from its current resolved position to the newly authored
relationship without a jump. The example replaces the relationship, computes
the old-to-new displacement in the target frame, and eases an `AnchorPose`
translation back to zero.

**Candidate:** Provide a relationship-transition request that replaces an
`AnchoredTo` relationship while preserving the current resolved pose, then
eases to the new relationship. It should support source-anchor changes,
target-anchor changes, target-entity changes, and offset changes.

**Why it belongs:** The operation depends on Valence geometry, relationship
hooks, pose ownership, and resolver scheduling.

**Review questions:**

- Should transition requests capture the old resolved world pose immediately
  or at the next resolver pass?
- Which changes can be represented by translation alone, and which require
  rotation interpolation?
- What diagnostics should be recorded when either relationship cannot resolve?

### F3. Multi-entity relationship morphs

**Demonstrated behavior:** The same entities move between two complete layouts
without respawning. The example captures every live pose, keeps the outgoing
relationships during the ease, computes poses that reproduce the incoming
layout, and replaces the relationships only after the visual transition has
landed.

**Candidate:** Add a coordinated transition for a set of relationship changes.
The transition should capture all starting poses, prevent competing writers,
advance one shared progress value, and commit the replacement relationships as
one operation.

**Why it belongs:** This is the set-level form of smooth relationship
replacement and requires dependency-ordered resolution.

**Review questions:**

- Is this a general attachment transaction or an arrangement-to-arrangement
  transition?
- Must the old and new relationship graphs contain the same entities?
- How should cycles or partially invalid destination graphs be rejected before
  motion begins?

### F4. Arrangement-type replacement

**Demonstrated behavior:** A hinge chain changes between accordion and coil
behavior. A selection made while folded does not alter the live pose; the
change is committed only when the next fold begins through the flat state.

**Candidate:** Provide an entity-targeted operation that replaces the current
arrangement component and guarantees that `Strip`, `Accordion`, and `Coil` are
mutually exclusive. Replacement should preserve members and relationship order and mark
the arrangement for recalculation.

**Why it belongs:** Arrangement identity and the exactly-one invariant are core
arrangement concerns.

The short-term observer design is investigated separately in
[`mutex_arrangements.md`](mutex_arrangements.md).

### F5. Scalar arrangement-fold transport

**Demonstrated behavior:** Every hinge in an accordion or coil shares one fold
progress value. A full transition has constant duration regardless of member
count, unlike a staged sequence where members advance at successive stage
boundaries.

**Candidate:** Add a small transport for driving the `fold` field on one
arrangement component. It would own progress, endpoint, timing, easing,
direction, and motion state while the existing arrangement system converts the
result into member hinge angles.

**Why it belongs:** It is reusable motion over the public arrangement
components and is distinct from `FoldSequence`'s staged member playback.

**Review questions:**

- Should this extend `FoldSequence`, share its playback internals, or be a
  separate `ArrangementFold` component?
- Should duration describe the whole arrangement transition or radians per
  second?
- Should it support both `Accordion` and `Coil` through one command API?

### F6. Explicit terminal commands

**Demonstrated behavior:** Fold and reset commands target a known terminal
state: fully folded or fully spread out. The command does not depend on which
endpoint is currently active.

**Decision from A9.3:** Use `PlayTo(FoldEndpoint)` for explicit terminal
travel. It selects continuous playback, and requesting the same endpoint again
is an idempotent no-op. Keep `Step(FoldDirection)` for adjacent-boundary
travel. Do not add separate `Fold` and `Unfold` command variants.

**Status:** Decided.

### F7. Pause and resume

**Demonstrated behavior:** Both spin and hinge-fold motion can freeze at the
current continuous position and resume without resetting timers, endpoints, or
direction.

**Decision from A9.3:** Add `FoldCommand::Pause` and `FoldCommand::Resume`.
Pausing retains the raw position, target, direction, and `FoldMotion`; resuming
continues that exact journey. Expose the state through
`FoldSequenceState::is_paused()`. Virtual-time pause is not sufficient when one
animation must pause while unrelated application motion continues.

**Why it belongs:** The current folding as-built explicitly has no pause state,
so applications must currently duplicate this transport concern.

**Status:** Decided.

### F8. Step versus continuous travel

**Demonstrated behavior:** `Step` advances one rest state and stops. `Glide`
continues through the intermediate flat state to the requested destination.

**Decision from A9.3:** Choose transport policy per command:
`Step(FoldDirection)` selects adjacent-boundary travel and
`PlayTo(FoldEndpoint)` selects terminal travel. Retain `FoldMotion::Step` and
`FoldMotion::Play` as runtime state instead of maintaining an application
state machine.

**Remaining question:** Is the example's two-state scalar fold a one-stage
`FoldSequence` or does it need the separate transport considered by F5?

**Status:** Partial; command policy decided, scalar transport reuse pending F5.

### F9. Safe reconfiguration through a neutral state

**Demonstrated behavior:** Changing accordion/coil selection or fold direction
while the assembly is folded first unfolds it, commits the pending
configuration only when it is flat, and then refolds. This avoids discontinuous
geometry.

**Candidate:** Add a queued reconfiguration transaction with three steps:
travel to a required neutral endpoint, replace selected arrangement data, then
optionally return to the requested terminal endpoint.

**Why it belongs:** The safe commit point depends on arrangement geometry and
transport state. Applications should issue an intent without reproducing the
transition rules.

### F10. Live resolved-offset animation

**Demonstrated behavior:** The depth between linked panels changes continuously
along the target anchor frame while the authored relationship stays intact.
Every member in the chain reflects the per-link offset.

**Candidate:** Add a tween lens or small driver for `ResolvedAnchorOffset`, plus
an optional helper for applying one per-link offset across an ordered
arrangement.

**Why it belongs:** `ResolvedAnchorOffset` already establishes the runtime
override; only reusable animation and arrangement propagation are missing.

### F11. Anchored pose spin envelope

**Demonstrated behavior:** An anchored assembly accelerates into rotation about
the anchor plane normal, can pause, decelerates symmetrically, and settles on
the nearest full turn so the resting orientation is stable.

**Candidate:** Provide an optional pose-motion adapter for continuous rotation
with rise, hold, fall, pause, and terminal-angle snapping. Axis selection should
come from resolved anchor or edge geometry rather than panel coordinates.

**Review questions:**

- Is continuous spin common enough for core `hana_valence`, or should it live in
  a separate animation adapter crate?
- Should the adapter write `AnchorPose` directly or produce a scalar angle for
  another system to compose?

### F12. Dynamic member reconciliation

**Demonstrated behavior:** Members can be appended and removed while any
capability is active. Existing entities keep their relative order, and new
members join the current arrangement and animation state.

**Candidate:** Derive positions from the current `Members` order, then add an
explicit reactivation/replacement path that marks all retained members pending
when an arrangement's topology or ownership changes. Document how insertion,
removal, and retargeting affect relationship order.

**Current support:** Addition, removal, insertion order, and stored stable
indices are implemented today. A3 replaces those indices with derived
positions. This review concerns reactivation and topology changes, not basic
member storage.

### F13. Anchor and relationship diagnostics

**Demonstrated behavior:** The example can display active source and target
anchor locations and draw the world-space link between separated anchors.

**Candidate:** Add an optional diagnostic/gizmo plugin that can render resolved
anchor points, local frames, ordered edges, relationship links, hinge axes, and
skipped relationships. This should consume `ResolvedAnchorGeometry`,
`ResolvedAnchorWorld`, and existing diagnostics without depending on panel
layout types.

**Why it belongs:** The information is useful to every geometry provider and
would make authored relationship failures easier to inspect.

**Required review from A9.4:** The removal of the adapter-specific
`FoldAngleInvalidReason`, `FoldAngleDiagnostic`, and `FoldAngleDiagnostics`
family must not end the diagnostic discussion. Decide one coherent policy for
anchor and hinge failures, including non-finite `Angle` endpoints or derived
angles, retained versus transient diagnostics, structured failure reasons, and
warning deduplication or throttling. Keep controller-level `FoldDiagnostics`
separate unless this review demonstrates a useful common representation.

### F14. Closed-loop connection constraints

**Motivating behavior:** A kaleidocycle is a moving closed ring. Cutting one
connection produces an acyclic tree that Hana can resolve, but animating the
closed mechanism requires the omitted seam to remain satisfied throughout the
motion.

**Candidate:** Add an explicit closure constraint that is separate from
`AnchoredTo`. Keep transform resolution acyclic while a specialized kinematic
rule or solver computes compatible hinge values and reports an unsatisfied
closing seam.

**Review questions:**

- Is a declarative closing-seam constraint sufficient, or is a general
  kinematic constraint solver required?
- Does the first implementation target one analytically solvable loop, such as
  a kaleidocycle, or arbitrary loops?
- How are impossible configurations and accumulated numerical error reported?

### F15. Pattern-oriented arrangement builders

**Motivation:** After the primary arrangement, connection, hinge, and sequence
model is settled, common arrangements should not require repetitive manual
setup. Authors should be able to intelligently apply or "paste" reusable
patterns over an explicit member selection.

**Candidate:** Review builder and arrangement-authoring features for materializing
common patterns such as serial `FoldStage` assignment, simultaneous stage
groups, fixed-member exclusions, repeated connection layouts, and reusable
endpoint recipes. These helpers must produce the same ordinary `Member`,
`AnchoredTo`, `Hinge`, and `FoldStage` data as direct authoring rather than
create a parallel runtime model.

**Scheduling:** Review this only after the primary A1-A10 decisions are
complete, so the convenience API is built over the final concepts instead of
preserving obsolete setup mechanisms such as `FoldFromArrangement`.

**Review questions:**

- Which repeated setup patterns are demonstrated strongly enough to deserve a
  named builder operation?
- Should a pattern copy concrete authored data, apply a reusable recipe, or
  support both through separate operations?
- How does a helper report partial matches, incompatible member geometry, or
  invalid topology without silently guessing?
- Which generated values remain individually editable after the pattern is
  applied?

## Keep outside `hana_valence`

These example features do not belong in the core crate:

- keyboard mappings, key-repeat timing, and highlighted control chips;
- capability menus and explanatory screen panels;
- panel colors, borders, labels, and anchor-marker layout elements;
- the panel-specific 3-by-3 anchor navigation model and human-readable anchor
  names;
- camera orbit presets, viewport-overflow detection, and automatic camera
  fitting;
- panel rebuilding thresholds used to limit presentation updates;
- `bevy_diegetic` unit conversion and panel geometry publication.

The general operations behind some of these features can still be Hana APIs.
For example, the 3-by-3 keyboard navigator stays in the example, while smooth
replacement of one `AnchorId` with another is a Valence concern.

## Review order

1. Decide arrangement activation/suspension semantics; the migrated example
   exposes this boundary immediately.
2. Review the mutually exclusive arrangement design in
   [`mutex_arrangements.md`](mutex_arrangements.md).
3. Decide whether smooth relationship replacement and coordinated set morphs
   are one API or two layers.
4. Decide whether scalar arrangement folding reuses `FoldSequence` playback
   internals or receives its own public transport.
5. Review pause/resume, explicit endpoint commands, travel policy, and neutral
   reconfiguration together because they affect one state model.
6. Decide whether offset animation and spin are core adapters or deferred
   extensions.
7. Decide whether diagnostic gizmos belong behind a `hana_valence` feature or
   in a companion integration crate.
8. Decide whether closed-loop constraints are in scope for the first general
   arrangement plan or explicitly deferred.
9. After the primary A1-A10 decisions, review pattern-oriented builders that
   intelligently apply common setup patterns without introducing parallel
   runtime concepts.

## Consequences from the arrangement migration

The migration removed the example's manual hinge-chain relationship authoring,
per-member accordion/coil angle calculation, and direct hinge-pose writer.
Tile zero now carries `QuadTiling` plus the committed `Accordion` or `Coil`,
later tiles join through `ArrangedPanel`, and the installed arrangement and
hinge systems own placement and rotation.

The example still owns `HingeChain` interaction state: selected versus
committed arrangement, direction, action, step/glide choice, pause state,
timers, unfold-first behavior, and queued refolding. Each frame it converts the
committed state into the root component's `fold` and `lean` fields. This is
direct evidence for F5 through F9 rather than evidence that the
arrangement geometry API is incomplete.

Two current API limitations required explicit lifecycle work in the example:

- `reconcile_hinge_arrangement` removes arrangement membership, runtime hinges,
  and root arrangement components while the fan or a mode morph owns the
  entities, then reconstructs membership when the hinge capability becomes
  active. F1 would replace this application-owned ownership handoff.
- Switching accordion/coil commits by explicitly removing both sibling
  arrangement types before inserting the selected component. F4 and
  [`mutex_arrangements.md`](mutex_arrangements.md) address that invariant.

The fan-to-hinge visual transition remains application-owned because Hana has
no coordinated relationship morph. Dynamic growth and stable ordering did not
require a new core mechanism: existing member observers and indices handled
new members once the example inserted `ArrangedPanel` in tile order.

## Accepted work and implementation order

- **A1:** Convert arrangement membership to a Bevy relationship and use its
  relationship target to maintain the reverse member collection. Remove custom
  reverse-list maintenance.
- **A2.1:** Name the `Member` component's arrangement-root `Entity` field
  `root_entity`.
- **A2.2:** Name the membership relationship pair `Member` / `Members`.
- **A3:** Remove `MemberIndex` and its assignment/repair systems. Derive current
  position by enumerating `Members`; introduce stored indexing later only if an
  accepted feature requires it.
- **A4.1:** Use a separate controller entity as the arrangement root. Make every
  physical tile a `Member`, keep physical `AnchoredTo` connection trees
  independent from membership order, and permit multiple physical connection
  roots. Keep `AnchoredTo` acyclic; track moving closed-loop support under F14.
- **A4.2 (reopened):** Add `Arrangement` as the controller identity and allow
  zero-member arrangements. Do not require `Strip`, `Accordion`, or `Coil`;
  directly authored `AnchoredTo` and `Hinge` data is valid. Decide the remaining
  topology-provider validity contract under A6 and the optional built-in recipe
  representation under A9.2.
- **A4.3:** Add a `Commands` extension method named `spawn_arrangement` without
  hidden defaults. Keep invalid resolved data and membership intact but make
  dependent writers inert, emit a non-spamming diagnostic, and reconcile
automatically after validity is restored. Finalize the helper parameters
after A4.2, A6, and A9.2.
- **A5:** Keep the topology-provider responsibility separate from
  `Strip`, `Accordion`, and `Coil`. Allow one connection topology to be reused
  with different folding behavior; keep staged `FoldSequence` orchestration
  outside both responsibilities.
- **A6.0:** Model logical-pattern and physical-graph authoring as a topology
  provider (`name TBD`) that may expose recipe-compatible fold directions.
  Expect regular triangle, quad, and hex sheets to support arbitrary finite
  dimensions plus Accordion, Coil, and Wrap; treat a strip as their
  one-dimensional degenerate case.
- **A6.1:** Let the topology provider decide independently for every `Member`
  whether it has no `AnchoredTo` or has one whose `AnchoredTo::target()` it
  selects. Never infer the preceding `Member` as a physical connection.
- **A7.1:** Represent every `AnchorPoint` orientation with a non-optional
  `AnchorOrientation`; its identity value means the entity's own local
  orientation.
- **A8:** Remove `ArrangementPlacement` and `MemberPlacement`. Add the shared,
  signed, unwrapped `bevy_kana::Angle` value and use it for Hana Valence's fixed
  connection angles, recipe offsets, and `Hinge` endpoints. The initially
  approved `HingePivot::reference_angle` conversion is superseded by its A9.4
  removal. Keep unit conversion explicit, then audit the rest of the workspace
  for scalar angular-displacement fields without blanket-converting
  orientations, progress values, or angular rates.
- **A9.1:** Move the unfolded and folded endpoint angles into `Hinge`, remove
  stored current-angle state and `FoldAngles`, and derive the current angle in
  `hinge_to_pose()`. Remove the two built-in angle writers; decide external
  progress under A9.3. Endpoint field names remain pending.
- **A9.3 (partial):** Keep `FoldStage` as the named simultaneous-playback
  stage. Use one duration-plus-easing value (`name TBD`) for sequence defaults
  and stage overrides; represent a distinct-duration fold as a one-member
  stage, and allow only easing to vary within a shared stage. Resolve easing
  per member with member override, then `FoldStage` override stored in
  `FoldSequence`, then sequence default. Keep only raw progress in
  `FoldSequenceState` and apply the selected curve in `hinge_to_pose()`. Keep
  `FoldSequenceState` as the canonical transport and `FoldCommandEvent` as its
  request path. Give `FoldCommand` the variants `Step(FoldDirection)`,
  `PlayTo(FoldEndpoint)`, `Pause`, `Resume`, and `Reverse`; retain paused
  journeys and preserve raw position across reversal. Keep `FoldSequence` as
  a separate optional component colocated with `Arrangement`, with
  `FoldSequenceState` as its derived runtime state. Defer continuous external
  driving to a single-writer raw-progress adapter.
- **A9.4:** Remove `FoldFromArrangement` and its snapshot request,
  diagnostic, system, and validation machinery. Retarget
  `FoldSequenceBuilder` to assign `FoldStage` directly to explicitly selected
  `Member` entities. Do not infer sequence participation from membership or
  renumber stages when members change. Remove `MemberPlacement`, whose only
  additional value was the live hinge angle eliminated by A9.1. Replace
  `ArrangementPlacement` with the connected payload of the named A6 result;
  keep `AnchoredTo`, `Edge`, and one fixed connection `Angle` together and
  store only final endpoint `Angle` values on `Hinge`. Remove
  `FoldSystems::Actuate` with `actuate_fold_hinges()` and retain
  `FoldSystems::Advance` as the controller-transport boundary before
  `hinge_to_pose()`. Remove `FoldAngleInvalidReason`, `FoldAngleDiagnostic`, and
  `FoldAngleDiagnostics`; defer a unified retained hinge-and-anchor diagnostic
  policy, including non-finite `Angle` failures and warning throttling, to F13.
  Remove public `Hinge::rotation()` without replacement so
  `hinge_to_pose()` remains the only fold-angle evaluation path. Keep
  the offset-pivot capability, remove `HingePivot::reference_angle`, calibrate
  the offset at the unfolded endpoint, and merge it into
  `Hinge::pivot_offset`. Remove the `HingePivot` component. Declare `AnchorPose`
  as a required component of `Hinge`; ordinary `Hinge` removal leaves the pose
  available for another writer. Remove unused `AnchorPoseLens` and the
  resulting empty tween integration; retain direct purpose-built `AnchorPose`
  writers and the future raw fold-progress adapter. Add nonempty,
  authoring-only `SharedFoldStage`; accept it through
  `FoldSequenceBuilder::stage(impl Into<SharedFoldStage>)`, retain fallible
  runtime collection conversion and duplicate-member validation, and queue no
  commands before validation succeeds.
- **A9.5:** Remove `FoldMember` / `FoldMembers`. Use `Member` / `Members` as
  the one arrangement-membership relationship, put `FoldStage` directly on
  participating members, and represent a fixed member by the absence of
  `FoldStage`. Support one active `FoldSequence` per `Arrangement` until a
  demonstrated feature requires overlapping sequence assignments.
- **F6:** Use `PlayTo(FoldEndpoint)` for explicit, idempotent terminal travel.
- **F7:** Add pause and resume commands plus
  `FoldSequenceState::is_paused()`.
- **F8 (partial):** Select adjacent-boundary versus terminal travel per command
  through `Step` versus `PlayTo`; decide scalar transport reuse under F5.
- **F13 (required follow-up):** Review one coherent retained diagnostic policy
  for anchor and hinge failures after removal of the adapter-specific fold-angle
  diagnostic family.
- **F15 (review item):** After A1-A10 are settled, review pattern-oriented
  builders that intelligently materialize common setup patterns through the
  final arrangement types.

As further decisions are confirmed, this section will record each deliverable,
its dependencies, and its ordering. Later decisions may reorder accepted work
without changing what was approved.

## Scope and phased-plan handoff

In-scope and out-of-scope work will be selected after the API, feature, and
mutual-exclusion reviews are complete. This document will then be converted
into a phased implementation plan; phase structure is intentionally not being
authored during the decision review.

## Deferred and rejected work

No work has been deferred or rejected yet.
