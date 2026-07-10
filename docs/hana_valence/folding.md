# Authored folding sequences

Status: team-reviewed design plan; user decisions recorded.

## Intent

Add a reusable folding model to `hana_valence`, let `fairy_dust` present and bind
the standard controls, and reduce each folding example to authored geometry plus
fold metadata.

The common interaction contract is:

- examples start idle;
- `Space` advances one fold stage;
- `Shift+Space` moves back one fold stage;
- `P` continues from the current position to the terminal position in the most
  recently requested direction;
- a step or play command may reverse an in-progress motion without snapping;
- algorithm selection remains independent of playback; and
- the title bar contains controls only. Teaching text remains in screen-space
  panels.

There is no automatic playback and no pause state.

## Existing code and the missing model

`AnchoredTo` already records physical attachment: which source anchor meets which
target anchor. `AnchoredHere` supplies the reverse index used by resolution. That
relationship answers **where an entity is attached**.

`Strip`, `Member`, `MemberIndex`, and `ArrangementMembers` already describe an
ordered arrangement. `Accordion` and `FoldPattern` provide continuous fold
distribution for that arrangement. They do not describe a multi-stage timeline.

The examples currently duplicate that timeline:

- `staggered_unfold.rs` stores a step target and maps consecutive panels to
  consecutive intervals;
- `triangles.rs` does the same while separately selecting its fold algorithm;
  and
- `box.rs` divides one clock into a lid stage and a wall stage.

The box net demonstrates why descendants or arrangement length cannot define the
timeline. Its anchor graph branches, and four wall entities intentionally share
one stage. Five moving faces therefore produce two stages, not five.

The new model keeps three concerns separate:

| Concern | Owner | Data |
| --- | --- | --- |
| Physical attachment | `hana_valence` anchor resolver | `AnchoredTo` / `AnchoredHere` |
| Fold order and grouping | `hana_valence` folding domain | `FoldMember` / `FoldMembers` |
| Fold endpoint and pivot behavior | `hana_valence` hinge adapter or another consumer | `FoldAngles` / `HingePivot` |
| Keyboard bindings and title chips | `fairy_dust` | `FoldControls` |
| Teaching panels, materials, camera, and scene composition | each example | existing example code |

`AnchoredTo` will not gain playback policy. A fixed root or any other anchored
entity remains valid without joining a fold sequence.

## Hana Valence API

### Authored sequence entity

Each independently controlled fold timeline is an entity carrying authored
`FoldSequence` data. Validation adds the read-only runtime
`FoldSequenceState` after it has inspected the sequence's relationships.

```rust
commands.spawn(
    FoldSequence::new(STEP_SECONDS)
        .with_initial(FoldEndpoint::Unfolded),
);
```

`FoldSequence` contains the duration of one stage, Bevy's `EaseFunction`, and the
authored initial endpoint. The endpoint belongs here because a folded terminal
position is unknown until validation derives the stage count.

```rust
pub struct FoldSequence {
    pub step_seconds: f32,
    pub easing: EaseFunction,
    pub initial: FoldEndpoint,
}

pub enum FoldEndpoint {
    Unfolded,
    Folded,
}

pub struct FoldSequenceState {
    stages: usize,
    playback: FoldPlayback,
}

pub enum FoldDirection {
    Folding,
    Unfolding,
}

pub enum FoldMotion {
    Idle,
    Step,
    Play,
}
```

`FoldPlayback` privately stores continuous `position`, integer `target`,
`FoldDirection`, and `FoldMotion`. For a three-stage sequence, position and
target use `0.0..=3.0` and `0..=3`. Keeping the continuous position and discrete
target separate supports reversal without snapping and permits repeated
same-direction steps to queue more than one stage.

On the first valid membership revision, validation resolves `FoldEndpoint` into
the numeric initial boundary and inserts `FoldSequenceState`. An unfolded start
remembers `Folding`; a folded start remembers `Unfolding`. Later membership
growth preserves the numeric position. Membership removal clamps position and
target only when the new terminal boundary is lower.

`FoldSequenceState` exposes stage count, position, target, direction, motion,
readiness, and stage-fraction accessors. Its fields are not publicly mutable.
Fairy Dust reads this Hana-owned state and never repeats the validation rules.

### Optional fold relationship

Every moving entity may opt into one sequence with an immutable Bevy
relationship.

```rust
#[derive(Component)]
#[component(immutable)]
#[relationship(relationship_target = FoldMembers)]
pub struct FoldMember {
    #[relationship]
    sequence: Entity,
    pub stage: FoldStage,
}

#[derive(Component)]
#[relationship_target(relationship = FoldMember)]
pub struct FoldMembers(Vec<Entity>);

pub struct FoldStage(pub usize);
```

`FoldMember` is optional. Adding `AnchoredTo`, `Hinge`, or an arrangement does
not implicitly add it. The reverse relationship provides discovery; the stage
value provides authored order and grouping.

The sequence validator enforces these rules:

- stages begin at zero;
- used stage values are contiguous;
- more than one member may use a stage;
- a member belongs to at most one sequence because `FoldMember` has one
  relationship target;
- an empty sequence has zero stages and ignores playback commands; and
- the derived stage count is `max(stage) + 1`, after contiguity validation.

Membership changes trigger validation and rebuild `FoldSequenceState` metadata
only for the affected sequence. When a change lowers the stage count, playback
position and target are clamped to the new terminal boundary. Invalid membership
removes readiness, disables motion, and emits one diagnostic when that revision
is observed. An empty sequence is ready with zero stages, remains idle, and may
gain members later without reapplying its authored initial endpoint.

The fixed root normally has no `FoldMember`. It supplies the authored world
transform and the first physical attachment but does not consume a playback
stage.

### Commands and transitions

Callers trigger an event on the sequence entity. The target is implicit in the
trigger, so the event carries only the requested action.

```rust
#[derive(Event)]
pub enum FoldCommand {
    Step(FoldDirection),
    Play,
}

commands
    .entity(sequence)
    .trigger(FoldCommand::Step(FoldDirection::Folding));
```

The observer applies these transitions:

| Action | Condition | New target | Motion |
| --- | --- | --- | --- |
| `Step(Folding)` | already stepping toward folded | `min(target + 1, stages)` | `Step` if movement remains |
| `Step(Unfolding)` | already stepping toward unfolded | `target.saturating_sub(1)` | `Step` if movement remains |
| either step | idle, playing, or changing direction | adjacent boundary from `position` in the requested direction | `Step` if movement remains |
| `Play` | any | `stages` when folding, `0` when unfolding | `Play` if movement remains |

A folding step chooses `floor(position) + 1`; an unfolding step chooses
`ceil(position) - 1`. Those formulas move one full boundary even when position
is already an integer. They are clamped to `0..=stages`.

A direction-changing step therefore retargets from the current continuous
position, not from a queued or terminal target. It does not wait and does not
snap. Only a same-direction command during `FoldMotion::Step` extends the
existing discrete target by one.

At a target boundary, motion becomes `Idle`. Reaching an intermediate step
target does not change the remembered direction. `Play` at the matching terminal
boundary is a no-op. A second `Play` event while already playing in the same
direction is also a no-op.

The update system advances `position` by elapsed virtual time and clamps it at
the target. `FoldSequenceState::fraction` derives a stage's local fraction on
demand:

```rust
let fraction = (position - stage as f32).clamp(0.0, 1.0);
```

Easing is applied to that local fraction. All members in one stage receive the
same fraction.

### Fold actuation

Sequence timing must not require every consumer to animate `Hinge`.
`FoldSequenceState::fraction(stage)` exposes the eased value without writing a
derived component to every member every frame. The built-in hinge adapter covers
the examples.

```rust
pub struct FoldAngles {
    pub unfolded: f32,
    pub folded: f32,
}
```

The hinge adapter follows `FoldMember` to its sequence, reads the fraction,
maps `FoldAngles::unfolded` to `FoldAngles::folded`, and writes `Hinge::angle`
before `hinge_to_pose`. Another consumer follows the same relationship and calls
the fraction accessor without carrying `FoldAngles` or `Hinge`.

This split preserves the current `Accordion` arrangement API. A continuously
controlled accordion may keep using `Accordion::fold`; a staged demonstration
uses `FoldMember` and the hinge adapter.

`FoldAngles` is an explicit ownership marker for `Hinge::angle`.
`drive_arrangement_hinges` excludes members carrying it, preventing two systems
from writing the same angle. Removing `FoldAngles` returns ownership to the
arrangement driver.

### Physical pivot offset

The current examples use `ResolvedAnchorOffset` to compensate for a hinge axis
outside the rendered face. That is example-local, duplicates the same equation,
and replaces the authored `AnchoredTo::offset` while present.

Add an optional hinge pivot component owned by the hinge domain:

```rust
pub struct HingePivot {
    pub offset: Vec3,
    pub reference_angle: f32,
}
```

`offset` is the pivot-line offset from the source anchor, expressed in that
anchor's tangent frame at `reference_angle`. `reference_angle` is the angle at
which the pivot contributes no translation. This matters for triangle strips
whose unfolded rest orientation is a half-turn.

`hinge_to_pose` resolves the edge axis and pivot offset into the same source
anchor tangent frame, computes the delta from `reference_angle`, and uses that
delta rotation to keep the pivot line invariant:

```rust
let delta = hinge.angle - pivot.reference_angle;
let translation = pivot.offset - Quat::from_axis_angle(axis, delta) * pivot.offset;
```

The sketch assumes `axis` and `offset` have already been converted into the same
tangent frame. The implementation must use `AnchoredTo::source_anchor` and its
`AnchorPoint::frame` for that conversion. It must diagnose a missing source
anchor relationship instead of silently mixing frames. A non-identity source
frame is part of the required test matrix.

Without `HingePivot`, translation remains zero and existing callers keep their
current behavior. `AnchoredTo::offset` remains available for authored spacing,
and the resolver composes that spacing with `AnchorPose::translation`.

The hinge adapter assigns `reference_angle = FoldAngles::unfolded` unless the
caller supplies another value. This makes the unfolded endpoint the physical
reference for the standard fold algorithms.

### Authoring helpers

The first release should provide two non-generic authoring paths.

1. `FoldFromArrangement` is a one-time authoring request placed on a sequence.
   A scheduled system processes it after member-index assignment and deferred
   command application. It enumerates `ArrangementMembers::iter()` and assigns
   fresh zero-based stages in that order. It does not copy `MemberIndex`, whose
   values begin at one and may contain gaps after removal.
2. `FoldSequenceBuilder::new` accepts explicit groups for branched structures.
   Each call to `stage` adds one stage and attaches every listed entity to it.

Conceptual setup:

```rust
FoldSequenceBuilder::new(&mut commands, sequence)
    .stage([lid])
    .stage([north, south, east, west])
    .finish();
```

The builder writes ordinary components and relationships; it does not become a
second runtime representation. It returns validation errors for duplicate
members and empty authored groups. The scheduled arrangement request reports a
diagnostic if the sequence or arrangement is missing and removes itself after a
successful snapshot. Later arrangement edits require another explicit request;
they do not silently rewrite an active fold sequence.

Arrangement conversion authors membership and stages only. Fold endpoints and
pivots belong to separately named algorithm recipes or to explicit example
setup. In particular, a triangle accordion uses a constant local fold sign
because its rest angle is a half-turn, while a quad accordion alternates local
signs. A generic arrangement helper cannot infer that distinction from order.

Algorithmic constructors may later generate accordion strips, inward wraps,
radial nets, or other structures. They must produce the same `AnchoredTo`,
`FoldMember`, `FoldAngles`, and `HingePivot` data that manual authoring produces.
Playback remains independent of which constructor authored the data.

## Fairy Dust integration

`fairy_dust` will add a workspace dependency on `hana_valence`. This is an
intentional direct dependency: Fairy Dust adapts Hana Valence fold state into
example controls in the same way it adapts other workspace APIs into title-bar
presentation.

The entry point is a builder capability:

```rust
fairy_dust::sprinkle_example()
    .with_fold_controls()
```

`with_fold_controls` will:

- register BEI actions for fold, unfold, and play;
- bind `Space`, `Shift+Space`, and `P`;
- add the `Space Fold`, `Shift+Space Unfold`, and `P Play` title chips;
- send `FoldCommand` events to the controlled sequence; and
- mirror `FoldSequenceState::motion()` to chip activation.

The step chips are active only while a one-stage motion in their direction is in
progress. `P Play` is active only during play-to-terminal motion. Idle controls
are inactive. There is no pause chip.

The standard examples contain one sequence, so automatic routing is convenient.
The proposed routing contract is:

- if exactly one ready `FoldSequenceState` exists, controls use it;
- if more than one exists, a `FairyDustFoldTarget` marker selects one; and
- zero sequences, multiple marked sequences, or multiple unmarked sequences
  emit a diagnostic and leave the controls inactive.

The marker lives in `fairy_dust`; it is presentation selection, not fold-domain
data. No `hana_valence` type refers back to Fairy Dust. A sequence is not a
routing candidate until Hana Valence publishes ready state.

Bindings use `bevy_enhanced_input`. The examples will remove direct
`ButtonInput<KeyCode>` reads for folding. The algorithm toggle in `triangles.rs`
also remains a BEI action and operates independently of the three playback
actions.

## Example migrations

All three examples retain `.with_brp_extras()`, the Fairy Dust orbit camera, and
a camera-home target authored from the visible geometry. Existing stable
transparency configuration remains enabled. The fold API migration does not
replace those presentation features.

### `staggered_unfold.rs`

- Keep the gold fixed root and its first physical attachment visually distinct.
- Give each of the five moving panels a consecutive `FoldStage`.
- Use alternating `FoldAngles` so the panels close as an accordion, not as an
  inward wrap.
- Put each rendered hinge visual on the same edge line used by its child panel's
  pivot.
- Use `HingePivot` so a panel rotates about the post axis rather than passing
  through the previous panel or the fixed root.
- Choose endpoint angles that let the panels rest flat against one another while
  preventing panel one from rotating through the root face.
- Remove the local playback resource, raw keyboard system, fold-motion mirror,
  and pivot-compensation system.

Pivot placement is authored per joint from the two connected solids. A
panel-to-panel half-turn uses the panel half-thickness needed for face contact;
the rendered cylinder radius is not added to the kinematic offset. The first
root joint has its own mount-face clearance and quarter-turn endpoint. Segmented
hinge knuckles can show the axis without placing a full cylinder between faces
that must close together.

The rendered knuckles do not create the anchor relationship or the sequence.
The panel spawn helper creates the visuals and fold metadata from one authored
edge definition so their axes cannot drift apart.

### `triangles.rs`

- Build the sequence with `FoldFromArrangement` after member indices settle.
- Assign one crease per stage.
- Replace `AccordionPlayback` and local motion state with the sequence runtime.
- Replace `apply_crease_gap` with `HingePivot` using the triangle rest angle as
  `reference_angle`.
- Keep the accordion/wrap toggle. It selects `FoldAngles` and pivot values but
  does not reset or replace the sequence timeline. Its behavior away from the
  unfolded boundary is Proposed Decision 2.
- Route fold, unfold, and play through `with_fold_controls`.

The algorithm toggle is orthogonal because both algorithms use the same member
order and stage grouping. If a future algorithm needs a different grouping, it
must author a different sequence plan instead of changing stage membership while
motion is active.

### `box.rs`

- Spawn one sequence in the unfolded state.
- Add the lid as stage zero.
- Add north, south, east, and west to stage one.
- Keep the center face outside the sequence as the fixed root.
- Replace `FoldPhase`, `FoldTarget`, `FoldPlayback`, and the local hinge driver
  with `FoldMember` plus `FoldAngles`.
- Replace replay input with the common three controls.

This example is the required grouped-stage test: its stage count must derive as
two even though five entities carry `FoldMember`.

## Module ownership

The new Hana Valence code belongs in one folding domain with types beside their
behavior:

```text
crates/hana_valence/src/
  fold/
    mod.rs        FoldPlugin and public exports
    sequence.rs   FoldSequence, relationship, validation, runtime state
    playback.rs   command events and state transitions
    hinge.rs      FoldAngles and the built-in hinge adapter
    author.rs     FoldSequenceBuilder
```

`HingePivot` remains in the existing hinge domain because it changes
`hinge_to_pose` behavior outside staged playback too. `fold::hinge` depends on
the public hinge API; `hinge.rs` does not depend on the fold module.

The recommended answer to Proposed Decision 1 is a public `FoldPlugin`. It adds
only fold observers, update systems, reflection, and diagnostics; it does not
register `hinge_to_pose`, anchor resolution, geometry providers, or arrangement
drivers. This is an explicit exception to Hana Valence's current
consumer-wiring-only contract and adds the workspace `bevy_app` dependency.
Playback timing also adds the focused workspace `bevy_time` dependency.

`FoldSystems::Advance` and `FoldSystems::Actuate` run in that order inside
`AnchorSystems::AnimatePose`. Consumers continue to register `hinge_to_pose`
after `FoldSystems::Actuate` and `resolve_anchors` in `AnchorSystems::Resolve`.
Fairy Dust's `with_fold_controls` installs `FoldPlugin` idempotently, then adds
only its BEI and title-chip adapter. Non-Fairy consumers add `FoldPlugin`
directly.

## Implementation phases

### Phase 1: sequence and playback

- Add `FoldSequence`, `FoldMember`, `FoldMembers`, and `FoldStage`.
- Add validation, `FoldSequenceState`, derived stage count, and diagnostics.
- Add `FoldPlayback`, `FoldCommand`, and the playback update.
- Add the focused `bevy_time` dependency for virtual-time advancement.
- Expose stage fractions through the runtime-state accessor.
- Unit-test every state transition, queued and play reversal, grouped stage,
  invalid stage set, sequence-before-members, members-before-sequence, membership
  growth and removal, and empty sequence.

### Phase 2: hinge actuation and physical pivot

- Add `FoldAngles` and its adapter system.
- Add `HingePivot` and update `hinge_to_pose`.
- Exclude `FoldAngles` members from the arrangement hinge driver.
- Prove authored offsets and pivot translation compose.
- Add resolver tests for zero pivot, an external axis, a nonzero reference
  angle, a non-identity source anchor frame, endpoint reversal, and alternating
  fold signs.

### Phase 3: authoring helpers

- Add the explicit grouped-stage builder.
- Add the scheduled one-time arrangement snapshot.
- Add tests that manual and generated structures produce equivalent components.

### Phase 4: Fairy Dust controls

- Add the workspace dependency and `with_fold_controls` capability.
- Install the approved Hana folding runtime without registering the anchor
  resolver twice.
- Add BEI actions, modifier-aware bindings, target routing, diagnostics, and
  title-chip activation.
- Test routing for zero, one, and multiple sequences.
- Test that algorithm actions coexist with the standard bindings.

### Phase 5: example migrations

- Migrate `staggered_unfold.rs`, then verify the physical endpoints visually.
- Migrate `triangles.rs`, preserving both algorithms at every playback boundary.
- Migrate `box.rs`, preserving its two-stage grouping.
- Remove local timing, raw fold input, and duplicate pivot logic.
- Verify the authored camera-home target at startup for each example.

## Acceptance criteria

- No folding example moves until input occurs.
- One `Space` press advances one stage; one `Shift+Space` press reverses one
  stage.
- `P` reaches the terminal boundary in the remembered direction from any
  continuous position.
- Reversal from a queued step or play-to-terminal motion immediately moves
  toward the adjacent boundary in the newly requested direction, never snaps,
  and never produces a non-finite transform.
- Grouped members move with the same stage fraction.
- The fixed root never consumes a stage.
- Accordion folds use physically consistent external pivot axes, finish with
  panel faces in contact, and do not pass through the fixed root at either
  endpoint.
- The triangle algorithm toggle does not change playback position, target, or
  direction.
- Fairy Dust owns all three standard bindings and title-chip activation through
  BEI.
- The title bar contains controls only; teaching information remains in
  screen-space panels.
- BRP extras expose the configured port, configured stable transparency remains
  enabled, and the orbit camera retains a useful home view.
- `cargo nextest run -p hana_valence` and the relevant Fairy Dust tests pass.
- All three examples compile without example-local playback resources.

## User decisions

### 1. Folding runtime installation

Should Hana Valence add a public `FoldPlugin` and a workspace `bevy_app`
dependency?

**Decision: approved.** Hana Valence owns the folding state machine through a
public `FoldPlugin`. Fairy Dust installs it idempotently; anchor resolution
remains consumer-wired.

- **Add `FoldPlugin` (recommended).** Hana owns the state machine everywhere,
  Fairy Dust installs it idempotently, non-Fairy consumers can opt in directly,
  and the existing anchor resolver remains consumer-wired.
- Keep Hana entirely component-and-system-only. Fairy Dust and every other
  consumer must reproduce the exact folding-system registration and ordering.

The body of this plan currently follows the recommended plugin option.

### 2. Algorithm toggle during a nonzero fold

What should happen when the triangle accordion/wrap toggle is requested away
from the fully unfolded boundary?

**Decision: queue until fully unfolded.** The selection changes immediately,
but its endpoint and pivot profile activates only when playback reaches zero.
This avoids an instantaneous transform change and does not add another animation
clock.

- **Queue the selected algorithm until fully unfolded (recommended).** This
  avoids an instantaneous transform change and adds no second animation clock.
- Add a separate transition that interpolates between algorithm profiles. This
  responds immediately but adds state, collision questions, and another control
  highlight contract.
- Apply the new profile immediately at the current fold fraction. This is the
  smallest implementation but visibly jumps between configurations.

All three choices keep stage membership, playback position, target, and
direction unchanged. Only the selected endpoint and pivot profile behavior
differs.
