# Authored folding sequences

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Adds Hana-owned staged folding, Fairy Dust controls, and three migrated folding examples.

## Delegation Context

- **Project:** `hana` Cargo workspace — Bevy-based visual/UI libraries and examples; `hana_valence` — anchor relationships, arrangements, hinge drivers, and the new authored folding runtime; `fairy_dust` — workspace example builder that adapts Hana folding state into BEI controls and title chips; `bevy_diegetic` — existing consumer that wires Hana anchor/arrangement systems and must remain the single anchor-resolver registration path.
- **Stack:** Rust 2024; Bevy `0.19.0` ECS relationships, observers, plugins, schedules, reflection, `Time<Virtual>`, transforms, and math/easing; `bevy_enhanced_input 0.26.0` with `bevy_kana` action macros; `bevy_tween 0.13.0` remains available but example-local folding tweens are removed; focused `bevy_app 0.19.0` and `bevy_time 0.19.0` dependencies for `FoldPlugin` and playback.
- **Layout:**
  - `Cargo.toml` — workspace dependencies and strict lint policy.
  - `crates/hana_valence/{Cargo.toml,src/,tests/,examples/,fixtures.rs}` — fold domain, anchor/hinge integration, tests, and three migrations.
  - `crates/hana_valence/src/fold/{mod.rs,sequence.rs,playback.rs,hinge.rs,author.rs}` — new folding domain.
  - `crates/fairy_dust/{Cargo.toml,src/}` — direct Hana dependency, fold-control capability, BEI bindings, routing, and title-chip state.
  - `crates/bevy_diegetic/src/panel/mod.rs` — read-only scheduling/registration contract that Fairy Dust already installs.
- **Key files:**
  - `Cargo.toml:10-46` — add focused workspace `bevy_app`/`bevy_time` dependencies; Bevy, BEI, tween, and workspace path dependencies live here; `Cargo.toml:48-79` defines denied Clippy groups and `missing_docs`.
  - `crates/hana_valence/Cargo.toml:15-35` — add `bevy_app` and `bevy_time`; preserve the optional `tween` feature and Fairy Dust dev dependency.
  - `crates/hana_valence/src/lib.rs:1-130` — update crate-level wiring/ownership documentation for the approved `FoldPlugin` exception; `crates/hana_valence/src/lib.rs:132-185` — declare and re-export the fold modules and `HingePivot`.
  - `crates/hana_valence/src/fold/mod.rs` — new public `FoldPlugin`, `FoldSystems::{Advance, Actuate}`, reflection/observer/system registration, and public exports.
  - `crates/hana_valence/src/fold/sequence.rs` — new `FoldSequence`, `FoldEndpoint`, immutable `FoldMember` relationship, reverse `FoldMembers`, `FoldStage`, validation/revision handling, diagnostics, and read-only `FoldSequenceState`; membership/state tests live here.
  - `crates/hana_valence/src/fold/playback.rs` — new private `FoldPlayback`, public `FoldDirection`, `FoldMotion`, `FoldCommand` payload and targeted `FoldCommandEvent`, transition observer, virtual-time advancement, fraction accessors, and transition/reversal tests.
  - `crates/hana_valence/src/fold/hinge.rs` — new `FoldAngles` and sequence-fraction-to-`Hinge::angle` adapter; actuation tests live here.
  - `crates/hana_valence/src/fold/author.rs` — new `FoldSequenceBuilder`, `FoldFromArrangement`, scheduled one-time arrangement snapshot, diagnostics, and manual/generated equivalence tests.
  - `crates/hana_valence/src/pose.rs:41-53` — existing `AnchorSystems` ordering contract into which `FoldSystems::Advance` then `FoldSystems::Actuate` must fit.
  - `crates/hana_valence/src/relation.rs:15-103` — immutable `AnchoredTo`/`AnchoredHere` relationship pattern to mirror for `FoldMember`; `crates/hana_valence/src/relation.rs:105-114` — resolver-owned offset that pivot support must no longer duplicate.
  - `crates/hana_valence/src/geometry.rs:22-35` — `AnchorPoint::frame` tangent-frame source for pivot conversion; `crates/hana_valence/src/geometry.rs:37-98` — ordered edge/axis semantics and failures; `crates/hana_valence/src/geometry.rs:100-169` — resolved geometry validation.
  - `crates/hana_valence/src/hinge.rs:19-99` — add optional `HingePivot` and make `hinge_to_pose` compute invariant-pivot translation without changing no-pivot behavior; extend inline tests at `crates/hana_valence/src/hinge.rs:101-334`.
  - `crates/hana_valence/src/arrange.rs:57-177` — existing `FoldPattern`, `Accordion`, `MemberIndex`, and insertion-ordered `ArrangementMembers` used by `FoldFromArrangement`; `crates/hana_valence/src/arrange.rs:257-400` — member assignment and arrangement hinge driver; exclude entities carrying `FoldAngles` and extend inline tests beginning at `crates/hana_valence/src/arrange.rs:509`.
  - `crates/hana_valence/src/resolve.rs:201-314` — verify authored `AnchoredTo::offset` and pivot-produced `AnchorPose::translation` compose as `offset + pose.translation`; extend resolver fixtures/tests beginning at `crates/hana_valence/src/resolve.rs:425`.
  - `crates/hana_valence/fixtures.rs` — shared quad/triangle anchor geometry; extend only if a required non-identity-frame fixture cannot remain local to a test.
  - `crates/hana_valence/tests/arrangements.rs:82-170` — existing triangle-strip and box-net integration tests; add staged arrangement/grouped-box and physical endpoint coverage; `crates/hana_valence/tests/arrangements.rs:172-255` — current manual scheduling and spawn helpers to migrate to the fold runtime.
  - `crates/fairy_dust/Cargo.toml:16-35` — add a direct workspace `hana_valence` dependency.
  - `crates/fairy_dust/src/lib.rs:41-62` — declare the fold-control module; `crates/fairy_dust/src/lib.rs:64-130` — export `FairyDustFoldTarget` and public capability types; `crates/fairy_dust/src/lib.rs:141-186` — plugin deduplication contract.
  - `crates/fairy_dust/src/fold_controls.rs` — new `with_fold_controls` support: ensure `FoldPlugin`/`EnhancedInputPlugin`, create BEI fold/unfold/play actions and contexts, bind bare `Space`, `Shift+Space`, and `P`, route to the sole ready sequence or marked `FairyDustFoldTarget`, emit routing diagnostics, reserve conflicting bare keys, and synchronize title-chip activation; routing/coexistence tests live here.
  - `crates/fairy_dust/src/builder/sprinkle.rs:62-213` — add state-agnostic `SprinkleBuilder<S>::with_fold_controls`; `crates/fairy_dust/src/builder/sprinkle.rs:341-343` is the app escape hatch that migrated examples should not need for folding.
  - `crates/fairy_dust/src/builder/title_bar.rs:41-151` — existing chip-state wiring; add a forwarding method only if an example invokes `with_fold_controls` from `TitleBarBuilder`.
  - `crates/fairy_dust/src/screen_panels/mod.rs:65-99` — title-bar installation and capability-time control registry used before Startup.
  - `crates/fairy_dust/src/screen_panels/title_bar.rs:154-177` — title-control registry; `crates/fairy_dust/src/screen_panels/title_bar.rs:189-327` — `ControlActivation`/`TitleBarControlState` synchronization surface.
  - `crates/fairy_dust/src/shortcuts.rs:36-105` — idempotent shortcut registry and reserved-key mechanism; `crates/fairy_dust/src/shortcuts.rs:107-147` — modifier/collision rules.
  - `crates/fairy_dust/src/restart.rs:27-65` — canonical Fairy Dust BEI/`bevy_kana` installation and action-binding pattern.
  - `crates/fairy_dust/src/constants.rs` — stable fold/unfold/play chip ids, visible labels, and reserved-key labels.
  - `crates/bevy_diegetic/src/panel/mod.rs:172-239` — read-only installation of membership observers, `AnchorSystems`, arrangement driver, `hinge_to_pose`, and `resolve_anchors`; do not duplicate these from `FoldPlugin` or Fairy Dust.
  - `crates/hana_valence/examples/staggered_unfold.rs:177-262` — remove autoplay/pause/local step state, tween registration, raw fold input, and duplicate scheduling; `crates/hana_valence/examples/staggered_unfold.rs:271-415` — author sequence/member/angle/pivot data while retaining mount, visible knuckles, home proxy, orbit camera, BRP extras, stable transparency, and teaching panel; remove local pivot compensation at `crates/hana_valence/examples/staggered_unfold.rs:483-497`.
  - `crates/hana_valence/examples/triangles.rs:109-179` — replace local playback/motion state while retaining algorithm selection; `crates/hana_valence/examples/triangles.rs:192-263` — install common fold controls and retain presentation capabilities; `crates/hana_valence/examples/triangles.rs:265-445` — use `FoldFromArrangement`, `FoldAngles`, `HingePivot`, and queued-at-unfolded algorithm profile switching; remove raw input/local timing/pivot code.
  - `crates/hana_valence/examples/box.rs:61-89` — replace `FoldTarget`, `FoldPhase`, and local playback; `crates/hana_valence/examples/box.rs:91-123` — install standard controls while retaining presentation; `crates/hana_valence/examples/box.rs:125-230` — author one sequence with lid stage `0`, four wall members at stage `1`, and fixed center outside the sequence; `crates/hana_valence/examples/box.rs:249-280` — attach `FoldMember`/`FoldAngles` through the hinged-face helper.
- **Build:** `cargo check --workspace --all-targets --all-features`; explicitly compile migrated examples with `cargo check -p hana_valence --examples --all-features`.
- **Test:** During Hana phases run `cargo nextest run -p hana_valence --all-features`; during Fairy Dust integration run `cargo nextest run -p fairy_dust --all-features`; final gate is `cargo nextest run --workspace --all-features`. Do not use `cargo test`.
- **Lint:** Run the full local `clippy` skill/workflow from `/Users/natemccoy/.codex/skills/generated-from-claude/clippy/SKILL.md` (`/clippy`), including its cache check, Mend/fix gate, mandatory rule-by-rule style review, Clippy, rustdoc, approval gate for findings, nightly-format wrapper, and completion report; do not replace it with hand-expanded Cargo subsets.
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_valence_init`
- **Invariants:** Use direct Cargo-family commands with configured `sccache`; because `origin` is owned by `natepiano`, use `cargo +nightly fmt`/`cargo +nightly fmt --all` and never plain `cargo fmt`; public Rust APIs require documentation and workspace lint groups are denied. Hana Valence owns the fold state machine through the approved public `FoldPlugin`; that plugin installs only fold observers/systems/reflected components/diagnostics and must not install anchor geometry providers, arrangement drivers, `hinge_to_pose`, `resolve_anchors`, or transform propagation. Concrete reflected types derive `Reflect` and use workspace `reflect_auto_register`; do not add explicit `register_type` calls except for generic or foreign cases. Stage-index conversions use `bevy_kana::ToF32`, never casts. `FoldSystems::Advance` precedes `FoldSystems::Actuate` inside `AnchorSystems::AnimatePose`; consumer `hinge_to_pose` runs after actuation and anchor resolution remains in `AnchorSystems::Resolve`. `AnchoredTo` continues to mean physical placement only; optional immutable `FoldMember` means playback membership/order, the fixed root normally has none, stages start at zero, are contiguous, may group multiple members, and derive count as `max + 1`; invalid revisions disable readiness and motion, while an empty sequence is ready/idle with zero stages. Hana-owned playback starts idle, has no pause/autoplay, preserves continuous position on reversal, uses `Space` for one folding stage, `Shift+Space` for one unfolding stage, and `P` to the terminal boundary in the remembered direction; fields remain privately mutable and Fairy Dust reads accessors rather than reproducing state rules. `FoldAngles` exclusively owns `Hinge::angle`; arrangement driving excludes those members. `HingePivot` is optional, uses the source anchor tangent frame and `reference_angle`, preserves existing zero translation when absent, diagnoses missing/mismatched source data, and composes with authored `AnchoredTo::offset` through `AnchorPose::translation`; transforms must remain finite. Arrangement conversion snapshots insertion-ordered `ArrangementMembers` only after every entry has `MemberIndex`, assigns fresh zero-based stages rather than copying `MemberIndex`, retains its request until ready, and never infers endpoint signs/pivots. Fairy Dust may depend directly on Hana Valence, installs `FoldPlugin` and BEI idempotently, owns only controls/routing/presentation, and never introduces a Hana-to-Fairy dependency; exactly one ready sequence routes automatically, multiple sequences require exactly one `FairyDustFoldTarget`, and ambiguous/absent targets diagnose and leave chips inactive. Step chips highlight only their one-stage motion; `P Play` highlights only play-to-terminal; title bars contain controls only. Triangle algorithm selection is orthogonal to membership/playback and, when requested away from position zero, queues its endpoint/pivot profile until fully unfolded without changing position, target, or direction. All three examples retain BRP extras, Fairy Dust orbit camera and useful authored home target, configured stable transparency, camera-control panel, and screen-space teaching content; migrations remove example-local playback resources, raw fold input, autoplay/pause controls, and duplicate pivot math. Phases 7–9 are a parallel wave after phases 1–6 and must not edit shared fixtures or each other's example files.

## Phases

### Phase 1 — Fold sequence model and validation  · status: done (checkpoint)

#### Work Order

**Goal:** Add the Hana-owned fold sequence entity, optional membership relationship, validated runtime state, and fold-only plugin boundary.

**Spec:**

- Add focused workspace/member `bevy_app` dependencies and create `crates/hana_valence/src/fold/{mod.rs,sequence.rs,playback.rs}`. `fold/mod.rs` owns the public `FoldPlugin` and public exports. Update the crate docs: anchor resolution remains consumer-wired, while the approved `FoldPlugin` is the one exception and owns only folding.
- Define the authored configuration exactly around this model:

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
  ```

  Provide `FoldSequence::new(step_seconds)` and `.with_initial(endpoint)`. The default initial endpoint is unfolded. Reject or diagnose non-finite/non-positive `step_seconds`; such a sequence is not ready.
- Add the optional immutable Bevy relationship exactly around this model:

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

  Mirror the `AnchoredTo`/`AnchoredHere` accessor and reflection conventions. `FoldMember` is never inserted implicitly by `AnchoredTo`, `Hinge`, or an arrangement. A member has one sequence target; duplicate members in a stage are invalid; multiple distinct members may share a stage.
- Define public `FoldDirection::{Folding, Unfolding}` and `FoldMotion::{Idle, Step, Play}` plus private `FoldPlayback` fields `position: f32`, `target: usize`, `direction: FoldDirection`, and `motion: FoldMotion` in `fold/playback.rs`. Do not expose mutable fields.
- Define `FoldSequenceState` in `fold/sequence.rs` with private `stages` and private playback state. Expose read-only accessors for readiness, stage count, continuous position, target boundary, direction, and motion. Fairy Dust must be able to read this state without repeating validation.
- Validate each affected sequence when its relationship revision changes. Used stages begin at zero and are contiguous; stage count is `max(stage) + 1`. An empty sequence is ready and idle with zero stages. Invalid stages or configuration remove readiness, disable motion, and emit one Hana-owned diagnostic per invalid revision.
- On the first valid membership revision, resolve `FoldEndpoint::Unfolded` to boundary zero with remembered direction `Folding`, or `FoldEndpoint::Folded` to the derived terminal boundary with remembered direction `Unfolding`. Later membership growth preserves numeric position/target. Membership removal clamps them only when the new terminal boundary is lower. The fixed root normally carries no `FoldMember` and consumes no stage.
- Register validation, relationship observers, diagnostics, and reflection in `FoldPlugin`. Do not register geometry providers, arrangement driving, `hinge_to_pose`, `resolve_anchors`, or transform propagation. Define public `FoldSystems::{Advance, Actuate}` now for later phases, without adding actuation behavior yet.
- Keep membership, validation, initialization-order, mutation, and diagnostic tests beside `FoldSequence`/`FoldMember`. Cover sequence-before-members, members-before-sequence, grouped stages, gaps, duplicates, empty initialization, later growth, removal/clamping, folded initialization, and relationship replacement/removal.

**Files:**
- `Cargo.toml` — add the focused workspace `bevy_app` dependency.
- `crates/hana_valence/Cargo.toml` — inherit `bevy_app`.
- `crates/hana_valence/src/lib.rs` — declare/re-export the fold domain and update the plugin ownership documentation.
- `crates/hana_valence/src/fold/mod.rs` — create `FoldPlugin`, `FoldSystems`, registration, and exports.
- `crates/hana_valence/src/fold/sequence.rs` — create authored types, relationship, validation, runtime state, diagnostics, and tests.
- `crates/hana_valence/src/fold/playback.rs` — create playback state types needed by `FoldSequenceState`; no command/clock systems yet.
- `crates/hana_valence/src/relation.rs` — read-only relationship implementation reference.

**Constraints from prior phases:** None.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo nextest run -p hana_valence --all-features` passes the named relationship/validation/initialization tests; `cargo check --workspace --all-targets --all-features` is green; `FoldPlugin` does not duplicate any registration in `crates/bevy_diegetic/src/panel/mod.rs`.

#### Retrospective

**What worked:**

- `FoldMember` relationship hooks plus `FoldValidationPending` revalidate old and new sequence targets after replacement/removal.
- The fold-only `FoldPlugin` leaves anchor/arrangement registration with consumers; 44 Hana Valence tests cover initialization, grouping, invalid revisions, growth, removal, and reflection contracts.

**What deviated from the plan:**

- Concrete fold types derive `Reflect` and use the workspace-wide `reflect_auto_register` contract; `FoldPlugin` does not add explicit `register_type` calls.
- `bevy_kana` became a normal Hana Valence dependency so private stage-count conversions use `ToF32` under the workspace lint policy.
- Two mechanical fix passes tightened numeric conversion, one `const fn`, a stale `AnchorSystems` doc sentence, and a test registry-guard lifetime.

**Surprises:**

- The full lint workflow still reports three pre-existing `unused-pub` Mend findings in `crates/hana_valence/fixtures.rs`; Phase 1 did not change them.

**Implications for remaining phases:**

- Later fold modules must use `bevy_kana::ToF32` for `usize` stage conversions rather than casts.
- Later concrete reflected fold types should derive `Reflect` and rely on workspace auto-registration; explicit registration remains only for generic or foreign cases.
- Phase 2 extends the existing private `FoldPlayback` and public state accessors instead of replacing the validated state model.

#### Phase 1 Review

- Phases 2–5 now carry the `ToF32` and concrete-type reflection conventions learned during implementation.
- Phase 4 now orders actuation before consumer `hinge_to_pose` processing and tests same-frame angle visibility.
- Phase 5 now waits for every arrangement member's `MemberIndex` before snapshotting and validating membership.
- Phase 6 now specifies capability-key conflict behavior and same-frame title-chip settling.
- Phases 7–9 now require teaching panels whose content matches the staged runtime behavior.
- No user decision was required.

### Phase 2 — Playback commands and virtual-time advancement  · status: done (checkpoint)

#### Work Order

**Goal:** Make every ready fold sequence step, reverse, and play to its terminal boundary from Hana-owned runtime state.

**Spec:**

- Add focused workspace/member `bevy_time` dependencies. Use `Time<Virtual>` and `FoldSequence::step_seconds`; playback starts idle and has no autoplay or pause state.
- Keep the operation separate from its Bevy transport: public `FoldCommand` is the step/play payload, while public `FoldCommandEvent` is the entity-targeted envelope. Name the target field `sequence_entity` so its `Entity` type is explicit:

  ```rust
  #[derive(Clone, Copy, Debug, Reflect)]
  pub enum FoldCommand {
      Step(FoldDirection),
      Play,
  }

  #[derive(EntityEvent, Clone, Copy, Debug)]
  pub struct FoldCommandEvent {
      #[event_target]
      sequence_entity: Entity,
      pub command: FoldCommand,
  }

  commands.trigger(FoldCommandEvent::new(
      sequence_entity,
      FoldCommand::Step(FoldDirection::Folding),
  ));
  ```

  Provide `FoldCommandEvent::new(sequence_entity, command)` and a read-only `sequence_entity()` accessor. The event is entity-targeted through its `#[event_target]` field even when sent through `Commands::trigger`; no extension trait is needed. Invalid/not-ready/zero-stage targets remain idle and diagnose only according to the Phase 1 diagnostic policy.
- Implement the command observer with these transitions:
  - A same-direction `Step(Folding)` while `FoldMotion::Step` sets `min(target + 1, stages)`.
  - A same-direction `Step(Unfolding)` while `FoldMotion::Step` sets `target.saturating_sub(1)`.
  - A step while idle, playing, or changing direction targets the adjacent boundary from continuous position: folding uses `floor(position) + 1`; unfolding uses `ceil(position) - 1`; clamp both to `0..=stages`.
  - `Play` keeps the remembered direction and targets `stages` when folding or zero when unfolding.
  - Any step records its requested direction. At a matching terminal boundary, step/play is a no-op. A second same-direction `Play` while already playing is a no-op.
  - Only a same-direction command during `FoldMotion::Step` extends an existing queued target. Reversal from queued steps or play-to-terminal must move immediately toward the adjacent boundary from current position and must not snap.
- Advance `position` toward the integer target at one stage per `step_seconds`, clamp exactly at the target, and set motion to `Idle` when settled. Reaching an intermediate boundary does not change remembered direction.
- Add `FoldSequenceState::fraction(stage)` using `(position - stage as f32).clamp(0.0, 1.0)`, then apply the authored `EaseFunction`. Members sharing a stage receive the same fraction. This is an accessor; do not write a per-member fraction component each frame.
- Register the command observer and advance system through `FoldPlugin`. `FoldSystems::Advance` runs inside `AnchorSystems::AnimatePose` and before the already-defined `FoldSystems::Actuate`. Do not register the anchor pipeline.
- Exhaustively test integer/fractional steps in both directions, queued same-direction steps, direction reversal from step and play, reversal at exact boundaries, terminal no-ops, remembered play direction, grouped fractions, easing, empty/not-ready sequences, and large deltas that would cross the target.

**Files:**
- `Cargo.toml` — add the focused workspace `bevy_time` dependency.
- `crates/hana_valence/Cargo.toml` — inherit `bevy_time`.
- `crates/hana_valence/src/fold/mod.rs` — register command/advance behavior and ordering.
- `crates/hana_valence/src/fold/playback.rs` — implement commands, transitions, advancement, accessors, and tests.
- `crates/hana_valence/src/fold/sequence.rs` — expose read-only fraction/playback accessors without mutable fields.
- `crates/hana_valence/src/pose.rs` — read-only ordering contract reference.

**Constraints from prior phases:** Phase 1 provides `FoldPlugin`, `FoldSystems`, `FoldSequence`, validated `FoldSequenceState`, the immutable membership relationship, private playback fields, and the rule that only ready sequences move. Preserve its initialization/membership mutation behavior. Use the existing normal `bevy_kana` dependency and `ToF32` for stage conversions. Concrete reflected types derive `Reflect` and rely on workspace auto-registration; do not add explicit registrations.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo nextest run -p hana_valence --all-features` passes all named transition/reversal/fraction tests; `cargo check --workspace --all-targets --all-features` is green; no per-member fraction component or autoplay/pause state exists.

#### Retrospective

**What worked:**

- Phase 1's private `FoldPlayback` and validated `FoldSequenceState` extended directly into commands, continuous advancement, and eased per-stage fractions without adding per-member runtime state.
- The existing set chain already placed validation before `FoldSystems::Advance` and advancement before `FoldSystems::Actuate`; Phase 2 only had to install the advance system in the declared set.
- The transition matrix is covered by 57 passing Hana Valence tests, including fractional reversal, queued steps, exact-boundary behavior, terminal no-ops, large deltas, and grouped easing.

**What deviated from the plan:**

- Bevy 0.19 cannot derive `EntityEvent` for the originally planned payload enum. The approved API now separates public `FoldCommand` from public `FoldCommandEvent`, whose private `sequence_entity: Entity` field supplies the event target.
- Callers use `commands.trigger(FoldCommandEvent::new(sequence_entity, command))`; the event remains entity-targeted without an extension trait or private transport wrapper.
- `crates/hana_valence/src/pose.rs` required no edit because its existing `AnchorSystems::AnimatePose` contract already supports the fold set ordering.

**Surprises:**

- The first delegated verification was interrupted by model capacity after its workspace check passed. An xhigh escalation audit completed format, all tests, workspace check, and diff checks without changing the implementation.

**Implications for remaining phases:**

- Later controls and examples must emit `FoldCommandEvent` rather than treating `FoldCommand` itself as the Bevy event.
- Later actuation reads the already-eased `FoldSequenceState::fraction(FoldStage)` and must not duplicate easing or timing.
- Motion and direction accessors are now the sole source for Fairy Dust chip activation and remembered-direction presentation.

#### Phase 2 Review

- Independent and main-agent reviews approved the command API, state-machine behavior, scheduling, tests, and scope without implementation findings.
- Phases 4–5 now require focused `FoldPlugin` test apps to provide `Time<Virtual>` explicitly.
- Phase 5 now prevents pending arrangement requests from accepting an empty first revision and covers folded initialization plus resnapshot state retention.
- Phase 8 now schedules queued algorithm-profile activation between advancement and actuation on the exact zero-settle frame.
- The full lint workflow applied six approved Mend fixes, passed the rule-by-rule style review, and completed with clean Mend, Clippy, rustdoc, and nightly formatting.
- No user decision remains.

### Phase 3 — Frame-correct physical hinge pivots  · status: done (checkpoint)

#### Work Order

**Goal:** Let any anchored hinge rotate around an authored external pivot line while preserving current behavior when no pivot is present.

**Spec:**

- Add the optional public component in the existing hinge domain:

  ```rust
  pub struct HingePivot {
      pub offset: Vec3,
      pub reference_angle: f32,
  }
  ```

  `offset` is the pivot-line offset from the source anchor, expressed in that anchor's tangent frame at `reference_angle`. `reference_angle` is the hinge angle at which pivot translation is zero.
- Extend `hinge_to_pose` so `HingePivot` computes `AnchorPose::translation` while `Hinge` continues to compute rotation. Resolve the edge axis and pivot offset into the same source-anchor tangent frame using `AnchoredTo::source_anchor` and its `AnchorPoint::frame`. For `delta = hinge.angle - pivot.reference_angle`, hold the pivot invariant with:

  ```rust
  let translation =
      pivot.offset - Quat::from_axis_angle(axis_in_tangent_frame, delta) * pivot.offset;
  ```

  Do not mix the child-geometry edge axis with a resolver-frame offset. If `HingePivot` is present but the required `AnchoredTo`, source anchor, or compatible frame data is missing, diagnose and skip that entity's pose write instead of producing an incorrect transform.
- When `HingePivot` is absent, `hinge_to_pose` retains its exact current rotation behavior and writes `AnchorPose::translation = Vec3::ZERO`. Preserve existing degenerate-edge and same-frame overwrite diagnostics.
- Keep authored static spacing in `AnchoredTo::offset`. The resolver already composes it with `AnchorPose::translation` as `offset + pose.translation`; do not use `ResolvedAnchorOffset` for the pivot.
- Add tests for zero pivot, an external pivot at intermediate and endpoint angles, nonzero `reference_angle`, a non-identity source `AnchorPoint::frame`, endpoint reversal, missing source relationship/data, degenerate axes, finite transforms, and composition with nonzero authored `AnchoredTo::offset`. The non-identity-frame test must verify the pivot's world position stays invariant across several angles.

**Files:**
- `crates/hana_valence/src/hinge.rs` — add `HingePivot`, frame conversion, pose translation, diagnostics, and inline tests.
- `crates/hana_valence/src/lib.rs` — re-export `HingePivot` and update hinge documentation.
- `crates/hana_valence/src/geometry.rs` — read source-frame and edge-axis contracts; edit only if a small reusable conversion API is required.
- `crates/hana_valence/src/relation.rs` — read `AnchoredTo::source_anchor`/offset contracts; edit only for an accessor required by the public pivot implementation.
- `crates/hana_valence/src/resolve.rs` — add offset-plus-pivot composition tests.
- `crates/hana_valence/fixtures.rs` — edit only if the non-identity-frame fixture cannot remain local to the test.

**Constraints from prior phases:** Phases 1–2 provide fold state but do not own physical attachment. `AnchoredTo` remains placement-only. `FoldPlugin` must not register `hinge_to_pose` or `resolve_anchors`. Use `ToF32` for stage conversions and derive `Reflect` without explicit registration for concrete types.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo nextest run -p hana_valence --all-features` passes all named pivot/frame/offset tests; existing no-pivot hinge tests remain unchanged in behavior; `cargo check --workspace --all-targets --all-features` is green and no transform becomes non-finite.

#### Retrospective

**What worked:**

- `hinge_to_pose` remained the single owner of hinge-driven `AnchorPose`: the optional pivot branch adds translation without introducing another compensation system or `ResolvedAnchorOffset` writer.
- Converting the child-local edge axis through `source_frame.inverse()` places rotation and pivot compensation in the same source-anchor tangent coordinates used by the resolver.
- The non-identity-frame test identifies one fixed source-local pivot point and verifies its world position across four angles, independently exercising the resolver rather than only comparing the compensation formula.
- The Hana Valence suite now has 66 passing tests, including missing relationship/geometry/anchor data, invalid frames, degenerate axes, endpoint reversal, nonzero reference angle, finite-output guards, and authored-offset composition.

**What deviated from the plan:**

- `hinge_to_pose` now queries source geometry optionally so a pivoted entity with missing geometry can emit the pivot-specific diagnostic before skipping; no-pivot entities still retain their prior skip behavior.
- Existing crate and tween documentation was updated because `hinge_to_pose` no longer always resets translation to zero when a pivot is present.
- No geometry or relationship API change was needed; the existing public `AnchoredTo::source_anchor` field and `AnchorPoint::rotation()` supplied the required frame data.

**Surprises:**

- `reference_angle` affects only the compensation delta. `AnchorPose::rotation` must continue to use the absolute hinge angle so the resolver seats the source tangent frame correctly.

**Implications for remaining phases:**

- Example pivot offsets must be authored in the source-anchor tangent coordinates of the reference configuration, never copied from an unconverted child-local axis calculation.
- `FoldAngles::unfolded` should normally be the corresponding `HingePivot::reference_angle`; changing an algorithm profile must update both values together.
- Example-local `ResolvedAnchorOffset` or translation compensation is now duplicate behavior and must be removed during migration.

#### Phase 3 Review

- Independent and main-agent reviews approved the frame conversion, compensation math, failure behavior, resolver composition, and world-space invariant test without implementation findings.
- Phase 4 now writes absolute hinge endpoints, rejects non-finite authored endpoints before they reach `Hinge`, and tests a nonzero unfolded/reference endpoint.
- Phase 7 now pins the staggered chain to zero-angle reference configurations and source-anchor tangent offsets.
- Phase 8 now authors triangle endpoints as `PI + signed_lean` and atomically updates the complete angle/pivot profile before actuation.
- Phase 9 now explicitly uses absolute box angles and the intentional no-pivot path for zero-thickness faces.
- The full lint workflow completed with clean Mend, style review, Clippy, rustdoc, and nightly formatting. No user decision remains.

### Phase 4 — Fold-angle hinge actuation  · status: done (checkpoint)

#### Work Order

**Goal:** Map each fold member's eased stage fraction to its hinge endpoints with one unambiguous writer of `Hinge::angle`.

**Spec:**

- Create `fold/hinge.rs` with the public component:

  ```rust
  pub struct FoldAngles {
      pub unfolded: f32,
      pub folded: f32,
  }
  ```

- Implement the built-in adapter: for every entity carrying `FoldMember`, `FoldAngles`, and `Hinge`, follow the relationship to ready `FoldSequenceState`, read the eased `fraction(member.stage)`, and write the absolute hinge angle `unfolded + fraction * (folded - unfolded)`. Never subtract `HingePivot::reference_angle`; Phase 3 uses that value only for the compensation delta. Missing/not-ready sequence state leaves the existing angle unchanged and follows the sequence diagnostic policy. Non-finite `FoldAngles` endpoints diagnose and also leave the existing `Hinge::angle` unchanged rather than relying on Phase 3's downstream finite-pose guard.
- `FoldAngles` is the explicit ownership marker for `Hinge::angle`. Change `drive_arrangement_hinges` to exclude entities carrying `FoldAngles`; removing `FoldAngles` returns ownership to the arrangement driver. Do not rely on registration order to resolve two writers.
- Register actuation in `FoldSystems::Actuate`, after `FoldSystems::Advance`, inside `AnchorSystems::AnimatePose`. Configure `FoldSystems::Actuate.before(crate::hinge_to_pose)` so a consumer-installed `hinge_to_pose` observes the current frame's angle; `FoldPlugin` still must not register that system or any other anchor-pipeline system.
- Keep `FoldAngles` independent of algorithm policy: it stores absolute endpoints only. `HingePivot.reference_angle` normally equals `FoldAngles::unfolded`, but callers may supply another reference. Do not infer signs or pivot offsets from arrangement order.
- Test unfolded/folded endpoints, partial easing, grouped members receiving identical fractions, reverse playback, absent/not-ready state, non-finite endpoints preserving the previous angle, `FoldAngles` ownership exclusion from the arrangement driver, ownership returning after removal, and scheduling before `hinge_to_pose` in an integration schedule. The schedule test must prove `hinge_to_pose` observes the current frame's angle rather than a one-frame-old value. Add a nonzero unfolded/reference-angle case proving that actuation writes the absolute pose rotation while pivot compensation is zero at that endpoint.

**Files:**
- `crates/hana_valence/src/fold/hinge.rs` — create `FoldAngles`, adapter, and tests.
- `crates/hana_valence/src/fold/mod.rs` — export/register the adapter in `FoldSystems::Actuate`.
- `crates/hana_valence/src/lib.rs` — re-export `FoldAngles` and the adapter surface.
- `crates/hana_valence/src/arrange.rs` — exclude `FoldAngles` from arrangement hinge driving and extend inline tests.
- `crates/hana_valence/tests/arrangements.rs` — add staged/grouped actuation and ordering coverage.
- `crates/hana_valence/src/hinge.rs` — read-only `Hinge`/`HingePivot` actuation target.

**Constraints from prior phases:** Phase 2 supplies eased `FoldSequenceState::fraction`; Phase 3 supplies optional `HingePivot` translation. The adapter writes only `Hinge::angle`; `hinge_to_pose` remains consumer-registered after actuation. Any focused app installing `FoldPlugin` must install Bevy time support or insert `Time::<Virtual>::default()`; `FoldPlugin` does not own global time initialization. Use `ToF32` for stage conversions and derive `Reflect` without explicit registration for concrete types.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo nextest run -p hana_valence --all-features` passes actuation, grouped-stage, writer-ownership, and schedule-order tests; `cargo check --workspace --all-targets --all-features` is green.

#### Retrospective

**What worked:**

- `FoldAngles` now gives fold actuation explicit ownership of `Hinge::angle`, while removing it restores arrangement driving.
- Absolute endpoint interpolation runs after playback advancement and before `hinge_to_pose`, so physical poses use the current frame's angle.

**What deviated from the plan:**

- `FoldAngleDiagnostics` and its public reason/entry types were added so rejected writes remain inspectable rather than warning only.
- The first blind review required an arrangement-level test; the follow-up added real `FoldPlugin` scheduling, grouped stages, and resolved physical endpoint assertions.

**Surprises:**

- Finite authored endpoints can still overflow during interpolation, so actuation diagnoses `NonFiniteInterpolation` and preserves the prior hinge angle.

**Implications for remaining phases:**

- Authoring creates membership only; examples must attach absolute `FoldAngles` endpoints explicitly.
- Fairy Dust can derive chip state from Hana playback without mirroring hinge angles or fold fractions.
- Migrated examples inherit same-frame actuation before `hinge_to_pose`; they must not retain local angle writers or pivot compensation.

#### Phase 4 Review

- Phase 5 now validates complete builder input before queuing writes, reconciles removed and reordered arrangement members during resnapshot, and names `sequence.rs` as the validation owner.
- Phase 6 now routes and highlights controls only from Hana sequence playback state, without mirroring angle endpoints, fractions, or actuation diagnostics.
- Phases 7–9 now require normal example interactions to produce no fold-angle actuation diagnostics.
- Independent and main-agent reviews approved the final grouped-stage integration coverage and same-frame physical endpoint behavior. No user decision remains.

### Phase 5 — Explicit and arrangement-based authoring  · status: done (checkpoint)

#### Work Order

**Goal:** Let setup code author grouped fold stages explicitly or snapshot an existing arrangement without manually counting stages.

**Spec:**

- Create `fold/author.rs` with `FoldSequenceBuilder`. Preserve the conceptual call site:

  ```rust
  FoldSequenceBuilder::new(&mut commands, sequence)
      .stage([lid])
      .stage([north, south, east, west])
      .finish();
  ```

  Each `stage` call accumulates the next zero-based `FoldStage` group locally. `finish` validates the complete builder input before queuing any relationship commands, returns a typed error for an empty authored group or duplicate member, and queues no writes on error. On success it inserts/replaces `FoldMember` on every listed entity using ordinary relationship/components. It does not become a second runtime representation and cannot synchronously validate whether deferred `Commands` targets still exist.
- Add `FoldFromArrangement` as a one-time request on a sequence entity, containing the arrangement entity to snapshot. Register a readiness-driven snapshot system in `FoldPlugin` before sequence validation. It reads insertion-ordered `ArrangementMembers::iter()` and succeeds only when every listed entity has `MemberIndex`; otherwise it retains the request and retries on a later update. Once ready, enumerate fresh zero-based stages and insert `FoldMember` relationships. Remove the request only after relationship insertion succeeds.
- A new sequence carrying `FoldFromArrangement` must not validate as an empty sequence while the request is pending: exclude pending requests from sequence validation, then order request removal and deferred relationship insertion so the first accepted revision sees the derived stage count. An already-initialized sequence being resnapshotted retains its prior valid state until the replacement snapshot succeeds. Validation may occur after deferred commands settle in the same update or on the following update, but never between request removal and visible membership.
- Do not copy `MemberIndex` values: they begin at one and may contain gaps after removal. Use `MemberIndex` only to ensure the assignment phase has settled if required by scheduling.
- On missing sequence/arrangement/not-ready arrangement data, emit one diagnostic for that request and retain it for a later retry. Remove `FoldFromArrangement` only after a successful snapshot. Later arrangement edits do not rewrite an active sequence; the caller must insert another request.
- A successful resnapshot reconciles the sequence to the new ordered snapshot: remove `FoldMember` from entities no longer present, replace stages for retained or reordered members, and insert membership for new members. Queue the complete reconciliation before removing `FoldFromArrangement` so validation never observes a partially replaced membership set.
- Both authoring paths create membership/stages only. They never infer `FoldAngles`, endpoint signs, `HingePivot`, or algorithm policy. A triangle accordion uses a constant local fold sign because its rest angle is a half-turn, while a quad accordion alternates local signs; order alone cannot distinguish them.
- Test the grouped lid/walls call, consecutive explicit stages, empty/duplicate errors with no partial membership writes, insertion-order snapshots, fresh zero-based stages despite gapped `MemberIndex`, deferred readiness, retained failed requests, folded initialization deriving its terminal boundary from the completed first snapshot, removal after success, prior-state retention during resnapshot, removal and reordering reconciliation, explicit resnapshot after arrangement mutation, and equivalence of manually/automatically authored membership.

**Files:**
- `crates/hana_valence/src/fold/author.rs` — create builder, one-time request, snapshot system, diagnostics, and tests.
- `crates/hana_valence/src/fold/mod.rs` — export authoring APIs and register the snapshot system in the required order.
- `crates/hana_valence/src/fold/sequence.rs` — gate `validate_fold_sequences` while a new arrangement snapshot is pending, preserving existing valid state during resnapshot.
- `crates/hana_valence/src/lib.rs` — re-export public authoring APIs/errors.
- `crates/hana_valence/src/arrange.rs` — read arrangement ordering/assignment contracts; edit only if a public scheduling label or accessor is required.
- `crates/hana_valence/tests/arrangements.rs` — add grouped-box and arrangement-snapshot integration coverage.

**Constraints from prior phases:** Phase 1 owns relationship validation and derived stage count. Phase 4 owns endpoint actuation. Authoring must produce `FoldMember`/`FoldStage` only and leave endpoint/pivot policy to callers. Any focused app installing `FoldPlugin` must install Bevy time support or insert `Time::<Virtual>::default()`; `FoldPlugin` does not own global time initialization. Use `ToF32` for stage conversions and derive `Reflect` without explicit registration for concrete types.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo nextest run -p hana_valence --all-features` passes all explicit/snapshot/deferred/equivalence tests; the grouped box derives two stages from five members; `cargo check --workspace --all-targets --all-features` is green.

#### Retrospective

**What worked:**

- `FoldSequenceBuilder` validates all grouped input before queuing relationship writes, so empty and duplicate errors are atomic.
- `FoldFromArrangement` applies removal, reordering, insertion, and request cleanup inside one deferred world command before sequence validation.

**What deviated from the plan:**

- Public bounded snapshot diagnostics and a private per-request marker were added to keep retry failures inspectable without repeated warnings.
- The delegated Clippy pass added the required `# Errors` documentation and changed the copyable snapshot request helper to pass by value.

**Surprises:**

- The blind review found that the reflected arrangement request needed `#[entities]` so Bevy remaps its arrangement entity during reflective cloning.

**Implications for remaining phases:**

- Fairy Dust can treat authoring as complete only when `FoldSequenceState::is_ready()` is true; pending snapshots retain prior valid state.
- `triangles` can request arrangement-derived membership without copying `MemberIndex` or inferring its algorithm profile.
- `box` can author its lid and four-wall groups through the explicit builder with no manual stage count.

#### Phase 5 Review

- Phase 6 now leaves controls quietly inactive while no sequence is ready and emits routing diagnostics only for failed input routing or ambiguous ready sequences.
- Phase 8 now verifies its arrangement snapshot is diagnostic-free and ready with one stage per member before folding.
- Phase 9 now handles `FoldSequenceBuilder::finish()` explicitly and verifies successful grouped authoring.
- The remaining phases stay distinct and correctly ordered, with Phases 7–9 still parallel after Phase 6. No user decision remains.

### Phase 6 — Fairy Dust fold controls  · status: done (checkpoint)

#### Work Order

**Goal:** Add one Fairy Dust capability that installs Hana folding and provides the standard fold, unfold, and play controls.

**Spec:**

- Add a direct workspace `hana_valence` dependency to `fairy_dust`. There is no Hana-to-Fairy dependency. Add `fold_controls.rs`, public `FairyDustFoldTarget`, and state-agnostic `SprinkleBuilder<S>::with_fold_controls()`:

  ```rust
  fairy_dust::sprinkle_example()
      .with_fold_controls()
  ```

- `with_fold_controls` idempotently installs Hana's public `FoldPlugin` and Fairy Dust's existing enhanced-input support. It must not register `hinge_to_pose`, arrangement driving, `resolve_anchors`, or any other anchor pipeline system already installed by `bevy_diegetic`.
- Define private BEI input context/actions for one folding step, one unfolding step, and play-to-terminal. Bind bare `Space` only when no modifier is held, `Shift+Space` for unfolding with either Shift key normalized by BEI, and bare `P` for play. Reserve conflicting bare keys through the existing shortcut registry. Do not read `ButtonInput<KeyCode>` directly.
- Register title chips with stable ids and visible labels `Space Fold`, `Shift+Space Unfold`, and `P Play`. Step chips are active only while `FoldMotion::Step` moves in their direction. `P Play` is active only during `FoldMotion::Play`. All are inactive when idle; there is no pause control. Run chip synchronization in `PostUpdate` after `FoldSystems::Advance` and before `refresh_changed_title_bar` so activation clears on the exact frame playback settles.
- Route actions to Hana by triggering `FoldCommandEvent::new(sequence_entity, command)` for the selected sequence. If exactly one ready `FoldSequenceState` exists, route automatically. With multiple ready sequences, exactly one sequence carrying `FairyDustFoldTarget` is required. Passive chip synchronization with no ready sequence leaves controls inactive without diagnosing because arrangement snapshot authoring may still be pending. Emit one stable diagnostic when an input action cannot route because no sequence is ready, or when multiple ready sequences are ambiguous because zero or multiple targets are marked. Fairy Dust reads Hana accessors and never reproduces validation or transition rules.
- Keep title bars controls-only; no instructional prose belongs in the title bar. The capability must coexist with camera/home/help actions and with an example-owned BEI algorithm-toggle action. Extend key reservation so reinstalling the same capability is idempotent while two different capabilities reserving the same bare key are rejected; explicitly cover the existing cube-spin `P` control versus `P Play`.
- Add tests for idempotent plugin/input installation, bare/modifier binding separation, same-capability reservation idempotence, cube-spin `P` versus `P Play` rejection, passive zero-ready inactivity without diagnostics, zero/one/multiple input routing, marker selection, invalid target inactivity, stable failed-input and ambiguous-ready diagnostics, emitted commands, exact-settle-frame step/play chip activation, and coexistence with another BEI action.

**Files:**
- `crates/fairy_dust/Cargo.toml` — add direct `hana_valence` dependency.
- `crates/fairy_dust/src/lib.rs` — declare/export fold controls and marker.
- `crates/fairy_dust/src/fold_controls.rs` — create capability support, actions, routing, diagnostics, chip synchronization, and tests.
- `crates/fairy_dust/src/builder/sprinkle.rs` — add `with_fold_controls`.
- `crates/fairy_dust/src/builder/title_bar.rs` — add forwarding only if required by the chosen call order.
- `crates/fairy_dust/src/constants.rs` — add stable control ids/labels/reservation labels.
- `crates/fairy_dust/src/screen_panels/mod.rs` — use the existing title-control registry; edit only for a minimal public capability hook if required.
- `crates/fairy_dust/src/screen_panels/title_bar.rs` — use existing control activation state; edit only for a missing accessor.
- `crates/fairy_dust/src/shortcuts.rs` — extend key reservation so the same capability can reinstall idempotently while different capabilities conflict, including cube spin versus fold play on `P`.
- `crates/fairy_dust/src/restart.rs` — read-only BEI pattern reference.
- `crates/bevy_diegetic/src/panel/mod.rs` — read-only registration contract; do not edit.

**Constraints from prior phases:** Phases 1–5 provide the public `FoldPlugin`, `FoldCommand`, `FoldCommandEvent`, ready-state accessors, motion/direction accessors, and authoring APIs. Hana owns the state machine; Fairy Dust owns only input, routing, diagnostics, and presentation. Route and highlight solely from ready `FoldSequenceState`, `FoldMotion`, and `FoldDirection`; do not mirror `FoldAngles`, fractions, or `FoldAngleDiagnostics` into Fairy Dust.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo nextest run -p fairy_dust --all-features` passes all named input/routing/chip tests; `cargo nextest run -p hana_valence --all-features` remains green; `cargo check --workspace --all-targets --all-features` is green and the anchor pipeline is registered once.

#### Retrospective

**What worked:**

- `.with_fold_controls()` installs Hana playback and private BEI actions idempotently without registering any anchor-pipeline systems.
- Routing selects only ready sequences, remains quiet while authoring is pending, and drives the three title chips from Hana motion state on the settle frame.

**What deviated from the plan:**

- Shortcut reservations now carry a capability `TypeId`; `camera_home.rs` and `cube_spin.rs` were updated to name their owners so repeat installation is idempotent and cross-capability collisions fail immediately.
- Public bounded routing diagnostics expose the rejected action and ready/marked counts for inspection.

**Surprises:**

- The full Fairy Dust verification required 66 tests; the existing macOS compact-unwind linker warning remained non-fatal.

**Implications for remaining phases:**

- All three examples can remove raw fold input and install the same Space, Shift+Space, and P controls through one builder call.
- Example-owned BEI algorithm controls coexist with folding as long as they reserve different bare keys.
- Title bars need no example-local fold activation state; instructional content remains in screen-space teaching panels.

#### Phase 6 Review

- A new Phase 6.5 prepares the shared Hana example dependency graph before the three example-local migrations run in parallel.
- Phases 7–9 now call `.with_fold_controls()` while they still hold `SprinkleBuilder`, remove conflicting legacy shortcuts and local fold-chip wiring, and verify installation without reservation collisions.
- All three example acceptance gates now require a single routable sequence with no `FoldControlDiagnostic` during normal controls; Phase 8 waits for its arrangement snapshot before the first input.
- No product or architecture decision blocks the parallel example wave.

### Phase 6.5 — Prepare parallel example dependencies  · status: done (checkpoint)

#### Work Order

**Goal:** Make the shared Hana manifest support three isolated, parallel example migrations without requiring any example agent to edit shared files.

**Spec:**

- Remove the `staggered_unfold` `required-features = ["tween"]` example declaration because Phase 7 removes its tween-based implementation and the example must compile without the optional Hana tween feature.
- Remove the direct `bevy_tween` dev-dependency once no Hana example imports it. Preserve Hana's optional normal `bevy_tween` dependency and public `tween` feature; this phase does not remove the library's tween adapters.
- Add direct `bevy_enhanced_input` example/dev access for the Phase 8 algorithm action and enable the `input` feature on Hana's existing `bevy_kana` dependency so the example can use the workspace BEI macros without reaching through Fairy Dust.
- Update `Cargo.lock` mechanically. Do not edit any example source or shared runtime API.

**Files:**

- `crates/hana_valence/Cargo.toml` — prepare the example dependency graph and remove the obsolete example feature gate.
- `Cargo.lock` — record the manifest dependency change.

**Constraints from prior phases:** Phase 6 provides `.with_fold_controls()` through Fairy Dust. Preserve Hana's optional `tween` library feature and all Phase 1–6 runtime APIs. This checkpoint must leave Phases 7–9 isolated to their named example source files.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo check -p hana_valence --examples`; `cargo check -p hana_valence --examples --all-features`; `cargo check --workspace --all-targets --all-features`; `cargo nextest run -p hana_valence --all-features` and `cargo nextest run -p fairy_dust --all-features` remain green.

#### Retrospective

**What worked:**

- The manifest-only checkpoint removed the obsolete example tween gate and direct dev tween dependency while preserving Hana's optional tween library API.
- Direct BEI and dev-only `bevy_kana/input` access now support the triangle example without widening normal Hana consumer features.

**What deviated from the plan:**

- The blind review moved `bevy_kana/input` from the normal dependency to a duplicate dev-dependency so Cargo feature unification affects examples/tests only.

**Implications for remaining phases:**

- Phases 7–9 can now edit only their named example files and run concurrently without manifest conflicts.
- `staggered_unfold` compiles without Hana's optional tween feature; `triangles` can author its own BEI algorithm action directly.

#### Phase 6.5 Review

- Phase 7 now compiles `staggered_unfold` with `--no-default-features` so residual tween imports cannot hide behind the preserved optional library feature.
- Workspace-wide check, nextest, and Clippy run at a root-owned parallel-wave join after all three example checkpoints rather than inside Phase 9.
- Phases 7–9 are isolated to distinct example files and safe to dispatch concurrently. No user decision remains.

### Phase 7 — Migrate `staggered_unfold`  · status: done (checkpoint)

#### Work Order

**Goal:** Make the staggered panel chain an idle, step-controlled, physically consistent accordion using the shared runtime and controls.

**Spec:**

- Confine implementation edits to `staggered_unfold.rs` so this Work Order can run in parallel with Phases 8 and 9. Do not edit shared fixtures or the other examples.
- Replace the local fold step/playback resource, autoplay/pause logic, fold-motion mirror, raw keyboard reads, example-local fold tween registration, duplicate fold scheduling, and `apply_hinge_pivot`/`ResolvedAnchorOffset` compensation with `FoldSequence`, `FoldMember`, `FoldAngles`, `HingePivot`, and `.with_fold_controls()`.
- Call `.with_fold_controls()` before `.with_title_bar(...)` while the chain still holds `SprinkleBuilder`. Remove `.with_shortcut(KeyP, toggle_pause)`, manually declared Space/Shift+Space/P title controls, and all local activation wiring; the shared capability owns those bindings and chips.
- Keep the gold fixed root visually and structurally distinct. It supplies the authored world transform and first `AnchoredTo` relationship but has no `FoldMember` and consumes no stage.
- Give the five moving panels consecutive zero-based stages. Preserve the accordion endpoints with the current edge ordering: panel one stops at a quarter-turn parallel to the mount face; panels two through five use alternating half-turns so they close face-to-face rather than coil inward. Use the current sign sequence as the starting authored values: `[FRAC_PI_2, -PI, PI, -PI, PI]`; if an existing edge endpoint order reverses an axis, adjust that endpoint's authored sign while preserving the stated world-space result.
- Every panel authors `FoldAngles::unfolded = 0.0` and `HingePivot::reference_angle = 0.0`; the listed values are absolute `FoldAngles::folded` endpoints. Author each `HingePivot::offset` in source-anchor tangent coordinates. The current identity-framed quad anchors allow the same vector to place the child-local knuckle axis; if that frame changes, convert the tangent-frame pivot line before positioning the visuals.
- Author pivot placement per joint from the connected solids. Panel-to-panel half-turns use the panel half-thickness required for face contact; do not add rendered cylinder radius to the kinematic offset. Compute the first root joint separately from mount-face clearance and its quarter-turn limit. A panel must never pass through the previous panel or fixed root.
- Render segmented hinge knuckles on the same edge line as each physical pivot. The visuals do not create relationships or stages; the panel spawn helper derives knuckles, `AnchoredTo`, `FoldMember`, `FoldAngles`, and `HingePivot` from one edge definition so their axes cannot diverge.
- Retain `.with_brp_extras()`, `.with_stable_transparency()`, the Fairy Dust orbit camera, camera-control panel, useful authored camera-home target/home proxy, fixed-root label, and screen-space teaching panel. Rewrite the teaching panel so it explains the authored fold stages, fixed root, and physical pivots; remove tween/ping-pong descriptions. The title bar contains only `H Home`, the standard folding controls, and help. The initial home view must show the physical attachment and moving panels clearly.
- The example starts idle. One `Space` advances one panel stage, `Shift+Space` reverses one, and `P` continues to the terminal boundary in the remembered direction.

**Files:**
- `crates/hana_valence/examples/staggered_unfold.rs` — complete migration and presentation/physical endpoint verification.

**Constraints from prior phases:** Phases 1–6.5 are complete and provide the runtime, frame-correct pivot, hinge adapter, explicit authoring, Fairy controls, and a manifest that no longer requires tween support for this example. This phase is independent of Phases 8 and 9 and may run in parallel with them. Do not change shared APIs, manifests, or fixtures in this phase.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo check -p hana_valence --example staggered_unfold --no-default-features`; `cargo check -p hana_valence --example staggered_unfold --all-features`; `cargo nextest run -p hana_valence --all-features`; launch the example and verify fold-control installation without a reservation panic, idle startup, both step directions, remembered-direction play, an invariant visible hinge axis during motion, face-to-face final panel contact, first-panel root clearance, BRP port display, stable transparency, orbit camera, authored home view, teaching-panel text that matches the staged runtime, no `FoldControlDiagnostic` during normal startup and standard fold inputs, and no `FoldAngleDiagnostic` during normal startup, stepping, reversal, or play.

#### Retrospective

**What worked:**

- One `PanelJoint` definition now authors the attachment, fold endpoint, tangent-frame pivot, and segmented knuckle line for each moving panel.
- The fixed gold mount remains outside the five-stage sequence; shared controls start idle and replace all tween, autoplay, pause, and local input behavior.

**What deviated from the plan:**

- BRP runtime inspection verified stage motion, pivot invariance, face spacing, root clearance, and clean diagnostics, but the captured native window was occluded and visually black.

**Implications for remaining phases:**

- Interactive root verification still needs to inspect visible knuckles, home framing, transparency, title text, and teaching-panel legibility.

#### Phase 7 Review

- The parallel-wave join now requires root-visible presentation verification whenever delegated screenshots were unavailable or occluded; BRP state alone does not satisfy visual acceptance.
- Phase 7 introduced no shared change requiring amendments to the already implemented Phase 8 or Phase 9 Work Orders. No user decision remains.

### Phase 8 — Migrate `triangles`  · status: done (checkpoint)

#### Work Order

**Goal:** Make the triangle strip use arrangement-derived stages and shared controls while retaining both fold algorithms without mid-fold jumps.

**Spec:**

- Confine implementation edits to `triangles.rs` so this Work Order can run in parallel with Phases 7 and 9. Do not edit shared fixtures or the other examples.
- Replace `AccordionPlayback`, local playback/motion state, raw fold input, local timing, replay behavior, duplicate fold scheduling, and `apply_crease_gap`/`ResolvedAnchorOffset` compensation with the shared fold runtime. Keep only example-owned algorithm selection state.
- Call `.with_fold_controls()` before `.with_title_bar(...)` while the chain still holds `SprinkleBuilder`. Remove the obsolete `R` replay shortcut, manually declared Space/Shift+Space/P controls, and local fold-chip activation. Replace only the existing `T` algorithm toggle with its example-owned BEI action and retain only its segmented algorithm control.
- Place `FoldFromArrangement` on the sequence after the arrangement is authored. It snapshots `ArrangementMembers` after member indices settle and assigns one zero-based stage per moving crease; the arrangement root is not a moving fold member.
- Author `FoldAngles` and `HingePivot` in the example's algorithm profile. For every triangle member, use the absolute endpoint `FoldAngles::unfolded = HingePivot::reference_angle = PI` and compute `FoldAngles::folded = PI + signed_lean`; never store `signed_lean` alone as the folded endpoint. The accordion uses a constant local fold sign because each member already has a half-turn rest orientation; the wrap algorithm uses the existing alternate policy. Express every pivot offset in source-anchor tangent coordinates. Do not ask `FoldFromArrangement` to infer signs, endpoints, or pivot gaps.
- Keep the accordion/wrap toggle as an example-owned BEI action, independent of Space/Shift+Space/P. Do not read `ButtonInput<KeyCode>` directly. Track selected and active algorithm profiles separately: the selection/title chip changes immediately, but when `FoldSequenceState::position()` is nonzero, queue endpoint/pivot changes until playback reaches fully unfolded position zero. Run queued-profile activation in `PostUpdate` after `FoldSystems::Advance` and before `FoldSystems::Actuate`, using Phase 2's exact `0.0` settle boundary as the deterministic activation point. Applying an active profile updates `FoldAngles` and the complete `HingePivot` atomically before actuation and must not change playback position, target, direction, stage membership, or grouping.
- Route the standard three controls through `.with_fold_controls()`. The example starts idle; one step moves one crease; `P` continues in the remembered direction.
- Retain `.with_brp_extras()`, `.with_stable_transparency()`, the Fairy Dust orbit camera, camera-control panel, useful authored camera-home target, and controls-only title bar. Add a concise screen-space teaching panel explaining arrangement-derived stages and queued algorithm-profile activation.

**Files:**
- `crates/hana_valence/examples/triangles.rs` — complete migration, BEI algorithm selection, queued profile activation, and presentation verification.

**Constraints from prior phases:** Phases 1–6.5 are complete and provide arrangement snapshot authoring, runtime accessors, frame-correct pivots, hinge actuation, Fairy controls, and direct example access to BEI plus `bevy_kana` input macros. This phase is independent of Phases 7 and 9 and may run in parallel with them. Do not change shared APIs, manifests, or fixtures in this phase.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo check -p hana_valence --example triangles --all-features`; `cargo nextest run -p hana_valence --all-features`; launch the example and verify fold-control installation without a reservation panic, idle startup, no `FoldSnapshotDiagnostic` during normal setup, a ready sequence with one zero-based stage per arrangement member before the first input, no `FoldControlDiagnostic` during standard inputs after readiness, one-crease steps in both directions, remembered-direction play, both algorithms, zero pivot compensation at the absolute `PI` reference endpoint, finite invariant pivots for both profiles, no transform jump when selection changes away from zero, queued activation on the same frame playback settles at zero, unchanged position/target/direction/membership/grouping across activation, stable transparency, BRP port display, orbit camera, home view, teaching-panel content, and no `FoldAngleDiagnostic` during normal startup, stepping, reversal, play, or algorithm switching.

#### Retrospective

**What worked:**

- Arrangement snapshot authoring supplies one stage per moving triangle while absolute PI-based angle/pivot profiles remain example-owned.
- The BEI T action updates selected state immediately and activates the complete profile only at ready position zero between fold advancement and actuation.

**What deviated from the plan:**

- Static checks and startup diagnostics passed, but interactive motion, both physical profiles, queued switching continuity, and presentation remain root-visible verification work.

**Implications for remaining phases:**

- Phase 9 needs no profile-switching behavior; the join gate must visually verify both triangle algorithms and same-frame queued activation.

#### Phase 8 Review

- The parallel-wave join now explicitly exercises Accordion and Wrap, step/reverse/play, and a mid-fold T selection that activates only at fully unfolded position without a visible jump or playback-state change.
- Phase 8 introduced no shared change requiring a Phase 9 amendment. No user decision remains.

### Phase 9 — Migrate `box`  · status: done (checkpoint)

#### Work Order

**Goal:** Make the box net demonstrate explicit grouped stages through the shared runtime and controls.

**Spec:**

- Confine implementation edits to `box.rs` so this Work Order can run in parallel with Phases 7 and 8. Do not edit shared fixtures or the other examples.
- Remove example-local `FoldPhase`, `FoldTarget`, `FoldPlayback`, replay input, timing, and hinge driver. Install `.with_fold_controls()` and author `FoldAngles` on the moving faces.
- Call `.with_fold_controls()` before `.with_title_bar(...)` while the chain still holds `SprinkleBuilder`. Remove the obsolete `R Replay` shortcut, manually declared Space/Shift+Space/P controls, and all local fold-chip activation wiring.
- Spawn one unfolded `FoldSequence`. Use `FoldSequenceBuilder` to author exactly two stages: stage zero contains only the lid; stage one contains north, south, east, and west. Handle `FoldSequenceBuilder::finish()` and fail setup with a clear error if this statically authored grouping is rejected; do not discard the `Result`. The center face is the fixed root, carries no `FoldMember`, and consumes no stage. Five moving faces must derive a stage count of two.
- Preserve the physical topology: the lid remains anchored to north's free outer edge, so folding stage zero raises the lid first and stage one raises all four walls while carrying the lid through the anchor chain. Do not derive stages from descendants or member count.
- Author absolute `FoldAngles { unfolded: 0.0, folded: BOX_FOLD_ANGLE }` on each moving face and omit `HingePivot` for the current zero-thickness planar faces; `hinge_to_pose` therefore preserves zero `AnchorPose::translation`.
- The example starts idle. `Space` advances lid then walls, `Shift+Space` reverses walls then lid, and `P` continues to the terminal boundary in the remembered direction.
- Retain `.with_brp_extras()`, the Fairy Dust orbit camera, camera-control panel, useful authored camera-home target, ground plane/studio lighting, and controls-only title bar. Add a concise screen-space teaching panel explaining the lid stage, grouped wall stage, and fixed root. Preserve any currently configured transparency capability; do not add unrelated presentation changes.

**Files:**
- `crates/hana_valence/examples/box.rs` — complete grouped-stage migration and presentation verification.

**Constraints from prior phases:** Phases 1–6.5 are complete and provide explicit grouped authoring, runtime, hinge actuation, Fairy controls, and a prepared shared example dependency graph. This phase is independent of Phases 7 and 8 and may run in parallel with them. Do not change shared APIs, manifests, or fixtures in this phase.

**Acceptance gate:** `cargo +nightly fmt --all -- --check`; `cargo check -p hana_valence --example box --all-features`; `cargo nextest run -p hana_valence --all-features`; launch the example and verify fold-control installation without a reservation panic, successful builder completion, idle startup, lid-only stage zero, four-wall stage one, reverse order, remembered-direction play, fixed center exclusion, BRP port display, orbit camera, home view, teaching-panel content, no `FoldControlDiagnostic` during normal startup and standard fold inputs, and no `FoldAngleDiagnostic` during normal startup, stepping, reversal, or play.

#### Retrospective

**What worked:**

- `FoldSequenceBuilder` expresses the physical topology directly: lid-only stage zero, four walls grouped at stage one, and the fixed center outside membership.
- Absolute planar endpoints use the intentional no-pivot path, and shared controls replace all replay timing and local hinge driving.

**What deviated from the plan:**

- Static verification passed, but the delegated environment reported no GPU; physical sequencing, orbit/home behavior, BRP title, and panel presentation remain root-visible checks.

**Implications for remaining phases:**

- All implementation phases are complete; only the parallel-wave join validation and interactive acceptance remain.

#### Phase 9 Review

- The parallel-wave join now explicitly exercises box idle state, lid-only then grouped-wall motion, reverse order, remembered-direction play, fixed-center behavior, and clean fold-control/fold-angle diagnostics.
- Existing static, lint, presentation, triangle, and box join checks cover the remaining evidence. No user decision remains.

## Parallel-wave join gate

After Phases 7–9 are individually reviewed and checkpointed, the root orchestrator runs `cargo check --workspace --all-targets --all-features`, `cargo nextest run --workspace --all-features`, and the full local `clippy` workflow with `auto-proceed`. The root also visibly verifies each example's presentation whenever delegated window evidence was unavailable or occluded: hinge/panel geometry, home framing, transparency, title controls, teaching-panel legibility, orbit interaction, and BRP port text. BRP state inspection alone does not satisfy this visual gate. Any cross-example, shared, or presentation regression is fixed and re-reviewed before the plan is complete.

For `triangles`, the root interaction pass must exercise Accordion and Wrap, one-step folding and unfolding, remembered-direction play, and a mid-fold T selection that stays queued until fully unfolded. Activation at position zero must not visibly jump or change playback position, target, direction, membership, or grouping.

For `box`, the root interaction pass must verify idle startup, a first Space that moves only the lid, a second Space that moves all four walls while carrying the lid, Shift+Space reversing walls before lid, and P following the remembered direction. The center must remain fixed and normal interaction must leave fold-control and fold-angle diagnostics empty.
