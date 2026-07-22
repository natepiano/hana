# Hana Valence arrangements and folding

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Build reusable arrangement providers, transient fold recipes, retained fold and camera sequences, shared external progress, and Hana transport integration.

> **As-built disposition: amend** — update `docs/hana_valence/as-built/anchoring-and-arrangements.md` and `docs/hana_valence/as-built/folding.md` after implementation.

## Delegation Context

- **Project:** `/Users/natemccoy/rust/bevy_hana` workspace — implement the `hana_valence` arrangement/folding redesign, shared `bevy_kana` playback primitives, `bevy_lagrange` retained camera playback, and sibling Hana transport integration.
- **Stack:** Rust 1.97.0, edition 2024; Bevy 0.19.0 with relationships, entity events, scenes, BSN, and `reflect_auto_register`; `bevy_tween` 0.13.0; `bevy_enhanced_input` 0.26.0; `thiserror` 2.0.18; `hana_valence` 0.1.0; `bevy_kana` 0.2.0-dev; `bevy_lagrange` 0.3.0-dev; sibling Hana workspace 0.1.0.
- **Layout:**
  - `crates/bevy_kana/` — semantic math, cascade, shared playback, and external-control primitives.
  - `crates/hana_valence/` — anchor geometry, arrangements, recipes, hinges, fold sequences, tests, and examples.
  - `crates/bevy_lagrange/` — retained camera sequences, native/external playback, events, and camera-input policy.
  - `crates/hana_diegetic/`, `crates/fairy_dust/` — workspace consumers and the panel-anchoring demonstration.
  - `/Users/natemccoy/rust/hana/crates/hana_animation/` — transport clock and progress bindings.
  - `/Users/natemccoy/rust/hana/crates/hana/` — Hana binary transport UI and camera consumers.
- **Key files:**
  - `Cargo.toml`, `Cargo.lock` — workspace dependency versions, features, and resolution.
  - `crates/bevy_kana/Cargo.toml` — math, playback, and optional tween features.
  - `crates/bevy_kana/src/{lib,prelude}.rs` — shared exports.
  - `crates/bevy_kana/src/cascade.rs` — existing cascade contract reused by fold timing.
  - `crates/bevy_kana/src/math/mod.rs`, `crates/bevy_kana/src/math/space/mod.rs` — semantic math module wiring.
  - `crates/bevy_kana/src/math/space/orientation.rs` — valid-by-construction `Orientation`.
  - `crates/bevy_kana/src/math/space/kinematics/*.rs` — `Position`, `Displacement`, conversions, and shared newtype patterns.
  - `crates/bevy_kana/examples/*.rs`, `crates/bevy_kana/{README.md,CHANGELOG.md}` — shared examples and migration documentation.
  - `crates/hana_valence/Cargo.toml` — scene, `bevy_kana`, `thiserror`, and feature dependencies.
  - `crates/hana_valence/src/lib.rs` — plugin/module wiring and public exports.
  - `crates/hana_valence/src/geometry.rs` — anchor sites, frames, edges, and resolved geometry.
  - `crates/hana_valence/src/relation.rs` — anchor relationships and semantic offsets.
  - `crates/hana_valence/src/{attachment,resolve,pose,hinge}.rs` — dependency resolution, pose, hinge evaluation, and transform output.
  - `crates/hana_valence/src/arrange.rs` — current arrangement implementation replaced by provider, plan, and materialization APIs.
  - `crates/hana_valence/src/fold/{mod,author,hinge,playback,sequence}.rs` — recipes, stages, targets, playback, commands, events, and removals.
  - `crates/hana_valence/src/tween.rs` — obsolete hinge/pose lenses removed when progress tweening moves to `bevy_kana`.
  - `crates/hana_valence/{fixtures.rs,tests/arrangements.rs}` — fixtures and public arrangement coverage.
  - `crates/hana_valence/examples/{box,staggered_unfold,triangles}.rs` — direct, staged, and provider examples.
  - `crates/hana_valence/{README.md,CHANGELOG.md}` — public API and breaking migration documentation.
  - `crates/hana_diegetic/src/panel/{anchoring,mod}.rs`, `crates/hana_diegetic/examples/panel_anchoring/*.rs` — panel arrangement consumer and demonstration.
  - `crates/hana_diegetic/examples/{aa_text,units}.rs` — camera-animation consumers.
  - `crates/fairy_dust/src/{camera_home,lib}.rs`, `crates/fairy_dust/src/camera_control_panel/preset_switch.rs` — camera command consumers.
  - `crates/bevy_lagrange/Cargo.toml` — shared playback dependency and features.
  - `crates/bevy_lagrange/src/{lib,system_sets}.rs` — exports and cross-crate ordering.
  - `crates/bevy_lagrange/src/animation/{mod,constants,events,lifecycle,queue}.rs` — retained sequence, commands, sampling, conflicts, events, and queue removal.
  - `crates/bevy_lagrange/src/{camera_home,camera_kind}.rs` — camera request integration.
  - `crates/bevy_lagrange/src/{orbit_cam,free_cam}/controller.rs` — deterministic sequence output and damping bypass.
  - `crates/bevy_lagrange/src/input/{lifecycle,routing/snapshot}.rs` — native/external input ownership.
  - `crates/bevy_lagrange/src/fit/mod.rs`, `crates/bevy_lagrange/src/fit/triggers/**/*.rs` — higher-level animation-source migration.
  - `crates/bevy_lagrange/examples/animation.rs`, `crates/bevy_lagrange/examples/showcase/*.rs`, `crates/bevy_lagrange/examples/{swapped_axis,zoom_to_fit}.rs` — command/event migration and runnable coverage.
  - `crates/bevy_lagrange/{README.md,CHANGELOG.md}` — retained-camera API and breaking migration documentation.
  - `/Users/natemccoy/rust/hana/Cargo.toml`, `/Users/natemccoy/rust/hana/Cargo.lock` — compatible `bevy_kana`/`bevy_lagrange` revisions.
  - `/Users/natemccoy/rust/hana/crates/hana_animation/Cargo.toml` — production `bevy_kana` dependency.
  - `/Users/natemccoy/rust/hana/crates/hana_animation/src/{lib,plugin,transport}.rs` — transport API, plugin wiring, direction, rate, and seek behavior.
  - `/Users/natemccoy/rust/hana/crates/hana_animation/src/binding.rs` — planned transport binding implementation.
  - `/Users/natemccoy/rust/hana/crates/hana/Cargo.toml` — Hana integration dependencies.
  - `/Users/natemccoy/rust/hana/crates/hana/src/{main,transport}.rs` — plugin composition and scrubber/transport integration.
  - `/Users/natemccoy/rust/hana/crates/hana/src/input/{mod,global_shortcuts}.rs` — explicit transport play/pause controls.
  - `/Users/natemccoy/rust/hana/crates/hana/src/camera/{editor_camera,flyover}.rs` — camera-sequence consumers.
- **Build:** `cd /Users/natemccoy/rust/bevy_hana && cargo check --workspace --all-targets --all-features`; integration: `cd /Users/natemccoy/rust/hana && cargo check -p hana_animation -p hana --all-targets --all-features`.
- **Test:** `cd /Users/natemccoy/rust/bevy_hana && cargo nextest run --workspace --all-features`; integration: `cd /Users/natemccoy/rust/hana && cargo nextest run -p hana_animation -p hana --all-features`.
- **Lint:** Full local `clippy` skill from each modified repository; delegated runs use `clippy auto-proceed`.
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_hana`; sibling integration: `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/hana`.
- **Invariants:**
  - Use `cargo +nightly fmt --all`, never plain `cargo fmt`; never add an `#[allow]` without user review; non-generic reflected types rely on `reflect_auto_register`.
  - `Arrangement` is a separate controller. `Member` / `Members` is the sole arrangement-membership relationship, and `AnchoredTo` / `AnchoredHere` is the sole physical attachment relationship. Allow zero members, branches, disconnected forests, repeated targets, and multiple roots; reject physical cycles.
  - Provider members are unique and deterministic. Build and validate one authoritative typed member association and `ArrangementPlan<S>` before invoking member scenes; erase the selection type only during ECS materialization; clean up reserved entities after synchronous failure.
  - `ArrangementPlugin` owns construction, baseline `Hinge` evaluation, and scene support. The application supplies `AssetPlugin`; `ArrangementPlugin` adds `ScenePlugin` when absent. Apply scenes onto reserved member roots with `queue_apply_scene()`.
  - Construction values use private fields and opaque reflection where structural reflection could bypass invariants. Public semantic `Position`, `Displacement`, `Orientation`, and `Angle` usage is intentional; `Orientation` normalizes fallible finite input; `Angle` preserves finite multi-turn values.
  - Every provider `Connection` materializes a baseline `Hinge` with equal base/folded angles and zero pivot. `Hinge` requires `AnchorPose`, stores no mutable current angle, and uses source-anchor-frame pivot calibration. `ArrangementPlugin` is the sole registrar of `hinge_to_pose()`.
  - Runtime ECS mutation belongs to the application: do not add arrangement-wide stale-state scans, rollback, retry validators, or retained reconciliation state. Normal queries skip unavailable state; pre-write selection, capability, recipe, and assignment failures remain no-write, while later missing application-owned entities are best effort.
  - `FoldGroup` and `FoldGroups` are nonempty and internally unique; overlap across groups and repeated members across stages are valid. `FoldTarget` is finite `0.0..=1.0`; repeated-member tracks are precomputed and history-independent for forward, backward, and arbitrary sampling.
  - Recipes are transient extension values. Validate exactly one assignment per selected retained connection before beginning hinge writes. Accordion alternates group direction, Coil coalesces repeated connections, and Wrap uses invariant-preserving `WindingClearance`.
  - `FoldSequence` owns stages, targets, and whole-value timing cascades. Authored `Duration` values are unrestricted; use exact `SequenceTime` boundary accounting and deterministic stage/member/endpoint event ordering without per-update transition allocation.
  - `SequencePlayback` is shared state embedded by domain runtime components. Native and external control are mutually exclusive; `ExternalAnimationControl` remains authoritative when a sample is temporarily absent; rejected fold/camera commands emit targeted typed rejection events.
  - External producers write progress before domain evaluation in `Update`; `FoldSystems::Advance` runs in that evaluation phase. Fold pose output follows, then anchor resolution and transform propagation in `PostUpdate`. Reuse resolver scratch allocations and keep the truly idle path write-free.
  - `bevy_lagrange` retains valid-by-construction `CameraMove` authoring, samples captured immutable endpoints, writes current and target controller values together, bypasses damping, clears produced input while externally controlled, and preserves complete interrupted-move identity in cancellation events.
  - `TransportBinding` maintains both external-control claim and progress. Transport direction reverses only the loop-relative playhead while elapsed/procedural clocks remain monotonic. Preserve signed multi-wrap traversal until domains emit every crossed boundary.
  - Public error enums use `thiserror` and preserve downstream sources. Every fallible public constructor and erased selection/capability path needs exact-variant tests; public breaking changes require crate changelogs and all workspace and sibling-Hana consumers must migrate to compatible pinned revisions.

## Phases

### Phase 1 — Semantic math invariants  · status: todo

#### Work Order

**Goal:** `bevy_kana` provides the semantic angle/orientation contract required by arrangements, and the whole workspace uses the valid-by-construction orientation API.

**Spec:**

- Add public `Angle` as the signed, unwrapped angular-displacement type. It preserves finite multi-turn values and never normalizes modulo a turn. Retain the approved `Angle::from_radians(...)` call shape and provide read/conversion access needed at Bevy/glam calculation boundaries.
- Make `Orientation` storage private. Fallible raw-`Quat` construction rejects non-finite and effectively zero-length quaternions and normalizes every accepted quaternion. Rotation composition and interpolation preserve the invariant. Remove tuple construction and infallible `From<Quat>`.
- Keep `Position` for spatial points and `Displacement` for offsets. Do not replace these public semantic values with raw Bevy/glam values.
- Use opaque reflection for invariant-bearing wrappers so structural `FromReflect` cannot replace private validated state. Do not add manual registration for non-generic types; `reflect_auto_register` owns that.
- Update all current workspace consumers needed to keep the tree green, while leaving the arrangement-specific field migration to Phase 4.
- Add the breaking `Orientation` migration and `Angle` addition to `crates/bevy_kana/CHANGELOG.md` and update its README/prelude exports.

**Files:**

- `crates/bevy_kana/src/math/space/orientation.rs` — private normalized orientation construction.
- `crates/bevy_kana/src/math/space/angle.rs` — new semantic angle.
- `crates/bevy_kana/src/math/{mod,space/mod}.rs` — module wiring.
- `crates/bevy_kana/src/{lib,prelude}.rs` — public exports.
- `crates/bevy_kana/{README.md,CHANGELOG.md}` — API and migration docs.
- Workspace Rust consumers found by the existing `Orientation` API — compile migration only.

**Constraints from prior phases:** None.

**Acceptance gate:** `Angle` preserves positive and negative multi-turn values; accepted raw quaternions are normalized; non-finite and effectively zero quaternions are rejected; invalid reflected values cannot bypass private construction; workspace build/tests and the full `clippy` skill pass.

### Phase 2 — Shared sequence playback engine  · status: todo

#### Work Order

**Goal:** `bevy_kana` supplies history-independent normalized sequence playback and exact ordered boundary traversal for both fold and camera domains.

**Spec:**

- Add public `SequenceDirection::{Forward, Backward}` and an ordinary `SequencePlayback` state machine embedded by domain runtime components rather than attached independently.
- `SequencePlayback` stores normalized current position, destination/paused state, ordered normalized boundary positions, a gap cursor, and mutually exclusive native/external control mode. It advances native playback, supports absolute forward/backward destinations, pause/resume, adjacent-boundary stepping, arbitrary seeks, and zero-duration cursor movement at one scalar position.
- Add exact `SequenceTime` with private `u128` whole seconds and `u32` nanoseconds plus conversion accessors. It must sum every `Duration` in an in-memory sequence without rejecting authored durations or losing boundary precision.
- Each domain owns an immutable boundary ledger with stable ordinals and domain metadata. Shared playback returns a compact traversal descriptor over old/new gaps, direction, and signed whole-ledger repetitions for multiple wraps. Do not allocate a transition `Vec` per update.
- `SequenceUpdate::NoTraversal` means neither scalar position nor ordered cursor changed. `Traversed` returns the final normalized position, direction, and compact traversal descriptor; it also represents zero-duration cursor-only movement.
- Define deterministic coincident ordering: exiting member records precede old stage end; old stage end precedes new stage begin; stage begin encloses member begins; member ends precede stage end; zero-duration member begin/end pairs are adjacent in group order; a reached endpoint is last. Backward traversal reverses the deterministic record order.
- Add shared scheduling sets:

```rust
#[derive(SystemSet, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum SequencePlaybackSystems {
    ProduceExternalProgress,
    EvaluateSequences,
}
```

  Configure them as an `Update` chain without moving domain evaluation into `bevy_kana`.
- Reuse playback buffers/cursors and keep an equal-position, no-boundary, no-mode-change update write-free.

**Files:**

- `crates/bevy_kana/src/sequence.rs` — shared timing, playback, cursor, traversal, direction, and system sets.
- `crates/bevy_kana/src/{lib,prelude}.rs` — exports.
- `crates/bevy_kana/CHANGELOG.md` — shared playback addition.

**Constraints from prior phases:** `Angle` and `Orientation` are semantic invariant types from Phase 1; this phase does not weaken their reflection contract.

**Acceptance gate:** Tests cover forward/backward travel, pause/resume, interior reversal, adjacent stepping, large seeks, coincident and zero-duration boundaries, endpoints, all-zero sequences, equal-position idle change ticks, and signed multi-wrap traversal without per-update transition allocation; workspace build/tests and the full `clippy` skill pass.

### Phase 3 — External progress and tween production  · status: todo

#### Work Order

**Goal:** any domain sequence can switch safely between native playback and an explicit external animation system, including Hana transport and optional `bevy_tween` producers.

**Spec:**

- Add the colocated components:

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct ExternalAnimationProgress(f32);

impl ExternalAnimationProgress {
    pub const START: Self = Self(0.0);
    pub const END: Self = Self(1.0);

    pub fn try_new(normalized: f32) -> Result<Self, AnimationProgressError>;
    pub const fn normalized(self) -> f32;
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct ExternalAnimationControl;
```

- Progress is finite and within `0.0..=1.0`; it has no `Default`, infallible scalar conversion, mutable dereference, duration, direction, or easing. Opaque reflection must not bypass construction.
- `ExternalAnimationControl` is the authoritative external-writer claim even when a sample is temporarily absent. Claim insertion cancels an active native journey through the domain lifecycle; unchanged progress holds the selected pose and native commands are rejected. Claim removal leaves native playback idle at the last external position and restores no prior journey.
- Add a shared typed rejection reason that domain-specific fold/camera rejection events can carry. Endpoint-idempotent commands and other valid no-ops are accepted no-ops, not rejections.
- Under a `bevy_kana` `tween` feature, add `ExternalAnimationProgressInterpolator::new(start, end)`. It applies no easing itself, supports forward/backward/partial ranges, preserves finite mapped overshoot inside the complete sequence, clamps only below `START` or above `END`, and leaves the prior component value unchanged for non-finite output.
- A tween permanently yields for the rest of that tween after another external owner claims the entity; it must not revive as a competing writer later. Configure the supported tween time/interpolation/write pipeline before `SequencePlaybackSystems::EvaluateSequences` and prove same-frame output.
- Add a runnable `bevy_kana` example covering forward, backward, partial ranges, ownership conflict, and the fact that upstream easing may overshoot.

**Files:**

- `crates/bevy_kana/Cargo.toml` — optional tween feature/dependency.
- `crates/bevy_kana/src/sequence.rs` — external ownership integration and rejection reason.
- `crates/bevy_kana/src/external_progress.rs` — progress/control components and error.
- `crates/bevy_kana/src/tween.rs` — optional interpolator and producer ordering.
- `crates/bevy_kana/src/{lib,prelude}.rs` — exports.
- `crates/bevy_kana/examples/external_progress.rs` — runnable integration.
- `crates/bevy_kana/{README.md,CHANGELOG.md}` — contract and overshoot documentation.

**Constraints from prior phases:** Use Phase 2 `SequencePlaybackSystems`, playback ownership mode, and compact traversal; use Phase 1 opaque invariant reflection.

**Acceptance gate:** Invalid progress is unconstructable; claim-without-sample still blocks native commands; insertion/removal transitions preserve the documented last position; a competing tween never revives; overshooting easings behave as documented; the example runs; workspace build/tests and the full `clippy` skill pass.

### Phase 4 — Anchor geometry and resolver contract  · status: todo

#### Work Order

**Goal:** `hana_valence` anchor geometry is named, semantic, and valid at construction, while runtime resolution remains best effort and allocation-conscious.

**Spec:**

- Rename `AnchorId` to `AnchorSite` and `EdgeMid` to `EdgeMidpoint`:

```rust
#[non_exhaustive]
pub enum AnchorSite {
    Vertex(u32),
    EdgeMidpoint(u32),
    Center,
}
```

  Document that vertex and edge indices use provider-defined ordering; `Center` is one whole-member site.
- Replace `AnchorPoint` with:

```rust
pub struct AnchorFrame {
    position: Position,
    orientation: Orientation,
}

impl AnchorFrame {
    pub fn try_new(
        position: Position,
        orientation: Orientation,
    ) -> Result<Self, GeometryError>;

    pub const fn position(&self) -> Position;
    pub const fn orientation(&self) -> Orientation;
}
```

  Both values are required; origin/identity express defaults. Reject non-finite position once; `Orientation` is already valid.
- Keep `Edge` as an ordered ordinary value with `start: AnchorSite` and `end: AnchorSite`; reversing endpoints reverses the axis/sign convention. `Edge::axis()` returns native `Dir3` at the calculation boundary.
- Replace public geometry fields and the separate `validate()` call with:

```rust
pub struct ResolvedAnchorGeometry {
    frames: HashMap<AnchorSite, AnchorFrame>,
    edges: Vec<Edge>,
}

impl ResolvedAnchorGeometry {
    pub fn try_new(
        frames: impl IntoIterator<Item = (AnchorSite, AnchorFrame)>,
        edges: impl IntoIterator<Item = Edge>,
    ) -> Result<Self, GeometryError>;

    pub fn frame(&self, site: AnchorSite) -> Result<&AnchorFrame, GeometryError>;
    pub fn frames(&self) -> impl Iterator<Item = (&AnchorSite, &AnchorFrame)>;
    pub fn edges(&self) -> &[Edge];
}
```

  Construction rejects duplicate sites, missing endpoints, a same-site edge, and near-coincident endpoints. Lookup uses `GeometryError::MissingAnchorSite { site }` rather than `Option`.
- Convert `AnchoredTo::offset` and `ResolvedAnchorOffset` to `Displacement`; convert `AnchorPose` rotation/translation to `Orientation`/`Displacement`; retain native Bevy/glam values at `Transform` and axis-calculation boundaries.
- Public error enums use `thiserror`. The geometry contract includes `NonFiniteAnchorPosition`, `DuplicateAnchorSite`, `MissingAnchorSite`, and `DegenerateEdge` causes.
- Keep `AnchoredTo` immutable and relationship-maintained; relationship replacement is the retarget path. `AnchoredTo` records only point-to-point attachment, not membership, edge, hinge endpoints, or timing.
- `resolve_anchors()` reads `AnchorPose` and relationship geometry, writes `Transform`, and rebuilds from current ECS state. Reuse per-system maps/vectors/queues/cycle-walk scratch but retain no topology reconciliation or deduplicated diagnostic state. Missing inputs skip that output and can participate when inputs later arrive.
- Use opaque reflection or omit reflected component editing for validated composites; test that dynamic reflection cannot introduce invalid geometry.

**Files:**

- `crates/hana_valence/Cargo.toml` — `thiserror` and semantic dependencies.
- `crates/hana_valence/src/geometry.rs` — sites, frames, edges, construction, errors.
- `crates/hana_valence/src/relation.rs` — semantic relationship offsets.
- `crates/hana_valence/src/{attachment,resolve,pose}.rs` — resolver and pose migration.
- `crates/hana_valence/src/lib.rs` — exports and docs.
- `crates/hana_valence/{fixtures.rs,tests/arrangements.rs}` — constructor, lookup, relationship, reflection, and resolution tests.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — breaking migration.

**Constraints from prior phases:** Use Phase 1 `Position`, `Displacement`, `Orientation`, and `Angle`; preserve their opaque reflection. Shared playback from Phases 2–3 is available but not used by geometry.

**Acceptance gate:** Every geometry constructor/lookup error has an exact-variant test; relationship replacement maintains `AnchoredHere`; nonzero semantic offsets resolve correctly; invalid reflection cannot bypass constructors; resolver scratch is reused; missing runtime inputs do not delete authoring; workspace build/tests and the full `clippy` skill pass.

### Phase 5 — Pure arrangement provider and plan  · status: todo

#### Work Order

**Goal:** provider authors can generate one typed, structurally valid arrangement plan against the library’s authoritative logical-member association without touching ECS state.

**Spec:**

- Add the public ordinary extension trait:

```rust
pub trait ArrangementProvider {
    type Member: Eq + Hash + Debug;
    type FoldGroupSelection: Eq + Hash + Debug + Send + Sync + 'static;

    fn members(&self) -> impl Iterator<Item = Self::Member>;

    fn generate_plan(
        &self,
        members: &ArrangementMemberEntities<Self::Member>,
    ) -> Result<ArrangementPlan<Self::FoldGroupSelection>, ArrangementError>;
}
```

  `Self::Member` is a provider’s authoring-time logical member, not the ECS `Member` relationship. A no-fold provider uses `Infallible` for its selection type.
- The library collects unique logical members once in deterministic order and creates read-only `ArrangementMemberEntities<M>`. It provides fallible logical-member lookup plus ordered iteration; requesting an unlisted member returns `ArrangementError::UnlistedMember` with the member’s `Debug` form. It is the only member/entity association accepted by plan construction.
- Add:

```rust
pub struct Connection {
    pub member_entity: Entity,
    pub anchored_to: AnchoredTo,
    pub member_edge: Edge,
    pub base_angle: Angle,
    pub hinge_clearance: HingeClearance,
}

pub struct HingeClearance {
    positive: Displacement,
    negative: Displacement,
}

impl HingeClearance {
    pub const CENTERED: Self;
    pub const fn new(positive: Displacement, negative: Displacement) -> Self;
    pub const fn positive(&self) -> Displacement;
    pub const fn negative(&self) -> Displacement;
}
```

  `member_entity` is the relationship source; `anchored_to` supplies target/sites; `member_edge` is the source-local ordered axis; `base_angle` is the immutable provider resting endpoint. Positive/negative clearance are independent source-local physical pivot displacements.
- `ArrangementPlan<S>` has private fields and is never an ECS component. `try_new(&ArrangementMemberEntities<M>, connections)` validates the complete connection forest; the finished plan does not retain a duplicate member collection. `S` stays typed until ECS materialization.
- Accept empty members, multiple roots, disconnected forests, branches/repeated targets, and finite multi-turn angles. Reject source/target outside the authoritative association, duplicate sources, self-targets, physical cycles, non-finite offsets/angles/clearance, and an edge with equal sites. There is no separate validation pass or validated-plan wrapper.
- `ArrangementError` uses `thiserror`, preserves a boxed downstream provider error as its source, and uses readable type names/selection `Debug` values rather than exposing `TypeId`.
- Keep application-owned ECS state out of this phase: no world scan, scene invocation, relationship insertion, or retained diagnostics.

**Files:**

- `crates/hana_valence/src/arrangement/mod.rs` — provider trait and public re-exports.
- `crates/hana_valence/src/arrangement/member_entities.rs` — authoritative logical-member association.
- `crates/hana_valence/src/arrangement/plan.rs` — connection, clearance, typed plan, and errors.
- `crates/hana_valence/src/lib.rs` — module wiring/exports.
- `crates/hana_valence/tests/arrangements.rs` — provider/plan construction coverage.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — provider-author API.

**Constraints from prior phases:** Use Phase 4 `AnchorSite`, `Edge`, `AnchoredTo`, and semantic spatial values; use Phase 1 `Angle`; preserve the project-wide no-separate-validation rule.

**Acceptance gate:** Tests prove every accepted graph shape and every rejected construction cause, including zero members, branches, disconnected roots, cycles, foreign entities, and duplicate sources; the plan cannot accept a second member collection or an unrelated selection type; no ECS state changes during pure failure; workspace build/tests and the full `clippy` skill pass.

### Phase 6 — Arrangement ECS materialization and commands  · status: todo

#### Work Order

**Goal:** application code can spawn a complete arrangement controller and member roots through one BSN-capable command API or bind a provider to existing entities.

**Spec:**

- Add public controller identity `Arrangement` on one non-spatial entity. Zero-member arrangements and arrangements without fold behavior are valid.
- Replace the duplicate arrangement-member storage with Bevy’s relationship pair:

```rust
#[derive(Component)]
#[component(immutable)]
#[relationship(relationship_target = Members)]
pub struct Member {
    #[relationship]
    pub root_entity: Entity,
}
```

  Bevy maintains `Members` on the controller. Retargeting replaces the complete immutable relationship. Store no `MemberIndex`; use current `Members` order only when enumeration is needed.
- Add named existing-member binding:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberBinding {
    Bound(Entity),
    Missing,
}
```

- Export `ArrangementCommandsExt` through the prelude with:

```rust
fn spawn_arrangement<P, F, S>(
    &mut self,
    provider: P,
    scene_for_member: F,
) -> Result<Entity, ArrangementError>
where
    P: ArrangementProvider,
    F: FnMut(&P::Member) -> S,
    S: Scene;

fn spawn_arrangement_from_members<P, F>(
    &mut self,
    provider: P,
    binding_for_member: F,
) -> Result<Entity, ArrangementError>
where
    P: ArrangementProvider,
    F: FnMut(&P::Member) -> MemberBinding;
```

- `spawn_arrangement()` reserves controller/member IDs, builds the authoritative association, generates and validates the complete plan, and only then invokes `scene_for_member()` and queues materialization. On synchronous plan failure, queue cleanup of all reserved IDs before returning the error. Never invoke a member scene for an invalid plan.
- Apply each returned scene onto its reserved member root with `queue_apply_scene()`; do not spawn a second scene root. `()` is the empty-scene option. Related entities inside the scene are not arrangement members unless separately related.
- `spawn_arrangement_from_members()` creates only the controller, consumes every binding before inserting anything, rejects `Missing` and duplicate bound entities, trusts `Bound(Entity)` without preflighting later ECS existence, and creates no replacement members/scenes.
- Materialize `Member`, `AnchoredTo`, and private retained connection/selection state; provider and plan are dropped. A member absent from connection sources is a physical root. Do not assume the preceding `Members` entry is a parent.
- Add `ArrangementPlugin` as the sole arrangement construction/materialization owner. Require an application-configured `AssetPlugin`; add `ScenePlugin` when absent. `FoldPlugin` composes with it without duplicate registration. A queued scene is same-frame only when dependencies are ready and deferred commands apply before Bevy’s `SpawnScene`; otherwise it applies later.
- Preserve direct advanced authoring: applications may attach valid `Member`, `AnchoredTo`, and later `Hinge` values without a provider. Dependent systems remain inert when their own inputs are missing and resume when those inputs arrive.
- Remove `ArrangementMembers`, `MemberIndex`, `TilingRule`, `QuadTiling`, `ArrangementPlacement`, `MemberPlacement`, their index/repair systems, and duplicate plan/member state. A one-row/one-column sheet will be an ordinary degenerate sheet, not a `Strip` type.

**Files:**

- `crates/hana_valence/Cargo.toml` — production Bevy asset/scene/BSN dependencies.
- `crates/hana_valence/src/arrangement/{mod,commands,materialize,plugin}.rs` — ECS identity, commands, scene/plugin contract, and materialization.
- `crates/hana_valence/src/arrange.rs` — remove/replace legacy arrangement implementation.
- `crates/hana_valence/src/lib.rs` — prelude/export/plugin docs.
- `crates/hana_valence/tests/arrangements.rs` — spawn, cleanup, relationships, existing members, and scene timing.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — breaking arrangement migration.
- Current workspace arrangement consumers — minimal compile migration to the new relationship/controller API.

**Constraints from prior phases:** Phase 5 provides the sole authoritative member association and typed plan. Phase 4 relationships/geometry remain the physical attachment representation. Baseline hinge materialization is deliberately completed atomically with the final hinge model in Phase 9; retain all connection calibration privately until then.

**Acceptance gate:** Tests cover valid code-authored BSN scenes, `()` scenes, same-frame/late application boundary, invalid-plan cleanup before scene invocation, zero members, existing bindings, duplicate/missing bindings, multiple physical roots, branches, disconnected forests, and relationship retargeting; removed duplicate membership APIs are absent; workspace build/tests and the full `clippy` skill pass.

### Phase 7 — Fold groups and retained provider capabilities  · status: todo

#### Work Order

**Goal:** a typed arrangement plan can retain ordered fold-group alternatives and purpose-specific provider knowledge for later transient recipes.

**Spec:**

- Add one nonempty, ordered, internally unique group:

```rust
pub struct FoldGroup {
    members: Vec<Entity>,
}

impl FoldGroup {
    pub fn try_new(
        first: Entity,
        remaining: impl IntoIterator<Item = Entity>,
    ) -> Result<Self, FoldAuthorError>;

    pub fn iter(&self) -> impl Iterator<Item = &Entity>;

    pub fn combine<'a>(
        groups: impl IntoIterator<Item = &'a FoldGroup>,
    ) -> Result<Self, FoldAuthorError>;
}
```

  `From<Entity>` is the one-member case. `TryFrom<Vec<Entity>>` and `try_from_iter()` reject empty or repeated members. Vector position is the semantic implicit group-local index. One entity may appear in several different groups.
- Add `FoldGroups` as a nonempty ordered collection with private `Vec<FoldGroup>`, `new(first, remaining)`, `From<FoldGroup>`, fallible dynamic conversion, slice access, indexing, borrowed iteration, and consuming iteration. It has no `Default`, unchecked conversion, mutable access, or absence variant.
- Extend `ArrangementPlan<S>` with selection-typed methods:

```rust
pub fn with_fold_groups(
    self,
    selection: S,
    groups: FoldGroups,
) -> Result<Self, ArrangementError>;

pub fn with_capability<C>(
    self,
    selection: S,
    capability: C,
) -> Result<Self, ArrangementError>
where
    C: Send + Sync + 'static;
```

  One `S` selects one complete group alternative. Retain associations in a private type-erased ECS table only after materialization. Reject duplicate selections, unknown selections for capability association, group members that are not retained connection sources, and duplicate concrete capability types. Generic association does not pretend to validate invariants inside arbitrary `C`.
- Keep the approved extension contract:

```rust
pub trait Provides<C>: ArrangementProvider {
    fn provide(
        &self,
        selection: &Self::FoldGroupSelection,
        groups: &FoldGroups,
        connections: &[Connection],
    ) -> Result<C, ArrangementError>;
}
```

  Provider code calls it during `generate_plan()`; application code does not.
- Add `WindingClearance`, a private mapping from connection member to canonical-positive `Displacement`. Its capability-specific constructor rejects duplicates/non-finite values and validates exact coverage for the selected fold groups before generic association. `clearance_for()` returns `FoldAuthorError::MissingWindingClearance { member_entity }`, not `Option`.
- Selection bounds remain `Eq + Hash + Debug + Send + Sync + 'static`; do not require clone, reflection, type paths, or serialization. Errors distinguish wrong selection Rust type, unknown selection value, missing provider input, duplicate selection, duplicate provider input, and boxed provider failure.
- Reflection does not expose the private type-erased table or bypass invariant-preserving capability constructors.

**Files:**

- `crates/hana_valence/src/arrangement/{plan,capabilities}.rs` — typed selections and private retention.
- `crates/hana_valence/src/fold/group.rs` — `FoldGroup` and `FoldGroups`.
- `crates/hana_valence/src/fold/error.rs` — `thiserror` construction/capability errors.
- `crates/hana_valence/src/lib.rs` — exports.
- `crates/hana_valence/tests/arrangements.rs` — exact erased-selection/capability coverage.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — provider/recipe extension API.

**Constraints from prior phases:** Extend the generic `ArrangementPlan<S>` and private materialized state from Phases 5–6; accept only members from the Phase 5 authoritative association; preserve named absence/error states.

**Acceptance gate:** Exact-variant tests cover wrong selection type, unknown value, missing provider input, duplicate selection/input, foreign group member, empty/duplicate group construction, overlapping groups, and incomplete winding clearance; every failure before materialization or recipe writes leaves state unchanged; workspace build/tests and the full `clippy` skill pass.

### Phase 8 — Built-in sheet providers  · status: todo

#### Work Order

**Goal:** application code can construct reusable quad and triangle sheets, including one-dimensional strips, with provider-authored connections and typed row/column folding alternatives.

**Spec:**

- Add public `QuadSheet`, `QuadCell`, `QuadFoldGroupSelection::{Rows, Columns}`, `TriangleSheet`, `TriangleCell`, and `TriangleFoldGroupSelection::{Rows, Columns}`.
- `QuadSheet::new(rows, columns)` and `TriangleSheet::new(rows, columns)` are ordinary pure providers. They enumerate deterministic logical cells, generate physical `Connection` forests without a previous-member assumption, and provide documented logical-cell geometry for member scenes.
- One row or one column is a normal degenerate sheet and supports compatible row/column selection. Add no `Strip` provider, recipe, component, or marker.
- Each provider generates complete alternative `FoldGroups` for supported rows and columns. Outer and inner positions have documented geometric meaning so Accordion/Coil/custom recipes can use ordering functionally.
- Providers implement `Provides<WindingClearance>` only for selections whose geometry can supply exact canonical-positive exterior pivot displacement. Capability construction receives the selected groups and connections and validates complete coverage before association.
- Providers return `ArrangementPlan<Self::FoldGroupSelection>` using the passed `ArrangementMemberEntities`; they never recollect or submit a second entity list.
- Keep hex sheets and box nets downstream/example-defined to prove the extension contract rather than adding more core types.

**Files:**

- `crates/hana_valence/src/providers/mod.rs` — built-in provider exports.
- `crates/hana_valence/src/providers/quad.rs` — quad cells, selections, connections, groups, clearance.
- `crates/hana_valence/src/providers/triangle.rs` — triangle cells, selections, connections, groups, clearance.
- `crates/hana_valence/src/lib.rs` — public exports/prelude.
- `crates/hana_valence/tests/arrangements.rs` — dimensions, ordering, degenerate sheets, plan and capability tests.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — ordinary provider call site.

**Constraints from prior phases:** Implement Phase 5 provider signature and typed plan; use Phase 7 groups/capabilities; spawn/materialization remains Phase 6. Do not retain provider logical members in ECS.

**Acceptance gate:** Quad and triangle plans cover rectangular dimensions, row/column alternatives, one-row/one-column cases, deterministic ordering, valid forests, and winding-clearance coverage; example-local hex/no-fold/box providers compile against the same trait in tests; workspace build/tests and the full `clippy` skill pass.

### Phase 9 — Fold recipes and final hinge calibration  · status: todo

#### Work Order

**Goal:** transient built-in or downstream recipes can replace every selected baseline hinge endpoint atomically up to the write boundary while preserving provider geometry.

**Spec:**

- Add the public extension trait and recipe-owned result:

```rust
pub trait FoldRecipe {
    type ProviderInput: Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    fn fold_assignments(
        &self,
        groups: &[Vec<Connection>],
        provider_input: &Self::ProviderInput,
    ) -> Result<Vec<FoldAssignment>, Self::Error>;
}

pub struct FoldAssignment {
    pub member_entity: Entity,
    pub folded_angle: Angle,
    pub pivot_offset: Displacement,
}
```

  Nested connections preserve outer-group and inner-member order. `FoldAssignment` deliberately omits `Edge`/base angle and remains a plain vector result so duplicate/omitted entries are visible during whole-result construction.
- Rework the durable component:

```rust
#[derive(Component)]
#[require(AnchorPose)]
pub struct Hinge {
    pub edge: Edge,
    pub base_angle: Angle,
    pub folded_angle: Angle,
    pub pivot_offset: Displacement,
}
```

  It stores two resting endpoints and pivot calibration, never mutable current angle. Every materialized `Connection` creates a baseline hinge with equal angles and zero pivot. Direct construction enforces finite authored fields once.
- `hinge_to_pose()` derives current angle from sequence position when present; without `FoldSequence`, it evaluates `base_angle`. A present but unresolved sequence preserves the previous `AnchorPose`. It reads `AnchoredTo` plus source geometry, converts the ordered member edge into the source-anchor frame, and compensates from the delta against `base_angle`. `ArrangementPlugin` is its sole registrar.
- Export deferred command:

```rust
fn apply_fold_recipe<S, R>(
    &mut self,
    arrangement_entity: Entity,
    selection: S,
    recipe: R,
)
where
    S: Eq + Hash + Debug + Send + Sync + 'static,
    R: FoldRecipe + Send + 'static;
```

  It retrieves retained groups/connections and the exact `ProviderInput`, calls the recipe, consumes the result once into a complete temporary set, and rejects omitted, duplicate, foreign, non-finite angle, or non-finite pivot assignments before starting writes. Selection/capability/recipe/assignment failures warn through Bevy’s per-command handler with arrangement and concrete source and change no hinge. Once writes begin, missing/repurposed application-owned entities are best effort rather than transactional.
- Recipe replacement is allowed only at the shared base endpoint; away from base it reports a conflict and retains current hinges. It does not change playback, take external ownership, or queue a pending recipe.
- Built-ins:
  - `Accordion { fold_offset: Angle }`; `default()` is a positive half-turn. Outer group parity adds/subtracts the offset from each connection base. All connections in one group share direction. Repeated entities of matching parity coalesce; opposite parity returns `ConflictingAccordionDirections` with both group positions. Select signed `HingeClearance`.
  - `Coil { fold_offset: Angle }`; no default. Apply the same signed relative offset at every selected connection; coalesce repeated entities in first-occurrence diagnostic order; select signed `HingeClearance`.
  - `Wrap { fold_offset: Angle }`; no default. Use `WindingClearance` final displacement without adding connection clearance again; mirror canonical-positive clearance for negative offset; coalesce repeated entities. Direct incomplete input returns exact `MissingWindingClearance` and no partial vector.
- Associated errors remain `Accordion::Error = FoldAuthorError`, `Wrap::Error = FoldAuthorError`, and `Coil::Error = Infallible`. Custom recipes preserve their concrete error source.
- Remove `Strip`, `FoldAngles`, mutable hinge-angle actuation, `HingePivot`, `HingeAngleLens`, `AnchorPoseLens`, `FoldSystems::Actuate`, `drive_arrangement_hinges()`, `actuate_fold_hinges()`, and public `Hinge::rotation()`.

**Files:**

- `crates/hana_valence/src/hinge.rs` — final calibration and pose conversion.
- `crates/hana_valence/src/fold/{recipe,recipes,author,error}.rs` — extension trait, assignments, built-ins, deferred application.
- `crates/hana_valence/src/fold/{mod,hinge}.rs`, `crates/hana_valence/src/tween.rs`, `crates/hana_valence/src/arrange.rs` — remove legacy actuation/components/lenses.
- `crates/hana_valence/src/arrangement/{commands,materialize}.rs` — baseline hinges and apply command.
- `crates/hana_valence/src/lib.rs` — exports/removals.
- `crates/hana_valence/tests/arrangements.rs` — algorithms, whole-result validation, base-only conflict, and best-effort writes.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — recipe/hinge migration.

**Constraints from prior phases:** Use retained typed selections/capabilities from Phase 7, built-in provider output from Phase 8, semantic geometry from Phase 4, and arrangement plugin/materialization from Phase 6. Do not add a separate runtime validation scan.

**Acceptance gate:** Tests cover baseline nonzero base pose, both clearance directions, asymmetric pivot, Accordion parity/conflict, Coil/Wrap repeated entities, direct Wrap failure, every assignment coverage error, unchanged hinges for every pre-write failure, base-only replacement conflict, downstream custom recipe/error, and missing ECS state after write start; removed legacy APIs are absent; workspace build/tests and the full `clippy` skill pass.

### Phase 10 — Fold sequence authoring  · status: todo

#### Work Order

**Goal:** arrangements can retain functional, history-independent fold sequence authoring with stage/member target and whole-value timing cascades.

**Spec:**

- Add validated normalized destination:

```rust
pub struct FoldTarget(f32);

impl FoldTarget {
    pub const BASE: Self = Self(0.0);
    pub const FOLDED: Self = Self(1.0);
    pub fn try_new(fraction: f32) -> Result<Self, FoldAuthorError>;
    pub const fn fraction(self) -> f32;
}
```

  Reject non-finite/out-of-range values. Sequence start is `BASE`; finite easing may overshoot later without changing authored targets.
- Add one whole timing value:

```rust
pub struct FoldTiming {
    pub start_offset: Duration,
    pub duration: Duration,
    pub easing: EaseFunction,
}
```

  Member override resolves before stage override before required sequence default. Overrides replace the entire timing value; do not expose `Cascade<FoldTiming>` in public Hana Valence fields/signatures. Authored durations are trusted; zero duration is an instantaneous snap.
- A stage lasts through the greatest `start_offset + duration` among its members; simultaneous members may share timing, and overlapping offsets produce waves without splitting one synchronization stage.
- Add `FoldStage` as an ordinary sequence-owned value containing one `FoldGroup`, one aligned `FoldTarget` per member, and private stage/member `Cascade<FoldTiming>` storage. `FoldStage::from(FoldGroup)` assigns `FOLDED` and inherited timing. Provide:

```rust
pub fn with_member_target(self, member: Entity, target: FoldTarget)
    -> Result<Self, FoldAuthorError>;
pub fn override_timing(self, timing: FoldTiming) -> Self;
pub fn inherit_timing(self) -> Self;
pub fn override_member_timing(self, member: Entity, timing: FoldTiming)
    -> Result<Self, FoldAuthorError>;
pub fn inherit_member_timing(self, member: Entity)
    -> Result<Self, FoldAuthorError>;
pub fn override_member_timings_with(
    self,
    override_for: impl FnMut(usize, Entity) -> FoldTiming,
) -> Self;
```

- `FoldSequence` owns ordered stages and required default timing. A zero-stage sequence is valid. Repeated members across stages are valid; precompute one ordered segment track per unique member, where each occurrence starts at its previous target and ends at its stage target. Arbitrary sampling uses the containing segment or latest completed target, never prior mutable output.
- Add pure builder:

```rust
impl FoldSequenceBuilder {
    pub fn new(default_timing: FoldTiming) -> Self;
    pub fn stage(self, stage: impl Into<FoldStage>) -> Self;
    pub fn stages<I, S>(self, stages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<FoldStage>;
    pub fn build(self) -> FoldSequence;
}
```

  It owns no `Commands`; `build()` is infallible because constituent values enforce local invariants.
- Add provider conveniences:

```rust
fn with_fold_sequence(
    self,
    selection: Self::FoldGroupSelection,
    default_timing: FoldTiming,
) -> impl ArrangementProvider<Member = Self::Member, FoldGroupSelection = Self::FoldGroupSelection>;

fn with_custom_fold_sequence<F>(
    self,
    selection: Self::FoldGroupSelection,
    sequence_for_groups: F,
) -> impl ArrangementProvider<Member = Self::Member, FoldGroupSelection = Self::FoldGroupSelection>
where
    F: FnOnce(&FoldGroups) -> Result<FoldSequence, FoldAuthorError>;
```

  The custom closure runs once synchronously after logical members have entities but before materialization. It may combine, subdivide, reorder, or omit groups and customize all timing. Failure aborts spawn before scenes/materialization. Add no public wrapper type, post-spawn group-selection setter, or per-member fold relationship/component.
- Raw member time clamps to `0.0..=1.0` before easing; every finite easing result, including under/overshoot, is preserved when interpolating targets and hinge angles. Non-finite easing leaves `AnchorPose` unchanged and warns contextually.

**Files:**

- `crates/hana_valence/src/fold/{sequence,timing,error}.rs` — target, stage, sequence, tracks, timing, builder.
- `crates/hana_valence/src/arrangement/provider.rs` — sequence provider adapters.
- `crates/hana_valence/src/arrangement/materialize.rs` — optional sequence materialization.
- `crates/hana_valence/src/lib.rs` — exports.
- `crates/hana_valence/tests/arrangements.rs` — authoring, cascade, tracks, closure failure.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — functional authoring examples.

**Constraints from prior phases:** Use Phase 7 `FoldGroup(s)` and existing Phase 1/3 easing/math contracts; materialize through Phase 6 only after pure success; Phase 9 hinges define endpoints. Keep generic `Cascade<T>` in `bevy_kana` and hidden behind domain verbs.

**Acceptance gate:** Tests cover zero stages, repeated members, intermediate targets, forward/backward/arbitrary history-independent segment sampling, inherited/overridden sequence-stage-member timing, simultaneous/sequential/staggered/wave/snap timing, finite easing overshoot, non-finite easing, standard/custom provider adapters, group combination/subdivision/reordering/omission, and atomic closure failure; workspace build/tests and the full `clippy` skill pass.

### Phase 11 — Fold playback, commands, and timed events  · status: todo

#### Work Order

**Goal:** native commands and external progress drive the same retained fold sequence, write deterministic poses, and emit every crossed stage/member/endpoint event with exact boundary timing.

**Spec:**

- `FoldSequenceState` is a public library-written component with private fields: embedded `SequencePlayback`, resolved timing/boundary ledger, per-member tracks, and lookup caches. Provide read-only `is_paused()` and `normalized_position()`; application code cannot mutate playback.
- `FoldPlugin` composes `ArrangementPlugin` and installs authoring-state rebuild, command observers, playback/event evaluation, and fold output. `FoldSystems::Advance` remains the public boundary in `SequencePlaybackSystems::EvaluateSequences`; remove `FoldSystems::Actuate`. `ArrangementPlugin` alone registers `hinge_to_pose()`. In `PostUpdate`, hinge pose precedes anchor resolution and transform propagation.
- Add entity-targeted constructors:

```rust
commands.trigger(FoldCommand::play(arrangement));
commands.trigger(FoldCommand::play_backward(arrangement));
commands.trigger(FoldCommand::pause(arrangement));
commands.trigger(FoldCommand::resume(arrangement));
commands.trigger(FoldCommand::step(arrangement));
commands.trigger(FoldCommand::step_backward(arrangement));
```

  Play destinations are absolute `1.0`/`0.0`, reverse smoothly from an interior journey, and are idempotent at destination. Pause retains destination; resume does nothing without a paused journey. Step travels to the adjacent stage boundary and stops. Add no relative reverse or pause toggle.
- Replacing `FoldSequence` discards old derived state. Native replacement cancels the old journey and leaves the new sequence idle at `START`; under external control, evaluate immediately at current external progress. Emit no traversal events between unrelated old/new boundary sets.
- Same-frame precedence is: rebuild replaced authoring; resolve effective `ExternalAnimationControl`; process ownership transitions; consume eligible commands; sample; emit events; write output. External claim cancels native journey, suppresses all native commands through a targeted `FoldCommandRejected` carrying command and shared typed reason, and remains authoritative without a current sample.
- Evaluate every unique staged member from immutable tracks. A hinge without sequence remains at base; an unresolved sequence preserves prior pose. Equal progress performs no output mutation unless traversal, ownership, authoring, or captured input changed.
- Add exact event timing:

```rust
pub struct FoldEventTiming {
    elapsed: SequenceTime,
    total: SequenceTime,
    progress: f32,
}
```

  Provide read-only accessors. Progress is stored explicitly so all-zero sequences distinguish endpoints.
- Add entity events `FoldStageBegin`, `FoldStageEnd`, `MemberFoldBegin`, `MemberFoldEnd`, and `FoldEndpointReached`. Stage events target arrangement; member events target member and include arrangement, stage index, direction, exact event timing, and resolved `FoldTiming`; endpoint includes `FoldEndpoint::{Base, Folded}` and timing.
- Every boundary actually crossed emits once in deterministic Phase 2 order. Backward movement swaps directional begin/end meaning, large seeks emit all boundaries, zero-duration movement emits adjacent begin/end, holding emits none, and multi-wrap traversal emits every crossing.
- Provide a tested member observer pattern that starts a secondary animation and derives catch-up from event boundary timing, resolved member timing, and already-updated sequence position. Do not add a second mutable progress source.
- Remove `FoldMember` / `FoldMembers`, numeric stage components, `FoldFromArrangement`, snapshot request/diagnostic machinery, and obsolete angle diagnostics. Retain controller `FoldDiagnostics` only for playback preparation/controller state if still needed; add no general diagnostic engine.

**Files:**

- `crates/hana_valence/src/fold/{mod,playback,sequence,events}.rs` — plugin, state, commands, evaluation, event translation.
- `crates/hana_valence/src/hinge.rs` — sequence-to-pose consumption and idle writes.
- `crates/hana_valence/src/lib.rs` — exports/removals.
- `crates/hana_valence/tests/arrangements.rs` — command, external, traversal, event, and idle-path coverage.
- `crates/hana_valence/examples/{box,staggered_unfold,triangles}.rs` — compile migration and event-driven effect example.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — playback migration.

**Constraints from prior phases:** Embed Phase 2 playback/time/traversal and Phase 3 external-control/rejection primitives; consume Phase 10 tracks/timing and Phase 9 hinges; follow Phase 6/9 plugin ownership.

**Acceptance gate:** Tests cover every command, interior reversal, pause/resume/step, sequence replacement, native/external takeover/release, rejected commands, arbitrary forward/backward seeks, multi-wrap traversal, zero-duration/all-zero sequences, exact event timing/order/targets, secondary-animation catch-up, base/no-sequence behavior, unresolved state, and idle change ticks; removed APIs are absent; workspace build/tests and the full `clippy` skill pass.

### Phase 12 — Retained camera authoring foundation  · status: todo

#### Work Order

**Goal:** `bevy_lagrange` retains valid camera-move authoring in a nonempty sequence that can later be sampled at arbitrary positions.

**Spec:**

- Make `CameraMove` an opaque value with private variant storage, read-only accessors, and fallible associated constructors equivalent to the existing two forms:

```rust
CameraMove::try_to_look_at(
    position,
    target,
    roll,
    duration,
    easing,
) -> Result<CameraMove, CameraMoveError>;

CameraMove::try_to_orbital_look_at(
    target,
    yaw,
    pitch,
    radius,
    roll,
    duration,
    easing,
) -> Result<CameraMove, CameraMoveError>;
```

  Preserve the current `ToLookAt` and `ToOrbitalLookAt` meanings. Constructors reject non-finite authored values, nonpositive orbital radius, coincident position/look points where orientation is undefined, and invalid easing parameters. Authored `Duration` remains trusted. Use `thiserror` for `CameraMoveError`; opaque reflection cannot bypass construction.
- Add retained authoring:

```rust
#[derive(Component)]
pub struct CameraSequence {
    moves: Vec<CameraMove>,
}

let sequence = CameraSequence::new(first_move)
    .then(second_move)
    .then(third_move);
```

  It is nonempty, has private collection storage/read-only inspection, no `Default`, and a fallible iterator conversion for dynamic input. Complete component replacement is the only re-authoring path.
- Add private `CameraSequenceState` storage shape for `SequencePlayback`, captured camera start, resolved endpoints, interruption snapshots, immutable boundary ledger, and sampling caches. This phase defines/prepares state but does not yet replace the existing runtime queue.
- Preparation captures current camera position/target as normalized `0.0`, resolves every move endpoint once, and validates any endpoint derived from live camera state once. For `FreeCam`, an omitted roll inherits the preceding resolved roll once. Sampling must never repeatedly read live roll or previously written targets.
- Positive-duration moves occupy normalized intervals proportional to duration; zero-duration moves retain ordered instant boundaries. Prepare enough immutable data for history-independent forward, backward, and arbitrary sampling.
- Migrate all current direct `CameraMove` enum construction in the workspace to fallible constructors so private authoring compiles, while retaining the existing `PlayAnimation`/queue runtime as a temporary compatibility path until Phase 13.

**Files:**

- `crates/bevy_lagrange/src/animation/{queue,sequence}.rs` — opaque moves, sequence authoring, prepared state.
- `crates/bevy_lagrange/src/animation/{mod,events,lifecycle}.rs` — temporary compatibility wiring.
- `crates/bevy_lagrange/src/lib.rs` — exports.
- `crates/bevy_lagrange/src/fit/triggers/**/*.rs`, `crates/bevy_lagrange/src/camera_home.rs` — constructor migration.
- Current `bevy_hana` examples/consumers constructing `CameraMove` — constructor migration only.
- `crates/bevy_lagrange/{README.md,CHANGELOG.md}` — constructor/sequence addition.

**Constraints from prior phases:** Embed Phase 2 `SequencePlayback`/boundary concepts and use Phase 3 external contract only in the prepared state; do not duplicate playback logic. Preserve Phase 1 invariant-reflection policy.

**Acceptance gate:** Exact-variant tests cover every `CameraMoveError`, accepted edge values, trusted zero/large duration, nonempty sequence construction, iterator-empty failure, immutable endpoint capture, duration-weighted/zero-duration boundaries, inherited FreeCam roll, and history-independent sample preparation; old runtime remains green; workspace build/tests and the full `clippy` skill pass.

### Phase 13 — Retained camera playback, commands, and events  · status: todo

#### Work Order

**Goal:** retained camera sequences run natively or from external progress with deterministic captured sampling, lifecycle events, input policy, and observable command rejection.

**Spec:**

- Activate private `CameraSequenceState` when retained authoring is first played or externally claimed. Native/external endpoint arrival retains authoring and state; explicit authoring removal removes private state; complete replacement recaptures the live camera as new position `0.0`.
- Add entity-targeted constructors mirroring fold commands:

```rust
commands.trigger(CameraCommand::play(camera));
commands.trigger(CameraCommand::play_backward(camera));
commands.trigger(CameraCommand::pause(camera));
commands.trigger(CameraCommand::resume(camera));
commands.trigger(CameraCommand::step(camera));
commands.trigger(CameraCommand::step_backward(camera));
```

  Use absolute destinations, smooth interior reversal, retained pause destination, no-op resume without paused journey, and adjacent-move stepping. Add targeted `CameraCommandRejected` containing the attempted command and shared typed external-control reason.
- Sample one captured move interval, compute local raw progress, and apply its easing exactly once. Update current and target OrbitCam/FreeCam controller values together, force controller output, bypass damping, and clear already-produced `OrbitCamInput`/`FreeCamInput` whenever external control is effective.
- Follow shared same-frame precedence: authoring rebuild; external ownership; ownership transition; eligible native commands; sample; events; controller writes. Claim-without-sample holds last pose and rejects native commands. Truly stationary state does not mutate controller output.
- Retain private active-journey state that captures/restores controller overrides exactly once and pairs lifecycle begin/end across native completion, external takeover/release, replacement, authoring removal, pause/resume, and conflict cancellation.
- Replace event semantics:
  - `AnimationBegin` includes `SequenceDirection` and fires once when a native journey begins.
  - Every crossed move boundary emits directional `CameraMoveEnd` then `CameraMoveBegin`, each with current-sequence `move_index` and authored `CameraMove`.
  - Endpoint arrival emits `AnimationEnd` after move end.
  - Unchanged position emits nothing; multi-move jumps, zero-duration moves, backward seeks, and multi-wrap traversal use shared deterministic ordering.
- Rename `AnimationReason` to:

```rust
pub enum AnimationEndReason {
    ReachedStart,
    ReachedEnd,
    Cancelled {
        direction: SequenceDirection,
        interrupted_move_index: usize,
        interrupted_move: CameraMove,
    },
}
```

  The complete move is stable cancellation identity; index is supplemental context only.
- Use target-bearing source values:

```rust
pub enum AnimationSource {
    CameraSequence,
    ZoomToFit { target: Entity },
    AnimateToFit { target: Entity },
    LookAt { target: Entity },
    LookAtAndZoomToFit { target: Entity },
}
```

- `AnimationConflictPolicy` governs competing higher-level native requests only: `LastWins` cancels/replaces, `FirstWins` rejects, idle/completed authoring is not conflict. Direct sequence replacement/commands are outside this policy. `CameraInputInterruptBehavior::{Ignore, Cancel, Complete}` remains unchanged for active/paused native journeys; external control suppresses physical input regardless of either native policy.
- `AnimationRejected` carries a typed reason distinguishing native `ConflictPolicy` rejection from `ExternalControl` rejection; it is separate from targeted `CameraCommandRejected` for direct command ownership failures.
- Retain `PlayAnimation` temporarily as an internal/deprecated adapter that constructs `CameraSequence` and triggers `CameraCommand::play`; Phase 14 removes it after all consumers migrate.

**Files:**

- `crates/bevy_lagrange/src/animation/{mod,sequence,events,lifecycle,queue}.rs` — evaluator, commands, state, compatibility adapter, lifecycle.
- `crates/bevy_lagrange/src/{orbit_cam,free_cam}/controller.rs` — current/target writes and damping bypass.
- `crates/bevy_lagrange/src/input/{lifecycle,routing/snapshot}.rs` — native/external input ownership.
- `crates/bevy_lagrange/src/{lib,system_sets}.rs` — exports and `SequencePlaybackSystems` order.
- `crates/bevy_lagrange` unit tests — playback, events, policy, idle writes.
- `crates/bevy_lagrange/CHANGELOG.md` — event/command transition notes (final removals in Phase 14).

**Constraints from prior phases:** Use Phase 12 retained authoring/captured endpoints, Phase 2 traversal/timing, and Phase 3 external control/rejection. Camera evaluation runs in `SequencePlaybackSystems::EvaluateSequences` and does not write shared progress.

**Acceptance gate:** Tests cover all commands, interior reversal, sequence replacement/removal, native/external takeover/release, claim-without-sample, rejected commands, both conflict policies, all input interrupt behaviors, damping bypass/input clearing, exact move/endpoint order in both directions, large seeks, zero duration, cancellation identity, paired lifecycle, and idle change ticks; workspace build/tests and the full `clippy` skill pass.

### Phase 14 — Camera request and consumer migration  · status: todo

#### Work Order

**Goal:** every camera operation and workspace consumer uses retained `CameraSequence` plus `CameraCommand`, and all destructive queue-era APIs are removed with a complete breaking changelog.

**Spec:**

- Higher-level `ZoomToFit`, `AnimateToFit`, `LookAt`, and `LookAtAndZoomToFit` operations insert/replace retained `CameraSequence`, store private source metadata, and trigger `CameraCommand::play`. Preserve their existing conflict-policy behavior and useful zoom lifecycle fields.
- Remove public/internal `PlayAnimation`, `CameraMoveList`, destructive queue processing, and compatibility adapter. Remove `ZoomContext`, `ZoomReason`, `ZoomAnimationMarker`, and queue/source markers made obsolete by private journey state.
- `ZoomBegin`/`ZoomEnd` keep their useful flat fields and nest the corresponding animation lifecycle; `ZoomEnd` uses `AnimationEndReason`. Target entity lives in target-bearing `AnimationSource`, not duplicate optional target fields.
- Update `CameraKind` registration, fit/look request support, camera home/flyover paths, input snapshots, Fairy Dust controls, Hana Diegetic examples, and all direct event observers to the retained API.
- Update the animation, showcase, swapped-axis, and zoom examples to demonstrate every `CameraCommand`, retained replacement, external progress, backward/forward traversal, input conflict, move/endpoint events, and valid `CameraMove` construction.
- `crates/bevy_lagrange/CHANGELOG.md` explicitly documents: `PlayAnimation`/`CameraMoveList` removal and replacement; `AnimationReason` to `AnimationEndReason`; start/end outcomes; direction on begin; move index/direction; target-bearing sources and removed target fields; complete cancellation move; and `ZoomContext`/`ZoomReason` removal.

**Files:**

- `crates/bevy_lagrange/src/animation/{mod,events,lifecycle,queue,sequence}.rs` — remove compatibility/destructive queue and finalize exports.
- `crates/bevy_lagrange/src/{camera_home,camera_kind}.rs` — retained request integration.
- `crates/bevy_lagrange/src/fit/mod.rs`, `crates/bevy_lagrange/src/fit/triggers/**/*.rs` — higher-level source/zoom migration.
- `crates/bevy_lagrange/src/input/{lifecycle,routing/snapshot}.rs` — remove queue-era checks.
- `crates/bevy_lagrange/src/lib.rs` — final public surface.
- `crates/bevy_lagrange/examples/animation.rs`, `crates/bevy_lagrange/examples/showcase/*.rs`, `crates/bevy_lagrange/examples/{swapped_axis,zoom_to_fit}.rs` — runnable migration.
- `crates/fairy_dust/src/{camera_home,lib}.rs`, `crates/fairy_dust/src/camera_control_panel/preset_switch.rs` — command/request consumers.
- `crates/hana_diegetic/examples/{aa_text,units}.rs` and other compiler-identified workspace consumers — event/request migration.
- `crates/bevy_lagrange/{README.md,CHANGELOG.md}` — final retained-camera documentation.

**Constraints from prior phases:** Preserve Phase 13 behavior exactly while deleting compatibility. Use Phase 12 fallible move constructors everywhere. Do not retain two runtime playback paths.

**Acceptance gate:** No `PlayAnimation`, `CameraMoveList`, `AnimationReason`, `ZoomContext`, `ZoomReason`, or destructive queue references remain; higher-level requests preserve source/target/conflict semantics; all examples run against retained playback; public breaking changes are documented; workspace build/tests and the full `clippy` skill pass.

### Phase 15 — Hana transport library and progress bindings  · status: todo

#### Work Order

**Goal:** `hana_animation` exposes seekable, directional transport and maps one playhead to any number of externally controlled camera/arrangement sequences.

**Spec:**

- In the sibling Hana workspace, add a production `bevy_kana` dependency pinned to a compatible `bevy_hana` revision containing Phases 1–3 and 12–14. Pin `bevy_lagrange` to that same compatible source revision; update `Cargo.lock`.
- Transport playback owns three independent values: `Playback::{Playing, Paused}`, `SequenceDirection::{Forward, Backward}`, and positive finite nonzero `PlaybackRate`.

```rust
transport.set_playback_rate(PlaybackRate::try_new(2.0)?);
transport.play_backward();
transport.pause();
transport.resume();
transport.play();
transport.try_seek(seconds)?;
```

  Default is playing forward at 1x. Play methods select absolute direction without resetting rate. Pause retains direction/rate; resume restores them. `try_seek()` changes only loop-relative playhead, zeros update delta, and preserves playback, direction, rate, monotonic elapsed, frame, and beat.
- Make controlled transport fields private and expose read-only accessors/methods. Mapping avoids overflow before interpolation. Keep a temporary deprecated toggle shim only so the Hana binary remains green until Phase 16; remove it there.
- Automatic playback uses scaled real-time delta. Forward adds and backward subtracts only loop-relative `position.seconds`. `delta_seconds`, `elapsed_seconds`, beat, and frame remain monotonic; paused transport advances nothing and records zero delta.
- Add:

```rust
pub struct TransportBinding {
    pub range: TransportRange,
}

let forward = TransportRange::try_span(2.0, 6.0)?;
let backward = TransportRange::try_span(6.0, 2.0)?;
let instant = TransportRange::try_instant(4.0)?;
```

  A binding is colocated with the controlled entity and has no target entity. Span maps first endpoint to 0, second to 1, clamps outside, supports descending/negative finite endpoints, and rejects equal/non-finite endpoints. Instant maps below to 0 and at/above to 1. Mapping is linear and applies no easing.
- Binding lifecycle owns both `ExternalAnimationControl` and derived `ExternalAnimationProgress`: add/change calculates before domain evaluation; every transport update refreshes it; missing progress while still bound is repaired on the next producer pass; binding removal removes binding-owned claim/progress and leaves the domain idle at last pose. Adding a binding over direct external progress intentionally replaces that source; removal does not restore it.
- `TransportPlugin` advances transport then updates bindings in `SequencePlaybackSystems::ProduceExternalProgress`; deferred insertion/removal is visible before evaluation in the same frame. A binding without a current sequence is valid/inert; later authoring samples current bound position.
- Preserve direction and signed wrap count long enough for domains to emit every crossed boundary across one or multiple transport wraps before only the final scalar progress remains.

**Files:**

- `/Users/natemccoy/rust/hana/Cargo.toml`, `/Users/natemccoy/rust/hana/Cargo.lock` — compatible pinned dependencies.
- `/Users/natemccoy/rust/hana/crates/hana_animation/Cargo.toml` — production `bevy_kana` dependency.
- `/Users/natemccoy/rust/hana/crates/hana_animation/src/{lib,plugin,transport}.rs` — transport API and schedule.
- `/Users/natemccoy/rust/hana/crates/hana_animation/src/binding.rs` — range/binding lifecycle and progress production.
- `/Users/natemccoy/rust/hana/crates/hana_animation` tests — controls, mapping, lifecycle, wraps, scheduling.

**Constraints from prior phases:** Pin revisions that include Phase 3 external components/system sets and Phase 14 retained cameras. `hana_animation` maps transport only; `bevy_kana`, `hana_valence`, and `bevy_lagrange` must not depend on it.

**Acceptance gate:** Package tests cover rate validation, explicit controls, seek invariants, forward/backward/unbounded/repeating playback, ascending/descending/instant/negative ranges, many bindings, add/change/remove/repair/takeover lifecycle, same-frame production, inert binding, and signed multi-wrap preservation; `cargo check`/nextest for `hana_animation` pass and the full Hana-repo `clippy` skill passes with the temporary binary shim.

### Phase 16 — Hana binary transport integration  · status: todo

#### Work Order

**Goal:** the Hana binary uses explicit transport controls and one scrubber to drive retained camera and arrangement sequences through production bindings.

**Spec:**

- Compose the new `TransportPlugin`/shared sequence sets with retained `bevy_lagrange` camera playback and `hana_valence` arrangement playback using the exact pinned sources from Phase 15.
- Replace transport toggle behavior with explicit pause/resume actions. UI/gesture code owns whether scrubbing pauses and later resumes; the transport API does not infer gesture policy.
- Connect repeated scrubber updates to `Transport::try_seek()`. Bind camera and arrangement controller entities through colocated `TransportBinding` values; demonstrate synchronized sampling, forward/backward range mapping, and no registry/target list.
- Migrate Hana camera flyover/editor code from `PlayAnimation` to valid `CameraMove`, retained `CameraSequence`, and `CameraCommand`.
- Remove the temporary `toggle_playback()` shim from `hana_animation` and migrate all Hana examples/consumers.
- Build and exercise the Hana binary against the exact integrated sources. Confirm producer/evaluator ordering, writer ownership, backward/forward scrubbing, and loop crossings at runtime; do not use `Hana` as shorthand for a library in docs/code comments.

**Files:**

- `/Users/natemccoy/rust/hana/crates/hana_animation/src/transport.rs` — remove compatibility toggle.
- `/Users/natemccoy/rust/hana/crates/hana/Cargo.toml` — integrated dependencies/features.
- `/Users/natemccoy/rust/hana/crates/hana/src/{main,transport}.rs` — plugin composition and scrubber binding.
- `/Users/natemccoy/rust/hana/crates/hana/src/input/{mod,global_shortcuts}.rs` — explicit controls.
- `/Users/natemccoy/rust/hana/crates/hana/src/camera/{editor_camera,flyover}.rs` — retained camera API.
- Compiler-identified Hana examples/consumers using transport toggle or queue camera APIs — migration.

**Constraints from prior phases:** Use Phase 15 transport/bindings and exact dependency pins; use Phase 14 camera API and Phase 11 fold API. The Hana binary composes libraries but does not own their internal sequence state.

**Acceptance gate:** No transport toggle or queue camera API remains in the Hana workspace; scrub seeks both directions without changing monotonic clocks; camera and arrangement bindings stay synchronized; loop crossings produce complete domain events; Hana package/workspace build/tests and the full Hana-repo `clippy` skill pass.

### Phase 17 — Hana Valence provider and folding examples  · status: todo

#### Work Order

**Goal:** runnable Hana Valence examples demonstrate the small ordinary API plus downstream provider, recipe, direct-authoring, timing, and event extension paths.

**Spec:**

- Rewrite the triangle and staggered examples around `TriangleSheet`, typed selections, `spawn_arrangement()`, Accordion/Coil/Wrap, `with_fold_sequence()`, and every `FoldCommand`.
- Rewrite the box example as an example-defined `BoxNet` provider and a multi-step lid/wall sequence. Use stage/member timing to demonstrate snap, bounce, simultaneous walls, later lid movement, pause/resume, backward play, interior reversal, and exact fold events that start a secondary effect.
- Add example-defined `HexSheet`, no-fold decorative provider, and one downstream custom `FoldRecipe`/provider-input type. Demonstrate alternative selections and `Provides<C>` without adding these as core APIs.
- Cover `spawn_arrangement()` with code-authored BSN and `()` scene, `spawn_arrangement_from_members()` with both `MemberBinding` states, and advanced direct `AnchoredTo` plus baseline `Hinge` authoring.
- Cover functional group combination/subdivision/reordering/omission; sequence/stage/member timing inheritance/override; repeated members; `FoldTarget`; simultaneous, sequential, staggered, wave, snap, and finite easing overshoot.
- Examples stay presentation-light: no panel menus, keyboard-repeat UI, camera fitting, or Hana Diegetic unit conversion belongs in `hana_valence`.

**Files:**

- `crates/hana_valence/examples/{box,staggered_unfold,triangles}.rs` — primary examples.
- Additional focused `crates/hana_valence/examples/*.rs` only where one of the downstream extension/direct-authoring cases cannot remain readable in the three existing examples.
- `crates/hana_valence/tests/arrangements.rs` — executable assertions supporting example-only behavior.
- `crates/hana_valence/{README.md,CHANGELOG.md}` — example index and migration call sites.

**Constraints from prior phases:** Use only final Phase 6–11 public APIs; no compatibility types removed by those phases. Follow the primary-API-first example layout convention and keep decorative support below the core call flow.

**Acceptance gate:** Each required arrangement/provider/recipe/sequence/direct-authoring option appears in at least one runnable example; examples include quad/triangle plus downstream hex/box/no-fold/custom recipe scenarios; all examples build/run, workspace tests pass, and the full `clippy` skill passes.

### Phase 18 — Panel-anchoring migration  · status: todo

#### Work Order

**Goal:** the former Hana Diegetic panel-anchoring proof of concept delegates reusable arrangement/folding behavior to Hana Valence while retaining only presentation-specific UI locally.

**Spec:**

- Replace local member indexing, connection/fold drivers, Strip/Accordion/Coil components, direct mutable hinge angles, and local playback assumptions with final `ArrangementProvider`, typed fold groups, transient recipes, retained `FoldSequence`, and `FoldCommand` APIs.
- Preserve the useful behaviors: alternate accordion hinging over compatible group lines; physically calibrated exterior wrap; coil behavior as a same-signed relative fold; forward/backward play; wrap/unwrap; group/stage timing; and runtime recipe application only at the shared base endpoint.
- Keep application/presentation responsibilities in the example: keyboard bindings, repeat timing, highlighted controls, capability menus, labels, colors/borders, camera presets/fitting, viewport handling, unit conversion, and panel geometry publication.
- Use Hana Valence construction with BSN member scenes rather than maintaining a second arrangement-member list. The example may define its own provider/capability implementation when panel geometry supplies information not appropriate for a core provider.
- Demonstrate external progress and native playback without competing writers. Recipe switching reports base-only conflicts instead of silently mutating active hinges.
- Remove obsolete panel arrangement helpers only after all example behavior is reproduced; update module names/comments to distinguish `hana_valence` library ownership from application presentation.

**Files:**

- `crates/hana_diegetic/examples/panel_anchoring/{main,scene,anchor_demo,hinge,presentation,menu,info_panel,constants}.rs` — final API migration and presentation split.
- `crates/hana_diegetic/src/panel/{anchoring,mod}.rs` — retain geometry publication; remove arrangement behavior that moved to Hana Valence.
- `crates/hana_diegetic/Cargo.toml` — final features/dependencies if required.
- `crates/hana_valence/tests/arrangements.rs` — reusable behavior tests extracted from the proof of concept.
- `crates/hana_valence/CHANGELOG.md`, `crates/hana_diegetic/CHANGELOG.md` if present — ownership/migration notes.

**Constraints from prior phases:** Use final provider/recipe/sequence APIs from Phases 6–11 and retained camera APIs from Phase 14. Do not move presentation-only behavior into `hana_valence`.

**Acceptance gate:** The demonstration retains accordion, coil, exterior wrap, recipe switching, native controls, and external progress while no longer defining duplicate arrangement/fold machinery; reusable behavior is tested in Hana Valence; the example runs; workspace build/tests and the full `clippy` skill pass.

### Phase 19 — Coverage and migration closure  · status: todo

#### Work Order

**Goal:** every public option and breaking change is executable, documented, integrated across both workspaces, and ready for as-built distillation.

**Spec:**

- Audit every public fallible constructor: add one success test and exact-variant coverage for every documented error. For erased selection/capability/recipe/assignment failures before writes, assert both exact cause and unchanged hinges. Do not add transactional expectations after developer-owned ECS state is despawned/repurposed.
- Complete reflection tests proving invalid dynamic values cannot patch opaque math, progress, geometry, sequence, hinge, or camera authoring values.
- Complete cross-domain examples/tests for direct external progress, optional tween forward/backward/partial ranges and overshoot, transport bindings with several entities/ranges/seeks/loops/rate/direction, retained camera commands/input policy/events, and fold/camera same-frame producer ordering.
- Sweep public inventory and remove stale exports/references for every approved removal: arrangement indices/placements/tiling/Strip; fold membership/snapshot/actuation/lenses/pivots; destructive camera queue/PlayAnimation; old transport toggle.
- Update `crates/bevy_kana/CHANGELOG.md`, `crates/hana_valence/CHANGELOG.md`, and `crates/bevy_lagrange/CHANGELOG.md` with all public breaking changes and migration paths. Update crate READMEs and example docs to final names/signatures. Keep [future-work.md](future-work.md) as the home for explicitly deferred diagnostics, closed-loop constraints, richer builders, recipe policies, and other non-V1 work.
- Verify compatible source pins in `/Users/natemccoy/rust/hana`, then build/test both workspaces and exercise the Hana binary against the exact revisions. Record any necessary public API migration in the sibling Hana workspace rather than leaving compatibility shims.
- Do not convert this plan to an as-built document in this phase. Once all phases are `done`, run `/plan:to_as_built` with the stated amend disposition so the shipped behavior updates both existing Hana Valence as-built docs.

**Files:**

- All test/example/README/CHANGELOG files named in Delegation Context — final coverage and stale-reference sweep only.
- `crates/bevy_kana/src/{lib,prelude}.rs`, `crates/hana_valence/src/lib.rs`, `crates/bevy_lagrange/src/lib.rs` — final public export audit.
- `/Users/natemccoy/rust/hana/Cargo.toml`, `/Users/natemccoy/rust/hana/Cargo.lock` — final compatible pins.
- `docs/hana_valence/future-work.md` — preserve explicit non-V1 routing if cross-links change.

**Constraints from prior phases:** Treat Phases 1–18 as the final implementation surface; close gaps without reopening settled V1 design or adding a diagnostic engine. Preserve exact source pins and remove all temporary compatibility shims.

**Acceptance gate:** The complete public-option matrix is executable; every fallible constructor/error and no-write boundary is tested; no approved removed API remains; all examples compile and required runnable examples execute; both workspace build/nextest commands and both full `clippy` workflows pass; the Hana binary is exercised against exact pinned sources; the plan is ready for `/plan:to_as_built`.
