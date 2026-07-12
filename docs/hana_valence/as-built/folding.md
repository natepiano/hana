# Authored folding sequences

## What it is

`hana_valence` provides a reusable runtime for staged folding of anchored Bevy
assemblies. Authors declare which entities move together at each stage, their
absolute hinge endpoints, and optional physical pivot offsets; Hana validates
that authored data, owns playback, and actuates the hinges. `fairy_dust` adapts
the runtime into shared `Space`, `Shift+Space`, and endpoint-aware `P` controls
without duplicating the state machine. The examples cover an explicit
five-panel accordion, an arrangement-derived triangle strip with switchable
physical profiles, and a box net with grouped stages.

## How it works

A fold sequence is an entity carrying:

```rust
pub struct FoldSequence {
    pub step_seconds: f32,
    pub easing: EaseFunction,
    pub initial: FoldEndpoint,
}
```

`FoldSequence::new(step_seconds)` starts unfolded with
`EaseFunction::SmootherStep`. `.with_initial(FoldEndpoint::Folded)` starts at
the derived terminal boundary instead.

Moving entities opt into playback through the immutable Bevy relationship:

```rust
pub struct FoldMember {
    sequence: Entity,
    pub stage: FoldStage,
}
```

Construct it with `FoldMember::new(sequence_entity, FoldStage(index))`. Bevy
maintains the reverse `FoldMembers` relationship on the sequence. Stages are
zero-based; multiple distinct members may share one stage. A fixed root
normally has no `FoldMember`, so it establishes physical placement without
consuming a playback step.

Membership can be authored explicitly:

```rust
FoldSequenceBuilder::new(&mut commands, sequence)
    .stage([lid])
    .stage([north, south, east, west])
    .finish()?;
```

`FoldSequenceBuilder::finish() -> Result<(), FoldAuthorError>` validates all
groups before queuing relationship writes. It rejects empty stages and
duplicate members without partially authoring the sequence.

Alternatively, a sequence can carry:

```rust
FoldFromArrangement::new(arrangement_entity)
```

The one-time request waits until the arrangement has insertion-ordered
`ArrangementMembers` and every member has a `MemberIndex`. It then assigns
fresh consecutive `FoldStage` values, reconciles prior membership, and removes
the request. It does not copy `MemberIndex` values or infer physical fold data.

Validation derives the stage count and writes a read-only `FoldSequenceState`.
Its public accessors expose:

```rust
is_ready() -> bool
stage_count() -> usize
position() -> f32
target() -> usize
direction() -> FoldDirection
motion() -> FoldMotion
fraction(stage: FoldStage) -> f32
```

`position` is continuous and measured in stage-boundary units. `target` is an
integer boundary. `fraction(stage)` clamps `position - stage` to `0..=1` and
applies the sequence's easing, so grouped members receive the same fraction
without storing per-member playback state.

Playback uses a transport-independent command and an entity-targeted event:

```rust
pub enum FoldCommand {
    Step(FoldDirection),
    Play,
}

commands.trigger(FoldCommandEvent::new(
    sequence_entity,
    FoldCommand::Step(FoldDirection::Folding),
));
```

`FoldCommand::Step` moves toward one adjacent boundary. Repeated same-direction
steps during `FoldMotion::Step` can extend the queued target. A reversal chooses
the adjacent boundary from the current fractional position and never snaps.

`FoldCommand::Play` is endpoint-aware:

- Idle at unfolded plays toward folded.
- Idle at folded plays toward unfolded.
- Idle at an interior position continues the direction established by the
  latest step.
- During `FoldMotion::Step`, it promotes that step's direction to terminal
  playback.
- During `FoldMotion::Play`, it reverses immediately, keeps
  `FoldMotion::Play`, and preserves continuous position.

Playback advances through `Time<Virtual>` at one stage per `step_seconds`,
clamps exactly to the target, and becomes idle when it settles.

Each physical member carries:

```rust
pub struct FoldAngles {
    pub unfolded: f32,
    pub folded: f32,
}
```

`actuate_fold_hinges` reads the member's eased stage fraction and writes the
absolute hinge angle:

```text
angle = unfolded + fraction x (folded - unfolded)
```

`FoldAngles` is also the ownership marker for `Hinge::angle`.
`drive_arrangement_hinges` excludes entities carrying it; removing
`FoldAngles` returns ownership to the arrangement driver.

An optional external pivot is authored with:

```rust
pub struct HingePivot {
    pub offset: Vec3,
    pub reference_angle: f32,
}
```

`offset` is expressed in the source anchor's tangent frame at
`reference_angle`. `hinge_to_pose` converts the hinge axis into that frame and
computes:

```text
delta       = hinge.angle - reference_angle
translation = offset - rotate(axis, delta) x offset
```

The translation is written to `AnchorPose` and composes with the static
`AnchoredTo::offset` during anchor resolution. Without `HingePivot`, the hinge
retains its original absolute rotation behavior and writes zero pose
translation.

The frame data flow is:

```text
arrangement snapshot
  -> sequence validation
  -> FoldSystems::Advance
  -> optional consumer profile update
  -> FoldSystems::Actuate
  -> consumer-owned hinge_to_pose
  -> consumer-owned resolve_anchors
  -> transform propagation
```

`FoldPlugin` installs fold observers, diagnostics, validation, playback,
actuation, and schedule ordering. It deliberately does not install geometry
providers, arrangement driving, `hinge_to_pose`, anchor resolution, or
transform propagation.

`SprinkleBuilder<S>::with_fold_controls()` is Fairy Dust's adapter. It
idempotently installs `FoldPlugin`, enhanced input, routing, key reservations,
and title chips:

- `Space Fold`
- `Shift+Space Unfold`
- `P Play`

Exactly one ready sequence routes automatically. If several are ready, exactly
one must carry `FairyDustFoldTarget`. `P Play` remains highlighted for the whole
`FoldMotion::Play`, including after an immediate reversal, and clears on the
frame playback settles.

### File roles

| File | Role |
| --- | --- |
| `crates/hana_valence/src/fold/sequence.rs` | Authored sequence data, membership relationships, validation, runtime state, and sequence diagnostics |
| `crates/hana_valence/src/fold/playback.rs` | Commands, targeted event transport, endpoint-aware Play, continuous advancement, and reversal rules |
| `crates/hana_valence/src/fold/hinge.rs` | `FoldAngles`, hinge actuation, and rejected-angle diagnostics |
| `crates/hana_valence/src/fold/author.rs` | Explicit grouped authoring and one-time arrangement snapshots |
| `crates/hana_valence/src/fold/mod.rs` | `FoldPlugin`, exports, and fold-system ordering |
| `crates/hana_valence/src/hinge.rs` | `HingePivot` and invariant-pivot `AnchorPose` translation |
| `crates/hana_valence/src/arrange.rs` | Arrangement order and exclusion of `FoldAngles` from arrangement angle driving |
| `crates/fairy_dust/src/fold_controls.rs` | BEI bindings, ready-sequence selection, commands, diagnostics, and title-chip state |
| `crates/fairy_dust/src/builder/sprinkle.rs` | Public `.with_fold_controls()` capability |
| `crates/hana_valence/examples/staggered_unfold.rs` | Explicit five-stage physical accordion |
| `crates/hana_valence/examples/triangles.rs` | Arrangement-derived stages and queued algorithm profiles |
| `crates/hana_valence/examples/box.rs` | Explicit grouped stages over a nontrivial anchor topology |

## Invariants

- Hana owns validation, timing, direction, motion transitions, targets, and
  fractions. Fairy Dust reads `FoldSequenceState`; it does not reproduce those
  rules.
- `AnchoredTo` means physical placement. `FoldMember` independently and
  optionally means playback membership and stage order.
- Used stages begin at zero, are contiguous, and derive their count as
  `max + 1`. Multiple members may share a stage.
- A fixed root normally has no membership and consumes no stage.
- Runtime fields remain privately mutable. `FoldCommandEvent` is the public
  mutation path.
- Playback starts idle and has no autoplay, pause state, or pause control.
- At either endpoint, `Play` selects the opposite endpoint. During active Play,
  another Play reverses without changing continuous position.
- A step always establishes its requested direction. Interior idle Play follows
  that latest step direction, and Play during a step promotes it.
- A zero-stage sequence is ready and idle; Play cannot create motion for it.
- Invalid configuration or membership removes readiness and stops motion.
- `FoldAngles` exclusively owns `Hinge::angle` while present. Writer ownership
  is structural, not dependent on incidental system order.
- `FoldAngles` stores absolute endpoints. `HingePivot::reference_angle` affects
  only pivot compensation and is never subtracted from the authored hinge
  rotation.
- Pivot offsets use the source anchor's tangent frame. Missing, incompatible,
  degenerate, or non-finite pivot data skips the pose write rather than
  producing an incorrect transform.
- Static spacing belongs in `AnchoredTo::offset`; dynamic pivot compensation
  belongs in `AnchorPose::translation`.
- Arrangement snapshots author membership only. They never infer angle signs,
  endpoints, pivot offsets, or algorithm policy.
- Pending arrangement snapshots do not temporarily validate a new sequence as
  empty. Resnapshots retain the prior valid state until replacement succeeds.
- `FoldSystems::Advance` precedes `FoldSystems::Actuate`; actuation precedes
  consumer-installed `hinge_to_pose`.
- Passive Fairy Dust synchronization with no ready sequence stays quiet. An
  actual unroutable input records a diagnostic.
- Multiple ready sequences require exactly one `FairyDustFoldTarget`.
- Step chips highlight only directional `FoldMotion::Step`; `P Play` highlights
  only `FoldMotion::Play`.
- Title bars contain controls only. Teaching information belongs in
  screen-space panels.

## Calibration and gotchas

`FoldSequence::step_seconds` must be finite and positive. A focused app that
installs `FoldPlugin` directly must also provide `Time<Virtual>`; the plugin
does not initialize global time.

The initial endpoint is resolved on the first valid membership revision. Later
membership growth preserves numeric position and target. If the terminal
boundary shrinks, position and target clamp only as needed.

`FoldSequenceBuilder` validates its local description atomically, but its
successful writes use deferred `Commands`; it cannot synchronously prove that
every target entity will still exist when those commands apply.

`FoldFromArrangement` remains attached while arrangement data is incomplete and
records one readiness failure for that retained request. A successful snapshot
removes the request. Later arrangement changes require inserting a new request.

`FoldDiagnostics`, `FoldSnapshotDiagnostics`, `FoldAngleDiagnostics`, and Fairy
Dust's `FoldControlDiagnostics` retain bounded histories of 128 entries.

`staggered_unfold.rs` uses five consecutive stages with absolute folded angles:

```text
[PI/2, -PI, PI, -PI, PI]
```

Panel one stops parallel to the fixed mount. The remaining half-turns alternate
into a face-to-face stack. Panel pivot offsets use panel half-thickness rather
than rendered knuckle radius, and the visible segmented knuckles are derived
from the same `PanelJoint` definition as the physical pivot. The fixed gold
mount is outside the sequence. Its fold duration is `0.8` seconds per stage.

The staggered example's orbit camera uses TAA with `Msaa::Off`. This is
presentation configuration, not part of folding, but it is intentionally
compatible with `.with_stable_transparency()`, which also requires MSAA to
remain off.

`triangles.rs` snapshots five moving arrangement members into five stages. Every
algorithm profile uses `PI` as both `FoldAngles::unfolded` and
`HingePivot::reference_angle`; folded endpoints are `PI + signed_lean`, never
just the lean delta. Accordion uses one local sign, while Wrap alternates signs
and scales its small pivot spacing by member index. `TILE_GAP` is `0.001`, and
playback uses `SmoothStep` over `0.5` seconds per stage.

Triangle algorithm selection tracks `selected_algorithm` separately from
`active_algorithm`. The title selection changes immediately, but a profile
selected away from position zero remains queued. At exact
`position() == 0.0`, the example updates every affected `FoldAngles` and
`HingePivot` between `FoldSystems::Advance` and `FoldSystems::Actuate`, without
changing playback state or membership. Transform-only tile parents carry
`Visibility` so inherited visibility propagation does not emit Bevy B0004
warnings.

`box.rs` authors two stages: the lid alone, then all four walls. The lid is
physically anchored to the north wall, so the grouped wall stage carries it
into place. The center is the fixed root. Zero-thickness faces use absolute
`-PI/2` endpoints without `HingePivot`, and the sequence uses one second per
stage.

All three examples use `.with_brp_extras()`, Fairy Dust orbit controls, authored
camera-home targets, `.with_stable_transparency()`, camera-control panels,
controls-only title bars, and screen-space teaching panels.

Large macOS debug example binaries may emit `__eh_frame section too large`. It
is a shared compact-unwind metadata warning, not evidence of invalid folding
transforms or playback.

## Why

The sequence is an explicit relationship instead of an inferred descendant
walk because transform topology does not define playback order. The box
demonstrates the mismatch: the lid is a descendant of one wall but belongs to
the earlier stage, while four separate walls share the later stage.

Hana owns the state machine so every consumer receives identical stepping,
endpoint behavior, reversal, easing, validation, and direction semantics.
Fairy Dust remains an input and presentation adapter, avoiding a dependency
from Hana back into an example framework.

`FoldCommand` and `FoldCommandEvent` are separate so the requested operation
remains independent of transport while the event carries Bevy's entity target
explicitly through `sequence_entity`.

One continuous sequence position replaces per-member playback state. This
keeps grouped members synchronized and makes reversals continuous.

`FoldAngles` is an explicit ownership marker because two hinge writers ordered
only by schedule would remain fragile. Component presence makes the active
driver unambiguous.

`HingePivot` lives in the general hinge domain because external-axis rotation
is useful beyond staged folding. Expressing the offset in the source anchor
tangent frame matches resolver coordinates and preserves the pivot under
non-identity anchor frames.

Arrangement conversion is an explicit one-time snapshot because arrangement
order can supply stage order but cannot determine physical angle signs or pivot
geometry. Those remain next to the geometry and algorithm that understand
them.

Triangle profile changes wait for the unfolded boundary because replacing
angle and pivot calibration mid-fold would redefine the assembly beneath an
unchanged playback fraction. Applying the complete profile atomically at the
shared reference pose avoids visible jumps.
