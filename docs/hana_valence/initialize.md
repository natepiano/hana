# hana_valence — implementation plan

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Extracts bevy_diegetic's
> anchoring machinery into the standalone shape-agnostic `hana_valence` crate
> (geometry contract, attachment relation, resolver, hinge, arrangements), then
> re-points bevy_diegetic as one provider of anchor geometry.

## Delegation Context

- **Project:** `hana_valence` — a new shape-agnostic workspace crate ("shapes expose connection points and bond into animatable assemblies"), extracted from `bevy_diegetic`'s anchoring machinery. Workspace root `/Users/natemccoy/rust/bevy_hana` (repo "hana", `natepiano/hana`). Members declared by **glob**: root `Cargo.toml:3` `members = ["crates/*"]` (with `exclude = ["crates/hana", "vendor/clay-layout"]`) — a crate at `crates/hana_valence` is auto-registered as a member, no edit needed. To be consumed by `bevy_diegetic` it must be a `[workspace.dependencies]` entry, which **already exists**: root `Cargo.toml:43` `hana_valence = { path = "crates/hana_valence" }`. The crate dir also **already exists** (`crates/hana_valence/` with `Cargo.toml`, `src/lib.rs` = just `//! hana_valence`, README, LICENSEs, CHANGELOG). Model small crate for the inherited-Cargo pattern: `crates/bevy_lagrange/Cargo.toml` (and `crates/bevy_liminal/Cargo.toml`) — both use `authors.workspace = true`, `edition.workspace = true`, `license.workspace = true`, `repository.workspace = true`, and `[lints] workspace = true`. The existing `crates/hana_valence/Cargo.toml` already follows this but currently pulls full `bevy.workspace = true` (violates the dep-surface invariant; Phase 1 narrows it).
- **Stack:** Rust **edition 2024** (`[workspace.package] edition = "2024"`); Bevy **0.19.0** (`[workspace.dependencies] bevy = { version = "0.19.0", default-features = false }`). `bevy_tween` is **NOT** a workspace dependency (absent from root Cargo.toml entirely) — Phase 5 introduces it. The workspace depends on **full `bevy`** (default-features=false, features selected per-crate); it does **NOT** expose `bevy_ecs` / `bevy_transform` / `bevy_math` / `bevy_platform` as separate `[workspace.dependencies]` — Phase 1 adds those subcrate entries (today code reaches them via `bevy::ecs`, `bevy::platform::collections`, etc.). Strict workspace lints: clippy `all`/`cargo`/`nursery`/`pedantic` = deny, plus `unwrap_used`/`expect_used`/`panic`/`unreachable` = deny, and `missing_docs = "deny"` — every pub item needs docs; fallible paths return `Result`, never panic.
- **Layout:**
  - `crates/hana_valence/` — the new crate (already scaffolded; `src/lib.rs` is a stub). Phases add `src/geometry.rs`, `src/relation.rs`, `src/pose.rs`, `src/attachment.rs`, `src/resolve.rs`, `src/hinge.rs`, `src/tween.rs`, `src/arrange.rs`, `examples/`.
  - `crates/bevy_diegetic/src/panel/` — anchoring source: `attachment_resolver.rs`, `world_anchoring.rs`, `anchoring.rs`, `anchor_geometry.rs`, `gizmos.rs`, `mod.rs` (`PanelSystems`).
  - `crates/bevy_diegetic/src/screen_space/anchoring/` — `mod.rs`, `candidate.rs`, `placement.rs`, `projection.rs`, `rect.rs`, `resolve.rs`, `window.rs` (screen placer that consumes the shared skeleton).
  - `crates/bevy_diegetic/examples/` — flat `.rs` examples plus the `panel_anchoring/` subdir (multi-file example). `diegetic_text_stress.rs` also uses anchoring.
  - Integration tests live in `crates/bevy_diegetic/tests/` (only `trybuild.rs` + `trybuild/`); all anchoring unit tests are `#[cfg(test)]` modules **inside** the `src/` files.
- **Key files:**
  - `crates/bevy_diegetic/src/panel/attachment_resolver.rs` — generic attachment skeleton (all `pub(crate)`). `AttachmentResolveCandidate<R>` enum L14 (`Active`/`Skipped`, hard-codes `AnchoredToPanel` payload → becomes `AnchoredTo` on port); `AttachmentResolveAction` enum L39 (`Place`/`Fallback`); `resolve_panel_attachments<R,F>` L51 (topological, callback `F: FnMut(AttachmentResolveAction)->Result<(),R>`); `AttachmentGraph` struct L88 + impl L95; `AttachmentResolveDiagnostics<R>` struct L285, impl L291, `Default` L329. Imports: `super::AnchoredToPanel` (L10), `bevy::platform::collections::{HashMap,HashSet}` (L6-7), and **`bevy::prelude::*`** (L8) — the prelude imports must de-prelude on port.
  - `crates/bevy_diegetic/src/panel/world_anchoring.rs` — world resolver. Main system `resolve_world_space_panel_attachments` L50; `restore_inactive_world_panel_poses` L27; `placement` L95 / `desired_local_transform` L147 (calls `validate_supported_parent_transform`); `classify_candidates` L170 / `classify_candidate` L185 / `validate_candidate` L213; `place_world_attachment` L247 / `world_anchor_placement` L270; `target_anchor_point` L342 (the **Y-flip**: `right*offset.x − up*offset.y + normal*offset.z`, L350-351); `target_offset_meters` L355 (unit resolution); `plane_frame_translation` L378 (pose-translation in plane basis); `scaled_source_anchor_offset` L382 (`* source_scale`, L394); `plane_rotation` L397; `anchor_offset` L401; `validate_supported_parent_transform` L436 (rejects non-uniform parent scale). Source scale finiteness check L119-120.
  - `crates/bevy_diegetic/src/panel/anchoring.rs` — relationship pair + pose/offset. `AnchoredToPanel` struct L27 (derives/attrs L23-30: `#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]`, `#[component(immutable)]`, `#[reflect(PartialEq, Debug, FromWorld, Clone)]`, `#[relationship(relationship_target = PanelsAnchoredHere)]`, and on the `target` field `#[relationship] #[entities] #[reflect(ignore, default = "placeholder_entity")]`); `impl FromWorld for AnchoredToPanel` L71; `PanelAnchorPose` struct L86 (`#[reflect(Component, PartialEq, Debug, Default)]` L85); `PanelAnchorOffset` struct L112 (three `Dimension`s, impl L123); `PanelsAnchoredHere(Vec<Entity>)` L174 (`#[relationship_target(relationship = AnchoredToPanel)]` L173, `#[reflect(FromWorld, Default)]` L172).
  - `crates/bevy_diegetic/src/layout/units/anchor.rs:8` — the `Anchor` enum (TopLeft…Center authoring vocabulary).
  - `crates/bevy_diegetic/src/panel/anchor_geometry.rs` — geometry provider (computed on demand, not stored). `PanelAnchorGeometryParam` SystemParam struct L34, impl L48, `.get()` L56 (returns `Result<ResolvedPanelAnchorGeometry, PanelAnchorGeometryError>`); `ResolvedPanelAnchorGeometry` struct L109, impl L113 (`from_screen_panel`/`from_world_panel`); `PanelAnchorPoints` L170, `PanelAnchorPoint` L225, `PanelAnchorEdgeEndpoints` L254, `PanelAnchorEdge` L283, `PanelScreenBounds` L296, `PanelPlane` L367 (`point(Anchor)`, `right/up/normal` basis), `PanelAnchorGeometryError` L486.
  - `crates/bevy_diegetic/src/screen_space/anchoring/resolve.rs` — screen placer. `AnchorResolveDiagnostics` type alias L20 (`= AttachmentResolveDiagnostics<AnchorResolveSkip>`; re-exported `mod.rs:12`); calls `panel::resolve_panel_attachments` L61 (the callback seam); consumes `AttachmentResolveCandidate` L14/L49-52. `candidate.rs` L15 `classify_candidates` (→ `AttachmentResolveCandidate<AnchorResolveSkip>`, classification stays diegetic). `placement.rs` L105-115 is the `Place`/`Fallback` callback body + `AttachmentResolveReasons` L210. `projection.rs`/`rect.rs`/`window.rs` are viewport/window math (no skeleton refs).
  - `PanelSystems` enum — `crates/bevy_diegetic/src/panel/mod.rs:124`; `AnimateAnchorPose` variant L145; also referenced `world_anchoring.rs:1210`.
  - Named tests: `world_anchoring_respects_source_scale_and_parent_rotation` → `world_anchoring.rs:959`; `pose_written_in_animation_set_lands_this_frame` → `world_anchoring.rs:1205`; `insert_replace_and_remove_update_reverse_index` → `anchoring.rs:241`; `anchor_types_are_registered_with_expected_reflect_component_data` → `anchoring.rs:323`; `point_offsets_resolve_to_screen_pixels` → `screen_space/anchoring/resolve.rs:798`.
  - Example: `crates/bevy_diegetic/examples/panel_anchoring/` (multi-file). `hinge.rs` — `FoldPattern` enum L49 (`Accordion` L51, `Coil` L53), `HingeChain` L103, `crease_sign` L257 (Accordion alternation), `Quat::from_rotation_x` L330 (the per-shape fold the resolver generalizes to `from_axis_angle`). `anchor_demo.rs:828` `collect_tiles_by_order` (assumes contiguous orders), called from `scene.rs:299` and `anchor_demo.rs:794/904/1129`.
  - `crates/bevy_diegetic/examples/diegetic_text_stress.rs` — uses `Anchor` L53, `AnchoredToPanel` L54, `PanelAnchorOffset` L65.
- **Build:** `cargo build && cargo +nightly fmt` after changes (CI: `cargo build --release --workspace --all-features --examples`, format check `cargo +nightly fmt -- --check`, plus `taplo fmt --check` for TOML and `cargo mend --fail-on-warn`).
- **Test:** `cargo nextest run` (CI runs `cargo nextest run --all-features --workspace --tests`). No `.config/nextest.toml` exists — nextest is the runner convention, no committed config.
- **Lint:** the `clippy` skill.
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_hana` (verified present). No repo-local `docs/style/` overlay.
- **Invariants:**
  - Dep surface: `hana_valence` depends only on `bevy_ecs + bevy_transform + bevy_math + bevy_platform`, never full `bevy` and never `bevy_app` (so the crate exposes systems + sets, **no `Plugin` type** — consumers register). Gate: `cargo check -p hana_valence --no-default-features` stays green. Dev-dependencies (examples) may use full `bevy`.
  - Sole-Transform-writer: `resolve_anchors` is the only `Transform` writer for entities carrying `AnchoredTo`; drivers write `Transform` only on un-anchored entities (fly-together before the relation is added).
  - Double-writer guard: through Phases 1–6 the valence systems are **never registered into a diegetic-linked app** (unit-test schedules only); Phase 7's swap (delete diegetic world resolver + diagnostics ⇔ register provider + valence resolver + diagnostics) is atomic in one change.
  - Compile continuity: Phases 2–3 **copy** the skeleton + resolver into valence (the diegetic originals keep compiling for the screen placer); consumers switch and diegetic copies delete only in Phase 7. Every phase lands compiling and green.
  - `publish = false` on `crates/hana_valence/Cargo.toml` until the API is intended to ship.
  - Reflection split: `ReflectComponent` only on the mutable input components (`AnchorPose`, `Hinge`, `ResolvedAnchorGeometry`, `ResolvedAnchorOffset`, `ResolvedAnchorWorld`); the relationship pair (`AnchoredTo`/`AnchoredHere`) registers the Reflect *type* only — reflection insert/apply bypasses relationship hooks and would corrupt `AnchoredHere`. Mirror today's `anchor_types_are_registered_with_expected_reflect_component_data` split.
  - Provider write discipline: geometry-fill systems filter on `Changed<DiegeticPanel>` (or the provider's own data component), never `Changed<Transform>` (the resolver writes `Transform` every frame on exactly the anchored set — that filter self-triggers); mutate `ResolvedAnchorGeometry` in place (stable keys, retained allocation), insert only when absent. Precondition: providers emit **centered-local points in authored units with no transform baking**; the resolver applies `GlobalTransform` including scale.
  - Preserve child scale (`− rot * (child_global_scale ⊙ source_local)`) and parent-scale rejection (`validate_supported_parent_transform`) exactly; keep `point_offsets_resolve_to_screen_pixels` (DPI Pt→px) green; unit/DPI/size offset resolution stays diegetic-side (lowering), never in valence.
  - API boundary (compose, not re-export): no hana_valence type ever appears in a bevy_diegetic **public signature** (function returns, pub fields, trait bounds). Diegetic sugar is insert-only bundles with private fields; users who drive poses import `hana_valence` directly. Writing valence components from internal diegetic systems is the intended protocol.
  - `AnchoredHere` is `Vec<Entity>`: insertion order is the deterministic resolve + arrangement order. Never swap it for a set type.
  - Repo hygiene: after changes run `cargo build && cargo +nightly fmt` (and `taplo fmt` for TOML edits, unsandboxed); never commit or push without being asked.

## Phases

### Phase 1 — crate scaffold + core contract types  · status: todo

#### Work Order

**Goal:** `hana_valence` compiles standalone on the narrow dep surface with the complete core type contract, fill-time validation, and the reflection-registration split, proven by unit tests.

**Spec:**

Cargo setup:
- Add to root `Cargo.toml` `[workspace.dependencies]`: `bevy_ecs`, `bevy_transform`, `bevy_math`, `bevy_platform`, all version `0.19.0`, `default-features = false`, with the features the derives need (`bevy_ecs` needs its reflect feature for `Reflect`/`ReflectComponent`; `bevy_math` needs its reflect feature for `Vec3`/`Quat` reflection; enable only what compiles — the gate is `cargo check -p hana_valence --no-default-features`).
- Rewrite `crates/hana_valence/Cargo.toml`: drop `bevy.workspace = true`; depend on the four subcrates; add `publish = false`; keep the workspace-inherited fields and `[lints] workspace = true` (model: `crates/bevy_lagrange/Cargo.toml`).

Core types (module layout: `src/geometry.rs`, `src/relation.rs`, `src/pose.rs`, re-exported from `lib.rs`):

```rust
// geometry.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Reflect)]
#[non_exhaustive]              // a future Face(u32) must not silently break provider matches
pub enum AnchorId {
    Vertex(u32),
    EdgeMid(u32),
    #[default]                 // Default is REQUIRED: the relationship derive constructs
    Center,                    // non-relationship fields via Default::default()
}

#[derive(Clone, Copy, Debug, Default, Reflect)]
pub struct AnchorPoint {
    pub position: Vec3,         // local frame, centered, authored units — no transform baking
    pub frame:    Option<Quat>, // per-anchor tangent frame; None = entity frame (flat providers)
}
impl AnchorPoint {
    // Sole owner of the default — consumers never inline unwrap_or(IDENTITY).
    pub fn rotation(&self) -> Quat { self.frame.unwrap_or(Quat::IDENTITY) }
}

// No Default for Edge: Vertex(0)→Vertex(0) is a degenerate default.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
pub struct Edge { pub start: AnchorId, pub end: AnchorId }
impl Edge {
    // axis = end − start, order-significant: swapping endpoints flips fold sign.
    pub fn axis(&self, geom: &ResolvedAnchorGeometry) -> Result<Dir3, EdgeAxisError> { … }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeAxisError { MissingAnchor(AnchorId), Degenerate, FrameDivergent }

#[derive(Component, Reflect)]
#[reflect(Component)]           // mutable input: full ReflectComponent
pub struct ResolvedAnchorGeometry {
    pub points: HashMap<AnchorId, AnchorPoint>,   // bevy_platform HashMap
    pub edges:  Vec<Edge>,
}

// relation.rs — mirror the attribute pattern of today's AnchoredToPanel
// (anchoring.rs:23-30) exactly, including FromWorld + placeholder-entity default.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[component(immutable)]
#[reflect(PartialEq, Debug, FromWorld, Clone)]   // type-only registration — NO ReflectComponent
#[relationship(relationship_target = AnchoredHere)]
pub struct AnchoredTo {
    #[relationship]
    #[entities]
    #[reflect(ignore, default = "placeholder_entity")]
    target:            Entity,   // private; pub fn target(&self) -> Entity accessor
    pub source_anchor: AnchorId,
    pub target_anchor: AnchorId,
    pub offset:        Vec3,     // authored static, raw resolver-frame units
}
// Doc on the type: #[component(immutable)] makes retargeting a full re-insert
// (on_replace + on_insert re-fire, reverse index updates); provide a
// retargeted()-style full-value constructor, never partial mutation.

#[derive(Component, Debug, Default, Reflect)]
#[reflect(FromWorld, Default)]                   // type-only — NO ReflectComponent
#[relationship_target(relationship = AnchoredTo)]
pub struct AnchoredHere(Vec<Entity>);            // insertion order = resolve/arrangement order

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct ResolvedAnchorOffset(pub Vec3);       // mutable per-frame override; the resolver
                                                 // prefers it over AnchoredTo.offset

// pose.rs
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Default)]
pub struct AnchorPose { pub rotation: Quat, pub translation: Vec3 }
// Doc on the type: AnchorPose is deliberately NOT Transform — it is the
// local-frame animation seam the resolver converts; merging them would make
// animators and the resolver race on one component.

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct ResolvedAnchorWorld { pub points: HashMap<AnchorId, Vec3> }
// Opt-in cache for gizmos/UI; Phase 3's resolver recomputes it each frame
// for entities that carry it, so it cannot go stale.

#[derive(SystemSet, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum AnchorSystems {
    FillGeometry, // providers write ResolvedAnchorGeometry here
    AnimatePose,  // drivers write AnchorPose / Hinge.angle / Transform here
    Resolve,      // resolve_anchors reads geometry + relation + pose, writes Transform
}
// FillGeometry and AnimatePose both run before Resolve; consumers configure
// the sets (valence exposes no Plugin — see Delegation Context dep-surface rule).
```

Fill-time validation (always available, provider-called):
- `ResolvedAnchorGeometry::validate(&self) -> Result<(), GeometryError>`: every `Edge` references two existing, distinct points; all positions finite; frame-divergent edges rejected (`GeometryError` variant — an edge is valid only when both endpoint frames are equal or both `None`; for flat providers this is trivially true). Debug-assert frames are normalized (a non-unit provider quat silently scales/shears every downstream transform).
- `Edge::axis` returns `Err(MissingAnchor)` / `Err(Degenerate)` (`|end−start| < ε`) / `Err(FrameDivergent)` rather than NaN axes.
- Optional authoring helper trait `AnchorGeometry { fn anchor_point(&self, id: AnchorId) -> Option<Vec3>; }` may exist for providers to compute points internally — it is authoring sugar, never a resolver dispatch point.

`lib.rs`: crate overview docs (shapes expose anchor points and bond; the contract is a component, not dyn dispatch; one `ResolvedAnchorGeometry` per entity — never a global table) plus a registration snippet (marked `ignore`) showing a consumer configuring `AnchorSystems` in `PostUpdate` before `TransformSystems::Propagate`.

Tests (in-file `#[cfg(test)]`, run via `World` + `Schedule` — no `App`):
- Reverse-index maintenance: insert / replace / remove `AnchoredTo` updates `AnchoredHere` (mirror `anchoring.rs:241` `insert_replace_and_remove_update_reverse_index`).
- Reflection-registration split test (mirror `anchoring.rs:323`): mutable inputs expose `ReflectComponent`; `AnchoredTo`/`AnchoredHere` are registered types without `ReflectComponent`.
- Validation: missing-anchor edge, degenerate edge, non-finite point, frame-divergent edge each produce the right error; a valid flat-quad geometry passes.
- `AnchorPoint::rotation()` returns identity for `None`, the quat for `Some`.

**Files:**
- `Cargo.toml` (workspace root) — add the four subcrate `[workspace.dependencies]` entries.
- `crates/hana_valence/Cargo.toml` — narrow deps, `publish = false`.
- `crates/hana_valence/src/lib.rs` — module wiring + crate docs.
- `crates/hana_valence/src/geometry.rs`, `src/relation.rs`, `src/pose.rs` — new.
- Read-only reference: `crates/bevy_diegetic/src/panel/anchoring.rs` (attribute pattern, FromWorld impl, both mirrored tests).

**Constraints from prior phases:** none (first phase).

**Acceptance gate:** `cargo build` green; `cargo check -p hana_valence --no-default-features` green; `cargo nextest run -p hana_valence` green with the four test groups above; `cargo +nightly fmt`; the `clippy` skill clean.

---

### Phase 2 — attachment skeleton port (copy)  · status: todo

#### Work Order

**Goal:** The generic attachment skeleton (topological ordering, fallback dispatch, diagnostics) lives in `hana_valence` with an `AnchoredTo` payload, unit-tested; the diegetic original is untouched.

**Spec:**

Copy `crates/bevy_diegetic/src/panel/attachment_resolver.rs` into `crates/hana_valence/src/attachment.rs` (a **copy** — the diegetic file keeps compiling for the screen placer until Phase 7), with these changes:
- Payload re-type: `AttachmentResolveCandidate::Active` / `AttachmentResolveAction::Place` / `AttachmentGraph` carry `AnchoredTo` (Phase 1) instead of `AnchoredToPanel`. No extra `<A>` payload generic — `AnchoredTo` is the one stored relation both placers will read. The payload's `AnchorId` fields are simply unused by consumers that speak their own vocabulary.
- Rename `resolve_panel_attachments` → `resolve_attachments` (nothing panel-shaped remains).
- De-prelude: replace `bevy::prelude::*` / `bevy::platform::…` imports with granular `bevy_ecs` / `bevy_math` / `bevy_platform` paths (the no-default-features gate fails otherwise).
- Visibility: `pub` (was `pub(crate)`), with docs on every item (`missing_docs` is deny).
- Keep the name `AttachmentResolveDiagnostics<R>` exactly — it is already the one generic diagnostics type; diegetic's `WorldAnchorResolveDiagnostics` and screen `AnchorResolveDiagnostics` are aliases of it, and keeping the name means no rename anywhere and no cross-crate collision.
- Keep the callback shape: `resolve_attachments<R, F>` with `F: FnMut(AttachmentResolveAction) -> Result<(), R>`; candidates arrive **pre-classified** (`Active`/`Skipped`) — classification is the consumer's job, the skeleton owns ordering, dispatch, and diagnostics. Add warn-on-repeated-skip logging so a consumer can see why an attachment isn't moving.

Tests: chain ordering (3-level chain resolves parent-before-child in one pass), fallback dispatch (a `Skipped` candidate routes to `Fallback` with its reason recorded), diagnostics accumulation across frames, missing-target behavior preserved from the original.

**Files:**
- `crates/hana_valence/src/attachment.rs` — new (ported copy).
- `crates/hana_valence/src/lib.rs` — module + re-exports.
- Read-only source: `crates/bevy_diegetic/src/panel/attachment_resolver.rs` (L14 candidate enum, L39 action enum, L51 resolve fn, L88 graph, L285 diagnostics).

**Constraints from prior phases:** Phase 1 provides `AnchoredTo` (fields `target` private + `target()` accessor, `source_anchor`, `target_anchor`, `offset: Vec3`) in `crates/hana_valence/src/relation.rs`; `bevy_platform` HashMap/HashSet are workspace deps.

**Acceptance gate:** `cargo build` green; `cargo nextest run -p hana_valence` green including the four new skeleton tests; no-default-features check green; diegetic untouched (`git diff --stat` shows no `bevy_diegetic` changes); fmt + `clippy` skill clean.

---

### Phase 3 — `resolve_anchors` resolver  · status: todo

#### Work Order

**Goal:** The shape-agnostic resolver places anchored children from geometry + relation + pose, proven by the standalone two-quad test and ports of the existing green world-anchoring tests.

**Spec:**

`src/resolve.rs`: system `resolve_anchors`, the sole `Transform` writer for entities with `AnchoredTo`. The math (final, review-verified):

```
target_world = parent.global * parent.geometry[target_anchor].position
source_local = child.geometry[source_anchor].position
base         = parent.global.rotation * target_point.rotation()  // rotation() = frame.unwrap_or(IDENTITY)
rot          = base * pose.rotation * source_point.rotation().inverse()
                                    // seats the child's source frame onto the target frame;
                                    // flat children (frame None) reduce to base * pose.rotation
offset_eff   = resolved_anchor_offset.unwrap_or(anchored_to.offset)
                                    // mutable per-frame override, else the authored static
child.translation = target_world + base * (offset_eff + pose.translation)
                  − rot * (child_global_scale ⊙ source_local)     // child scale preserved
child.rotation    = rot
```

Invariants baked into the math:
- `offset_eff` and `pose.translation` apply in the target-anchor frame (`base`), independent of `pose.rotation` — the static offset does NOT rotate with the pose (matches today's plane-basis behavior, `plane_frame_translation` at `world_anchoring.rs:378`).
- The pin holds because the same `rot` appears in the `− rot * (…)` term and `child.rotation`.
- `source_point.rotation().inverse()` is a clean inverse only for unit quats — Phase 1's normalized-frame validation backs it.
- The child-scale factor sits inside the rotated term (today: `scaled_source_anchor_offset` × `source_scale`, `world_anchoring.rs:382-394`); uniform child scale is supported, non-uniform child scale is documented unsupported.
- No Y-flip and no unit resolution here: `offset` is raw resolver-frame units; unit/DPI/size lowering is diegetic-side (Phase 7).

Mechanics:
- Read parent `GlobalTransform` + parent `ResolvedAnchorGeometry`; child `ResolvedAnchorGeometry`, `Option<&AnchorPose>` (default identity), `Option<&ResolvedAnchorOffset>`, child `GlobalTransform` (scale), `&mut Transform`.
- Port `validate_supported_parent_transform` (`world_anchoring.rs:436`): reject non-uniform parent scale with a skip reason. Keep the source-scale finiteness check (L119-120).
- Order + dispatch through Phase 2's `resolve_attachments` skeleton. Define the valence skip reason enum, e.g. `ResolveSkip { MissingSourceGeometry, MissingTargetGeometry, MissingAnchor(AnchorId), DespawnedTarget, UnsupportedParentTransform, NonFiniteScale }`, and expose `AttachmentResolveDiagnostics<ResolveSkip>` as a resource the consumer registers. Skipped entities keep their last/authored `Transform` (fallback), with warn-on-repeated-skip.
- Recompute `ResolvedAnchorWorld` each frame for entities that carry it (opt-in cache, never stale).
- Scheduling contract (docs + test wiring, no `Plugin`): `resolve_anchors` runs in `AnchorSystems::Resolve`; consumers place `Resolve` in `PostUpdate` before `TransformSystems::Propagate`. Document the staleness rule: same-frame `GlobalTransform` reads of anchored entities are one frame stale by design (Resolve writes local; Propagate computes global after).
- **Do not register these systems into any diegetic-linked app** — unit-test schedules only until Phase 7 (double-writer guard).

Tests:
- `two_quads_top_left_to_top_right` — the decoupling proof: two entities, hand-filled `ResolvedAnchorGeometry` (quad W×H: `Vertex(0)=(-W/2,+H/2,0)` TL, `Vertex(1)=(+W/2,+H/2,0)` TR, `Vertex(2)=(+W/2,-H/2,0)`, `Vertex(3)=(-W/2,-H/2,0)`, `EdgeMid(0..4)` at edge midpoints, `Center=(0,0,0)`, 4 perimeter edges), `AnchoredTo { source_anchor: Vertex(0), target_anchor: Vertex(1), .. }`, run the schedule, assert the child `Transform` seats TL on TR. If this test needs any panel type, the layering failed.
- Offset trace: target anchor at world `(3,3,0)`, raw `offset (0.25,−0.5,0)`, identity frames → child anchor lands `(3.25,2.5,0)` (the verified trace; diegetic's Y-flip happens in the Phase 7 lowering, so the raw offset here is pre-flipped).
- Scale + parent rotation port: child global scale 0.5 under a rotated parent reproduces today's green expectation `(1.5,1.25,0)` (mirror `world_anchoring_respects_source_scale_and_parent_rotation`, `world_anchoring.rs:959`).
- Same-frame pose: a system writing `AnchorPose` in `AnimatePose` lands in this frame's `Transform` (mirror `pose_written_in_animation_set_lands_this_frame`, `world_anchoring.rs:1205`).
- Frame seating: target anchor with `frame: Some(q)` → child rotation = `base * pose.rotation`, pin preserved; a child source frame composes out (`rot * source_frame = base * pose.rotation`).
- Wide + deep tree: >10 entities, mixed fan-out and a 4-deep chain, all resolve in one frame in topological order.
- `ResolvedAnchorOffset` override beats `AnchoredTo.offset` when present.

`lib.rs`: add the resolver math trace to the crate docs (the block above, with the offset-trace worked example).

**Files:**
- `crates/hana_valence/src/resolve.rs` — new.
- `crates/hana_valence/src/lib.rs` — module, re-exports, math-trace docs.
- Read-only source: `crates/bevy_diegetic/src/panel/world_anchoring.rs` (system L50, placement L95-147, `target_anchor_point` L342, `target_offset_meters` L355, `plane_frame_translation` L378, `scaled_source_anchor_offset` L382, `validate_supported_parent_transform` L436, tests L959/L1205).

**Constraints from prior phases:** Phase 1 types (`AnchorPoint::rotation()`, `ResolvedAnchorOffset`, `AnchorSystems`, `ResolvedAnchorWorld`) and Phase 2 skeleton (`resolve_attachments`, `AttachmentResolveCandidate<R>` with `AnchoredTo` payload, `AttachmentResolveDiagnostics<R>`) are in place; tests run via `World`+`Schedule`, no `App`/`Plugin` exists in the crate.

**Acceptance gate:** `cargo build` green; `cargo nextest run -p hana_valence` green including all seven named tests above; no-default-features check green; fmt + `clippy` skill clean.

---

### Phase 4 — `Hinge` + `hinge_to_pose`  · status: todo

#### Work Order

**Goal:** Scalar edge-fold works for any shape: `Hinge { edge, angle }` converts to `AnchorPose` via the edge axis, proven by an N-quad accordion test.

**Spec:**

`src/hinge.rs`:

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]                 // mutable input: full ReflectComponent
pub struct Hinge {
    pub edge:  Edge,                  // child-LOCAL endpoints; axis = end − start,
    pub angle: f32,                   // swapping endpoints flips fold sign
}
// Hinge is per-relation: a tile may be a child on one edge and a parent on
// several (polyhedral nets are trees — one Hinge per entity at its single
// AnchoredTo parent edge). angle is a plain f32 deliberately: the easiest
// possible target for any animator.

fn hinge_to_pose(q: Query<(&Hinge, &ResolvedAnchorGeometry, &mut AnchorPose)>) {
    // per entity:
    let axis = match hinge.edge.axis(geom) {
        Ok(axis) => axis,
        Err(reason) => { /* skip + diagnostics (MissingAnchor / Degenerate / FrameDivergent) */ }
    };
    pose.rotation = Quat::from_axis_angle(*axis, hinge.angle);
}   // runs in AnchorSystems::AnimatePose, before Resolve
```

- This is the generalization of the example's `Quat::from_rotation_x` (`panel_anchoring/hinge.rs:330`): a fold is about an **edge**, not "top/bottom" — the only per-shape input is which two anchors form the hinge edge, and that comes from `ResolvedAnchorGeometry.edges`.
- Frame-divergent edges cannot fold (the chord is neither endpoint's tangent): Phase 1's fill-time validation already rejects them from geometry; `Edge::axis` returns `FrameDivergent` as defense-in-depth, feeding diagnostics. For now the invariant is trivially "all frames `None`" (only flat providers exist); a frame-aware axis is a later, additive extension.
- Driver-conflict policy (last-writer-wins): `hinge_to_pose` overwrites `AnchorPose` every frame; to drive `AnchorPose` directly, remove `Hinge`. Document on both types; add a debug-only warning when an entity carries `Hinge` and something else also mutated `AnchorPose` this frame.
- Register `Hinge` with `ReflectComponent` (it is a mutable input).

Tests:
- N-quad straight strip: 5 hand-filled quads chained TL→TR (or edge-mid pairs), each with a `Hinge` on the shared edge; a manual system in `AnimatePose` drives angles; assert the folded transforms after one frame (accordion alternation = alternating angle signs, as `crease_sign` does in the example).
- Degenerate edge (`start == end` positions) skips with a diagnostic, no NaN in any `Transform`.
- Endpoint swap flips fold sign.

**Files:**
- `crates/hana_valence/src/hinge.rs` — new.
- `crates/hana_valence/src/lib.rs` — module + re-exports.
- Read-only reference: `crates/bevy_diegetic/examples/panel_anchoring/hinge.rs` (L49 `FoldPattern`, L257 `crease_sign`, L330 `from_rotation_x`).

**Constraints from prior phases:** `Edge::axis(&geom) -> Result<Dir3, EdgeAxisError>` exists (Phase 1); the resolver consumes `AnchorPose` in `Resolve` after `AnimatePose` (Phase 3); diagnostics type is `AttachmentResolveDiagnostics<R>` (Phase 2).

**Acceptance gate:** `cargo build` green; `cargo nextest run -p hana_valence` green including the three tests above; no-default-features check green; fmt + `clippy` skill clean.

---

### Phase 5 — bevy_tween adapters (`tween` feature)  · status: todo

#### Work Order

**Goal:** bevy_tween drives hinges and poses through shipped lenses, demonstrated by a staggered-unfold example.

**Spec:**

- Add `bevy_tween` as an **optional** dependency behind a new `tween` feature (pick the release compatible with Bevy 0.19; it is not yet a workspace dependency — add it to `[workspace.dependencies]`). The `tween` feature must not violate the no-default-features gate (adapters live in `src/tween.rs` behind `#[cfg(feature = "tween")]`).
- `src/tween.rs`: `HingeAngleLens` (lerp `Hinge.angle`) and `AnchorPoseLens` (slerp `rotation`, lerp `translation`), implementing bevy_tween's lens/interpolator trait for the version chosen. Tweens write in `AnchorSystems::AnimatePose` — document how to order bevy_tween's systems into that set.
- Published animator contract (crate docs): (1) `AnchorPose`, `Hinge.angle`, and `Transform` (un-anchored entities only) are the tweenable targets; (2) write them in `AnchorSystems::AnimatePose`; (3) these lenses are the ready-made adapters. bevy_animation `AnimatableProperty` impls are a possible later feature, not this phase.
- Example `crates/hana_valence/examples/staggered_unfold.rs` (dev-dependencies: full `bevy` with rendering, `bevy_tween`): 5 quads with hand-filled geometry in a strip, each hinge tweened to its target angle with per-entity start delays so the unfold propagates down the chain. Concrete per-entity tween targeting and delay composition — this is the worked proof that "any animator" is real, not a claim.

**Files:**
- `Cargo.toml` (workspace root) — `bevy_tween` workspace dependency.
- `crates/hana_valence/Cargo.toml` — `tween` feature + optional dep + example's dev-dependencies.
- `crates/hana_valence/src/tween.rs` — new.
- `crates/hana_valence/examples/staggered_unfold.rs` — new.

**Constraints from prior phases:** `Hinge`/`hinge_to_pose` (Phase 4), `AnchorPose` (Phase 1), scheduling contract (Phase 3: consumers configure `AnchorSystems` in `PostUpdate` before propagate; the example does its own registration since valence has no Plugin).

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo check -p hana_valence --no-default-features` still green; `cargo nextest run -p hana_valence --all-features` green; the example runs and visibly unfolds (manual spot-check); fmt + `clippy` skill clean.

---

### Phase 6 — diegetic provider bridge + vocabulary maps (additive)  · status: todo

#### Work Order

**Goal:** bevy_diegetic can produce valence geometry and translate its `Anchor` vocabulary both ways — defined and unit-tested, but registered into no app schedule yet.

**Spec:**

- `crates/bevy_diegetic/Cargo.toml`: add `hana_valence.workspace = true`.
- New module `crates/bevy_diegetic/src/panel/valence_provider.rs`:
  - `impl From<Anchor> for AnchorId` — the quad-provider map (this lives in diegetic on purpose; other shapes supply their own names):
    ```rust
    Anchor::TopLeft      => AnchorId::Vertex(0),
    Anchor::TopRight     => AnchorId::Vertex(1),
    Anchor::BottomRight  => AnchorId::Vertex(2),
    Anchor::BottomLeft   => AnchorId::Vertex(3),
    Anchor::TopCenter    => AnchorId::EdgeMid(0),
    Anchor::CenterRight  => AnchorId::EdgeMid(1),
    Anchor::BottomCenter => AnchorId::EdgeMid(2),
    Anchor::CenterLeft   => AnchorId::EdgeMid(3),
    Anchor::Center       => AnchorId::Center,
    ```
  - `impl TryFrom<AnchorId> for Anchor` — the partial inverse (`AnchorId` is non-exhaustive; unmappable ids return an error the screen placer will turn into a skip reason in Phase 7).
  - `write_panel_anchor_geometry` system: query `(Entity, &DiegeticPanel, …)` filtered `Changed<DiegeticPanel>` **only** (never `Changed<Transform>` — self-triggering; and local geometry is transform-invariant). For world-space panels, compute the 9 centered-local points + 4 perimeter edges from panel W×H in authored meters (reuse the size math behind `ResolvedPanelAnchorGeometry::from_world_panel`, `anchor_geometry.rs:113`) with **no transform baking** and `frame: None` everywhere; mutate an existing `ResolvedAnchorGeometry` in place (stable keys, retained allocation), insert only when absent. Panel resize reaches `Changed<DiegeticPanel>` (`set_width`/`set_height` mutate in place), so the filter misses nothing. The existing computed-on-demand `PanelAnchorGeometryParam` path stays for its own consumers (gizmos) — this is a new data flow, not a rename.
- **Do not register** the system into `DiegeticPanelPlugin` or any schedule — Phase 7's atomic swap does that (double-writer guard). Unit tests register it into a bare test schedule.

Tests (in `valence_provider.rs`):
- A spawned world panel gets a `ResolvedAnchorGeometry` with 9 points/4 edges at the expected centered-local positions; `validate()` passes.
- Resizing the panel updates positions in place (same map allocation observable via key stability; no remove/reinsert of the component).
- Round-trip: `Anchor -> AnchorId -> Anchor` is identity for all nine variants; `TryFrom` on an unmapped id errors.

**Files:**
- `crates/bevy_diegetic/Cargo.toml` — dependency.
- `crates/bevy_diegetic/src/panel/valence_provider.rs` — new.
- `crates/bevy_diegetic/src/panel/mod.rs` — module declaration only (no scheduling).
- Read-only reference: `crates/bevy_diegetic/src/panel/anchor_geometry.rs` (`from_world_panel` L113, `PanelPlane` L367), `crates/bevy_diegetic/src/layout/units/anchor.rs:8` (`Anchor`).

**Constraints from prior phases:** valence types come from Phases 1–3 (`AnchorId`, `AnchorPoint`, `Edge`, `ResolvedAnchorGeometry::validate`); the valence resolver is NOT running in diegetic apps yet — this phase writes an input component nothing reads, which is exactly why it is safely additive.

**Acceptance gate:** full `cargo build` green; `cargo nextest run` green (workspace) including the three new tests; no behavior change anywhere (no scheduling edits); fmt + `clippy` skill clean.

---

### Phase 7 — the atomic swap: relation, resolvers, placers, examples  · status: todo

#### Work Order

**Goal:** bevy_diegetic runs entirely on hana_valence — one stored relation (`AnchoredTo`), valence resolver on the world path, screen placer on the valence skeleton, diegetic copies deleted — with all existing behavior and tests preserved.

This phase is intentionally large because it cannot be split: the stored relation flips from `AnchoredToPanel` to `hana_valence::AnchoredTo`, and both placers read that one relation, so the world swap, the screen re-point, and the sugar rewrite must land together (double-writer/atomicity invariant). Work the checklist in order; the tree compiles only at the end — commit-sized intermediate states are not expected within this phase.

**Spec:**

1. **Sugar rewrite** (`crates/bevy_diegetic/src/panel/anchoring.rs`):
   - Delete the `AnchoredToPanel` relationship component and `PanelsAnchoredHere` (and the `pub use` re-export; consumers now read `hana_valence::AnchoredHere`).
   - New `AnchoredToPanel` = insert-only `#[derive(Bundle)]` with **private fields** wrapping: `hana_valence::AnchoredTo` (anchors lowered via `From<Anchor>`), the stored typed offset `PanelAnchorOffset`, and the authored component `PanelAttachmentAuthored { source: Anchor, target_anchor: Anchor }` (new, this file) that the screen placer reads by entity. Constructor `new(target: Entity, source: Anchor, target_anchor: Anchor) -> Self`; preserve the existing builder surface (offset et al.) so example call sites change minimally. No hana_valence type appears in any public signature.
   - Delete `PanelAnchorPose`; all consumers use `hana_valence::AnchorPose` directly (compose, no re-export).
   - `PanelAnchorOffset` stays diegetic (three `Dimension`s — unit/DPI/target-size knowledge does not move).
2. **Offset lowering (world)** — new system in `anchoring.rs` or `valence_provider.rs`: per frame, for world-space attachments with a `PanelAnchorOffset`, resolve against the live target exactly as today (`target_offset_meters` `world_anchoring.rs:355`: `to_layout_units(target.layout_unit())`, `target_size/panel_size` normalization) and apply the world Y-flip (today's `right*x − up*y + normal*z`, L350-351, becomes lowered `Vec3(x_m, −y_m, z_m)` since the valence resolver applies `base * offset` with no flip); write `hana_valence::ResolvedAnchorOffset`. One Vec3 write per frame, no relation re-insert. Screen-space attachments resolve their typed offset inside the placer callback as today (zero `ResolvedAnchorOffset` writes on the screen path).
3. **World path swap**: delete `world_anchoring.rs` (resolver, classify, placement, `WorldAnchorResolveDiagnostics` registration — the world path dissolves into provider + valence resolver). In `DiegeticPanelPlugin` (`panel/mod.rs`): configure `AnchorSystems` (`FillGeometry` and `AnimatePose` before `Resolve`, `Resolve` in `PostUpdate` before `TransformSystems::Propagate`); register `write_panel_anchor_geometry` (Phase 6) in `FillGeometry`, the offset-lowering system before `Resolve`, `hana_valence::resolve_anchors` + `hinge_to_pose` + the `AttachmentResolveDiagnostics<ResolveSkip>` resource, and hana_valence type registrations; order `PanelSystems::AnimateAnchorPose` inside `AnchorSystems::AnimatePose`.
4. **Screen placer re-point** (`screen_space/anchoring/`): `resolve.rs` calls `hana_valence::resolve_attachments` with `AttachmentResolveCandidate<AnchorResolveSkip>` carrying the `AnchoredTo` payload; `candidate.rs` classification reads `PanelAttachmentAuthored` (+ `PanelAnchorOffset`) by entity for its `Anchor`-typed math, with `TryFrom<AnchorId>` as fallback and a new skip-reason variant for unmappable ids; `placement.rs` callback body unchanged in spirit. The alias `AnchorResolveDiagnostics = AttachmentResolveDiagnostics<AnchorResolveSkip>` keeps working (same generic name, now from hana_valence).
5. **Delete** `crates/bevy_diegetic/src/panel/attachment_resolver.rs` (the last consumer just switched).
6. **Collateral updates**: `panel/gizmos.rs` (reads of `PanelsAnchoredHere`/pose types → `AnchoredHere`/`AnchorPose`); any `panel/mod.rs` re-exports; `panel/anchor_geometry.rs` untouched except doc references.
7. **Tests**:
   - Port from `world_anchoring.rs` into the new seams: `world_anchoring_respects_source_scale_and_parent_rotation` (same expected `(1.5,1.25,0)`) and `pose_written_in_animation_set_lands_this_frame` now run through provider + lowering + valence resolver in a diegetic test app.
   - `anchoring.rs`: replace the relationship tests with sugar-level ones (bundle insert creates `AnchoredTo` + `AnchoredHere`; reverse-index mechanics are covered valence-side since Phase 1). Update `anchor_types_are_registered_with_expected_reflect_component_data` for the new registration set (valence types registered by the plugin; split rule per Delegation Context).
   - `point_offsets_resolve_to_screen_pixels` (`screen_space/anchoring/resolve.rs:798`) must stay green — it is the DPI Pt→px guard for the lowering split.
   - New: world offset lowering test — a `PanelAnchorOffset` in Pt/target-relative units tracks a target resize/DPI change frame-to-frame via `ResolvedAnchorOffset`.
8. **Examples**: update `examples/panel_anchoring/*` (constructor call sites; `PanelAnchorPose` → `hana_valence::AnchorPose`; the example's own fold systems now write valence components in `AnchorSystems::AnimatePose`) and `examples/diegetic_text_stress.rs` (L53-65 call sites). Recipe-ification of hinge/spin/morph waits for Phase 8 — this phase is compile-and-behavior parity only.
9. **Breaking-changes record** (`crates/bevy_diegetic/CHANGELOG.md`): `Query<&AnchoredToPanel>` no longer possible (bundle, not component — query `hana_valence::AnchoredTo`); `PanelsAnchoredHere` → `hana_valence::AnchoredHere`; `PanelAnchorPose` → `hana_valence::AnchorPose`; `pub use PanelsAnchoredHere` removed. `Anchor::TopLeft` authoring and `AnchoredToPanel::new` call sites are unchanged.

**Files:**
- `crates/bevy_diegetic/src/panel/anchoring.rs` — sugar rewrite + authored component + lowering.
- `crates/bevy_diegetic/src/panel/world_anchoring.rs` — deleted (tests ported first).
- `crates/bevy_diegetic/src/panel/attachment_resolver.rs` — deleted.
- `crates/bevy_diegetic/src/panel/mod.rs` — plugin scheduling + set config + re-exports.
- `crates/bevy_diegetic/src/panel/gizmos.rs`, `src/panel/valence_provider.rs` — collateral.
- `crates/bevy_diegetic/src/screen_space/anchoring/{resolve,candidate,placement,mod}.rs` — re-point.
- `crates/bevy_diegetic/examples/panel_anchoring/*`, `examples/diegetic_text_stress.rs` — call sites.
- `crates/bevy_diegetic/CHANGELOG.md` — breaking list.

**Constraints from prior phases:** Phase 1–4 valence API (`AnchoredTo` private-target bundle-friendly ctor, `AnchoredHere`, `AnchorPose`, `ResolvedAnchorOffset`, `AnchorSystems`, `resolve_anchors`, `hinge_to_pose`, `resolve_attachments`, `AttachmentResolveDiagnostics<R>`, `ResolveSkip`); Phase 6 provider (`write_panel_anchor_geometry`, `From<Anchor>`, `TryFrom<AnchorId>`, `PanelAttachmentAuthored` is defined HERE not in Phase 6); valence exposes no Plugin — this phase owns all registration; the valence resolver applies no Y-flip and no unit resolution (both live in the lowering written here).

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo nextest run --all-features --workspace` green including the ported/updated named tests and the new lowering test; `panel_anchoring` example launches with hinge/spin/morph behavior parity (manual spot-check); fmt + `clippy` skill clean.

---

### Phase 8 — arrangements: tiling rule, Accordion/Strip, Member observer  · status: todo

#### Work Order

**Goal:** Named arrangements place and fold ordered sets of anchored tiles through one drivable parameter, with mid-animation spawn join.

**Spec:**

`src/arrange.rs`:
- **Tiling rule** (per-shape input, ~10 lines per shape): `next_edge(i) -> (Edge, Edge)` (which source/target edges seat tile `i` on tile `i−1`) and `rest_delta(i)` (rest pose between neighbors). Folding is universal (rotate about the shared edge — Phase 4); tiling is the only per-shape part: quads share parallel edges (straight strip), triangles alternate up/down, hexes zigzag.
- **Arrangement** = a named rule over an ordered set owning `placement(i) -> (AnchoredTo, Edge /* hinge edge */, rest pose)` plus exposed drivable params:
  - `Accordion { fold: f32 /* 0..1 */, lean: f32, pattern: FoldPattern }` — `FoldPattern::{Accordion, Coil}` encodes the fold distribution (uniform-alternating vs cumulative), ported from the example's `FoldPattern` (`panel_anchoring/hinge.rs:49`) rather than re-invented. Drive `fold` and the whole set folds.
  - `Strip {}` — static straight layout.
- A quad tiling rule ships in-crate for tests and the quad-based recipes (quads are plain geometry, no shape crate needed). Ordering over the set = `AnchoredHere` insertion order (deterministic by contract).
- **Member spawn-marker observer**: spawn a tile with `Member { arrangement }`; the observer reads the current member count, assigns index `i`, applies `placement(i)` (relation + rest pose + hinge), and seats it at the live fold angle if a fold is in flight — a new instance joins mid-animation and animates with the set. It must NOT assume contiguous order indices (the example's `collect_tiles_by_order`, `anchor_demo.rs:828`, has that bug — do not port it). Geometry-fill timing: if a spawned tile's geometry is not yet filled on its spawn frame, defer its first placement one frame rather than resolving at the authored pose and snapping.
- Docs (`lib.rs`): the naming-tiers workflow — (1) generated/procedural: no names, ids derived from adjacency; (2) hand-authored regular shape: provider names (`Anchor::TopLeft`) if offered; (3) one-off: raw `AnchorId` — with a one-line pick guide; recipes never hardcode ids (a reusable accordion asks geometry which edge it shares with its parent).
- Update `examples/panel_anchoring`: re-express the hinge/spin/morph demos on `Accordion` + a driver where they map (they stay examples, not library code).

Tests: 5-quad accordion folds under `fold` with both patterns (assert alternating vs cumulative angles); `Member` spawned mid-fold seats at the live angle; placement with non-contiguous indices works; `Strip` rest layout positions match hand-computed seats.

**Files:**
- `crates/hana_valence/src/arrange.rs` — new.
- `crates/hana_valence/src/lib.rs` — module + naming-tiers docs.
- `crates/bevy_diegetic/examples/panel_anchoring/*` — recipe port.
- Read-only reference: `crates/bevy_diegetic/examples/panel_anchoring/hinge.rs` (`FoldPattern` L49, `crease_sign` L257), `anchor_demo.rs:828`.

**Constraints from prior phases:** full valence stack live in diegetic since Phase 7 (provider fills geometry; `AnchorSystems` configured by `DiegeticPanelPlugin`; `Hinge`/`hinge_to_pose` drive folds); `AnchoredHere` iteration order = insertion order; observers are `bevy_ecs`-level (no `bevy_app` — the observer registers via `World::add_observer` or is exported for consumers to register).

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo nextest run --all-features` green including the four arrangement tests; no-default-features check green; `panel_anchoring` still behavior-par (spot-check); fmt + `clippy` skill clean.

---

### Phase 9 — triangle provider + box net demo + README  · status: todo

#### Work Order

**Goal:** A second shape proves the tiling-rule split, a box net folds shut, and the crate is documented for external use.

**Spec:**

- **Triangle provider** as example code (per the recipe/crate boundary: procedural shape providers are separate crates or example code — not core): `examples/triangle_accordion.rs` fills `ResolvedAnchorGeometry` for equilateral triangles (3 vertices, 3 edge-mids, centroid, 3 edges) and supplies the triangle tiling rule — shared edge + up/down flip alternate each step. This validates the open design question: the alternation must express cleanly through `next_edge(i)` / `rest_delta(i)`; if it cannot, the rule interface is wrong — fix the interface, do not special-case triangles.
- **Box net demo** `examples/box_net.rs`: 6 quads in a cross net — a **tree** of edge-shared tiles (one `Hinge` per tile at its single `AnchoredTo` parent edge; tree topology is the committed model — the net of any convex polyhedron is a simply-connected planar tree), each with target dihedral 90°; drive every hinge to its target (manual system or `tween` feature) and the box folds shut. Net closure is topology + target angles, not a resolver invariant — build the net to close; assert final face positions within an epsilon that tolerates cumulative float error.
- Optional stretch: tetrahedron (4 triangles) reusing the triangle geometry — include only if the triangle rule lands cleanly.
- **README** (`crates/hana_valence/README.md`), mirroring the bevy_diegetic README shape (quick-start, examples dir, bevy compat): name story — in chemistry, an atom's **valence** is its capacity to bond: the number and arrangement of connection points it offers. This crate gives shapes the same thing — programmable anchor points by which they bond, assemble, and animate as bonds form, break, and reconfigure. One-liner: *hana_valence — shapes expose connection points and bond into animatable assemblies; named for valence, an atom's capacity to bond.* Vocabulary note: the crate is `hana_valence` but types keep the **anchor** noun (`AnchorId`, `AnchoredTo`, `AnchorPose`) — an anchor point is the concrete connection site, valence the capacity those points add up to. Follows the workspace convention of borrowing one precise outside-field term (diegetic — film theory, lagrange — orbital mechanics, liminal — anthropology, valence — chemistry).

Tests: box-net fold closure within epsilon (headless, fixed frame count); triangle strip alternation positions match hand-computed seats.

**Files:**
- `crates/hana_valence/examples/triangle_accordion.rs` — new.
- `crates/hana_valence/examples/box_net.rs` — new.
- `crates/hana_valence/README.md` — rewrite.
- `crates/hana_valence/src/arrange.rs` — only if the tiling-rule interface needs adjustment for triangles.

**Constraints from prior phases:** arrangements + quad tiling rule (Phase 8); `Hinge` per-relation semantics (Phase 4); examples use full `bevy` dev-dependencies (Phase 5 precedent); the lib dep-surface gate still applies.

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo nextest run --all-features` green including closure + alternation tests; both examples run and visibly fold (manual spot-check); fmt + `clippy` skill clean.

---

## Deferred (recorded decisions, not scheduled in this plan)

- **Magnetize** — `magnetize(group)` finds nearest unpaired edges across loose tiles, creates `AnchoredTo`, tweens transforms to seat the edge. Decided: seated edges become hinges, so "magnetize then fold" composes. Pure edge math in valence; unscheduled.
- **Ring arrangement** (`Ring { closure }`) — follows the Phase 8 pattern when wanted.
- **Frame-aware hinge axis** — folding on frame-divergent (curved-surface) edges; additive extension after the curved-surface sampler (`surface-panels.md` `SurfaceSample`) fills `AnchorPoint.frame`.
- **Cross-space anchoring** — screen panel anchored to a world target needs a camera-projection step neither placer has; the seam: project the world anchor to viewport coordinates, feed the screen placer.
- **Debug gizmo module** — optional `debug` feature (`draw_anchor_geometry`, `draw_relations`, `draw_hinge_axes` behind a `GizmoConfigGroup`) so providers get an authoring aid; needs `bevy_gizmos`, so it must be feature-gated to protect the dep surface.
- **bevy_animation adapters** — `AnimatableProperty` / `animated_field!(Hinge::angle)` behind an `animation` feature; fits pre-authored choreography with graph blending, awkward for procedural nets.
- **`NetClosure` validator** — optional check that a net's topology + target angles close.
- **Widgets handoff** (binds `widgets.md` Phase 1, unblocked after Phase 7): widget reification publishes `ResolvedAnchorGeometry` on materialized widget entities, gated on `Has<AnchoredHere>` (widgets are high-cardinality; fill only actual anchor targets — panels stay unconditional); widget-side sugar mirrors `AnchoredToPanel::new` but takes `WidgetId` (resolved to the stable entity internally); widget reification also publishes screen rects (widget bounds + parent `ResolvedScreenPanelPosition` → the existing `screen_panel_rects` path) so screen-space tooltips cover widget targets, plus a cleanup sweep when a panel leaves screen space.

## Appendix — further research: Verlet dynamics over the anchor graph

Not a design; a research direction noted 2026-07-07. Nothing in the contract
above depends on it, and nothing above blocks it.

The valence resolver is kinematic: parent pose in, child pose out, one
direction, no state. Verlet integration is the standard cheap way to add
dynamics on top of exactly this kind of constraint network (Jakobsen,
"Advanced Character Physics", 2001 — ropes, cloth, ragdolls). Each particle
stores current + previous position (velocity is implicit); constraints are
enforced by iterative relaxation that corrects both endpoints, so coupling is
two-way — the piece the kinematic resolver deliberately lacks.

Why it fits valence specifically — the crate's data already *is* the
constraint topology a Verlet solver needs:

- `AnchorPoint`s = constraint attachment sites.
- `AnchoredTo` edges = distance constraints between bodies.
- A `Hinge` edge = two shared particles along the pivot line; free swing
  around them is a hinge with no special-casing.
- The anchored-to target = the pinned particles a chain hangs from.

A hypothetical `hana_verlet` layer would read the anchor graph to build its
particle/constraint set, simulate, and write results back — either into
valence inputs (`Hinge` angle, `AnchorPose`) for spring-driven secondary
motion, or directly to `Transform` for fully simulated bodies while valence
keeps resolving the kinematic ones. Same division of labor as animation:
valence owns topology and pose resolution; simulation is a separate writer of
its inputs.

Known problems to research, not solved here:

- **Rigid-body orientation.** A particle has no rotation. Standard fix:
  3-4 particles per panel (corners) with rigid mutual distance constraints,
  recover position + rotation from the corner set (or shape matching).
- **Stiffness vs cost.** Rigidity comes from relaxation iteration count;
  droop is a feature at low counts, but stiff chains need enough iterations
  to avoid visible stretch.
- **Collision.** World/panel collision needs additional constraint types;
  out of scope for a first pass.
- **Deformation.** Bodies stay rigid quads unless subdivided into particle
  grids — which is exactly Verlet cloth, and a plausible follow-on for
  banner-like panels.
- **Handoff semantics.** A body switching between kinematic (valence
  resolver) and simulated (Verlet writes `Transform`) needs a clean
  ownership rule so both never write the same entity in one frame.

Payoff if pursued: hanging sign chains, cables between panels, cloth-ish
banners — a few dozen lines of solver, no physics-engine dependency. It would
also be a second consumer of the geometry contract, reinforcing the decision
to carry per-point frames.
