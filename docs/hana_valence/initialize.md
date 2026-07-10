# hana_valence — implementation plan

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Extracts bevy_diegetic's
> anchoring machinery into the standalone shape-agnostic `hana_valence` crate
> (geometry contract, attachment relation, resolver, hinge, arrangements), then
> re-points bevy_diegetic as one provider of anchor geometry.

## Delegation Context

- **Project:** `hana_valence` — a new shape-agnostic workspace crate ("shapes expose connection points and bond into animatable assemblies"), extracted from `bevy_diegetic`'s anchoring machinery. Workspace root `/Users/natemccoy/rust/bevy_hana` (repo "hana", `natepiano/hana`). Members declared by **glob**: root `Cargo.toml:3` `members = ["crates/*"]` (with `exclude = ["crates/hana", "vendor/clay-layout"]`) — a crate at `crates/hana_valence` is auto-registered as a member, no edit needed. To be consumed by `bevy_diegetic` it must be a `[workspace.dependencies]` entry, which **already exists**: root `Cargo.toml:67` `hana_valence = { path = "crates/hana_valence" }` (shifted from L43 by Phase 1's subcrate entries). The crate dir also **already exists** (`crates/hana_valence/` with `Cargo.toml`, `src/lib.rs` = just `//! hana_valence`, README, LICENSEs, CHANGELOG). Model small crate for the inherited-Cargo pattern: `crates/bevy_lagrange/Cargo.toml` (and `crates/bevy_liminal/Cargo.toml`) — both use `authors.workspace = true`, `edition.workspace = true`, `license.workspace = true`, `repository.workspace = true`, and `[lints] workspace = true`. The existing `crates/hana_valence/Cargo.toml` already follows this; Phase 1 narrowed its deps to the five subcrates below and set `publish = false`.
- **Stack:** Rust **edition 2024** (`[workspace.package] edition = "2024"`); Bevy **0.19.0** (`[workspace.dependencies] bevy = { version = "0.19.0", default-features = false }`). `bevy_tween` is a workspace dependency (added by Phase 5) and is an **opt-out default feature** of `hana_valence`: `hana_valence/Cargo.toml` has `default = ["tween"]`, and the root `[workspace.dependencies] hana_valence` sets `default-features = false` (a member cannot override an inherited workspace default — Cargo errors — so it lives on the workspace entry). Consequently `bevy_diegetic` inherits `default-features = false` and pulls **no** `bevy_tween`; the `--no-default-features` build is the gate that exercises tween-off. Any workspace member wanting tween adds `features = ["tween"]`. The workspace depends on **full `bevy`** (default-features=false, features selected per-crate); Phase 1 added separate `[workspace.dependencies]` subcrate entries, all `0.19.0` + `default-features = false`: `bevy_ecs` `["bevy_reflect","std"]`, `bevy_math` `["bevy_reflect","std"]`, `bevy_platform` `["std"]`, `bevy_transform` `["std"]` (the hana_valence manifest adds `bevy-support` — the `Transform`/`GlobalTransform` `Component` impls need it), `bevy_reflect` `["glam","std"]` (the `Reflect` derive and `TypeRegistry` live only in `bevy_reflect` — no subcrate re-exports them). Strict workspace lints: clippy `all`/`cargo`/`nursery`/`pedantic` = deny, plus `unwrap_used`/`expect_used`/`panic`/`unreachable` = deny, and `missing_docs = "deny"` — every pub item needs docs; fallible paths return `Result`, never panic.
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
  - Dep surface: `hana_valence` depends only on `bevy_ecs + bevy_transform + bevy_math + bevy_platform + bevy_reflect + tracing` (`tracing = { version = "0.1", default-features = false, features = ["std"] }` — already in the tree via full `bevy`; `bevy_log`'s subscriber captures `tracing::warn!` directly), no **direct** dependency on full `bevy` or `bevy_app` (so the crate exposes systems + sets, **no `Plugin` type** — consumers register). `bevy_transform`'s `bevy-support` feature — required in Bevy 0.19 for the `Transform`/`GlobalTransform` `Component` impls — pulls `bevy_app` transitively; accepted, no `bevy_app` API is used. Gate: `cargo check -p hana_valence --no-default-features` stays green. Dev-dependencies (examples) may use full `bevy`.
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

### Phase 1 — crate scaffold + core contract types  · status: done (uncommitted)

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

#### Retrospective

**What worked:**
- Fast-path delegation from this Work Order: implemented nearly verbatim, all gates green on first pass (build, no-default-features check, 10 tests, clippy).

**What deviated from the plan:**
- Fifth narrow dependency `bevy_reflect` (default-features = false, features `glam` + `std`) was compile-mandatory: none of the four named subcrates re-export the `Reflect` derive macro or `TypeRegistry` (verified in bevy_ecs 0.19 source — its prelude exports only `ReflectComponent`/`AppTypeRegistry` etc.). Full `bevy`/`bevy_app` remain excluded. Dep-surface invariant updated accordingly.
- `ResolvedAnchorWorld` drops `Copy` from the spec's derive list — impossible on a `HashMap` field.
- Post-review fix: `Edge::axis` needed an explicit `MIN_AXIS_LENGTH = 1e-4` (associated const on `Edge`) sub-epsilon check — `Dir3::new` only rejects zero/non-finite separations, so tiny-but-finite edges passed. Covered by `sub_minimum_length_edge_reports_degenerate`.
- Optional `AnchorGeometry` authoring helper trait not created (spec marked it optional).

**Surprises:**
- Workspace feature sets that satisfy the no-default-features gate are now known: `bevy_ecs` `["bevy_reflect","std"]`, `bevy_math` `["bevy_reflect","std"]`, `bevy_platform` `["std"]`, `bevy_transform` `["std"]`, `bevy_reflect` `["glam","std"]`.

**Implications for remaining phases:**
- Phase 2's de-prelude step may import from `bevy_reflect` directly; the dep is already in place.
- Tests needing a registry use `bevy_ecs::prelude::AppTypeRegistry` + `bevy_reflect::TypeRegistry` (pattern in `relation.rs` tests).
- `GeometryError` is a fifth public geometry type (`MissingAnchor { edge, anchor }` / `DegenerateEdge` / `NonFinitePoint` / `FrameDivergentEdge`), re-exported from `lib.rs` alongside the spec'd types.

#### Phase 1 Review

- Logging decision (user-approved): the warn output specified in Phases 2–4 uses `tracing::warn!`; `tracing` (default-features = false, `std`) added to the dep-surface invariant as the sixth allowed dependency — already in the tree via full `bevy`, so no new crate. Phases 2, 3, 4 Work Orders now name the mechanism; Phase 2 adds the `[workspace.dependencies]` entry.
- Phase 2 de-prelude list extended with `bevy_reflect` (the `Reflect` derive and `TypeRegistry` live only there).
- Phase 3 and Phase 7 constraints now record the shipped `AnchoredTo` API (`new`/`with_offset`/`retargeted`/`target()`) and that `ResolvedAnchorWorld` is `Clone`, not `Copy`.
- Phase 4 Work Order notes `Edge::MIN_AXIS_LENGTH` (1e-4) is a private associated const; its degenerate-edge test uses coincident positions.
- Phase 5 gains a fallback for bevy_tween/Bevy-0.19 incompatibility (pin a git rev, or defer after Phases 6–7).
- Delegation Context: `hana_valence` workspace-dep line ref corrected `Cargo.toml:43` → `:67`; Stack line updated with the actual subcrate feature sets.
- No remaining phase is redundant or invalidated; Phase 1's extras (`GeometryError` public, `AnchoredHere` read API, `retargeted()`) support Phases 7–8 rather than displace them.

---

### Phase 2 — attachment skeleton port (copy)  · status: done (uncommitted)

#### Work Order

**Goal:** The generic attachment skeleton (topological ordering, fallback dispatch, diagnostics) lives in `hana_valence` with an `AnchoredTo` payload, unit-tested; the diegetic original is untouched.

**Spec:**

Copy `crates/bevy_diegetic/src/panel/attachment_resolver.rs` into `crates/hana_valence/src/attachment.rs` (a **copy** — the diegetic file keeps compiling for the screen placer until Phase 7), with these changes:
- Payload re-type: `AttachmentResolveCandidate::Active` / `AttachmentResolveAction::Place` / `AttachmentGraph` carry `AnchoredTo` (Phase 1) instead of `AnchoredToPanel`. No extra `<A>` payload generic — `AnchoredTo` is the one stored relation both placers will read. The payload's `AnchorId` fields are simply unused by consumers that speak their own vocabulary.
- Rename `resolve_panel_attachments` → `resolve_attachments` (nothing panel-shaped remains).
- De-prelude: replace `bevy::prelude::*` / `bevy::platform::…` imports with granular `bevy_ecs` / `bevy_math` / `bevy_platform` / `bevy_reflect` paths (the no-default-features gate fails otherwise; the `Reflect` derive and `TypeRegistry` come from `bevy_reflect` — see Phase 1 retrospective).
- Visibility: `pub` (was `pub(crate)`), with docs on every item (`missing_docs` is deny).
- Keep the name `AttachmentResolveDiagnostics<R>` exactly — it is already the one generic diagnostics type; diegetic's `WorldAnchorResolveDiagnostics` and screen `AnchorResolveDiagnostics` are aliases of it, and keeping the name means no rename anywhere and no cross-crate collision.
- Keep the callback shape: `resolve_attachments<R, F>` with `F: FnMut(AttachmentResolveAction) -> Result<(), R>`; candidates arrive **pre-classified** (`Active`/`Skipped`) — classification is the consumer's job, the skeleton owns ordering, dispatch, and diagnostics. Add warn-on-repeated-skip logging via `tracing::warn!` so a consumer can see why an attachment isn't moving (add `tracing` to `[workspace.dependencies]` per the dep-surface invariant; it is not yet a workspace entry).

Tests: chain ordering (3-level chain resolves parent-before-child in one pass), fallback dispatch (a `Skipped` candidate routes to `Fallback` with its reason recorded), diagnostics accumulation across frames, missing-target behavior preserved from the original.

**Files:**
- `crates/hana_valence/src/attachment.rs` — new (ported copy).
- `crates/hana_valence/src/lib.rs` — module + re-exports.
- Read-only source: `crates/bevy_diegetic/src/panel/attachment_resolver.rs` (L14 candidate enum, L39 action enum, L51 resolve fn, L88 graph, L285 diagnostics).

**Constraints from prior phases:** Phase 1 provides `AnchoredTo` (fields `target` private + `target()` accessor, `source_anchor`, `target_anchor`, `offset: Vec3`) in `crates/hana_valence/src/relation.rs`; `bevy_platform` HashMap/HashSet are workspace deps.

**Acceptance gate:** `cargo build` green; `cargo nextest run -p hana_valence` green including the four new skeleton tests; no-default-features check green; diegetic untouched (`git diff --stat` shows no `bevy_diegetic` changes); fmt + `clippy` skill clean.

#### Retrospective

**What worked:**
- Ordering/cycle/fallback algorithm copied unchanged from `attachment_resolver.rs`; blind review approved with zero findings; all gates green first pass (14 tests total).

**What deviated from the plan:**
- `resolve_attachments` requires `R: Debug` (beyond the original's `Copy + Eq + Hash + Send + Sync + 'static`) so the repeated-skip warning can print the reason value.
- `AttachmentResolveDiagnostics` gained public read accessors — `entries()`, `len()`, `is_empty()`, and `current()` (test-only in the original) — a `pub` resource consumers must be able to read. `AttachmentResolveDiagnostic` is exported for the iterator item.
- Frame/count increments use `saturating_add` instead of `+=` (overflow-panic lints).
- Tests written as four granular tests rather than porting the original's single combined test.

**Implications for remaining phases:**
- Phase 3's `ResolveSkip` enum must satisfy `Copy + Debug + Eq + Hash + Send + Sync + 'static` (the `resolve_attachments` bound).
- The repeated-skip `tracing::warn!` fires on every repeat occurrence (once per frame per persisting skip); throttling is deliberately deferred until it proves noisy in practice.

#### Phase 2 Review

- Phase 3 Work Order: `ResolveSkip` gained the resolver-produced variants (`BlockedBySkippedDependency`, `Cycle`, `BlockedByCycle`) and the `AttachmentResolveReasons` construction fact; the shipped `resolve_attachments` signature + `R` bounds + caller pre-classification responsibility added as constraints; `attachment.rs` added to Files (read-only); noted the skeleton's `record` already emits the repeated-skip `tracing::warn!` so Phase 3 adds no logging of its own.
- Phase 4 Work Order (user decision): hinge failures do NOT write `AttachmentResolveDiagnostics` (its `record`/`begin_frame` are private and its frame counter is owned by `resolve_attachments`; writes from `AnimatePose` would mis-stamp the frame). Resolved as a `Result`-first split: pure `Hinge::rotation(&self, &ResolvedAnchorGeometry) -> Result<Quat, EdgeAxisError>` propagates the failure as a value for API consumers; the bundled `hinge_to_pose` system applies the skip-write + `tracing::warn!` policy per entity (never a system-level `Result` — one bad entity must not abort healthy ones).
- Phase 7 Work Order: recorded the diegetic-side verification (`R: Debug` already derived by both skip enums; `AttachmentResolveReasons` name/shape unchanged so `placement.rs:210` ports as-is) and the accepted behavior change (screen placer gains per-frame repeated-skip warnings; throttling deferred).
- No remaining phase was found redundant, mis-scoped, or invalidated.

---

### Phase 3 — `resolve_anchors` resolver  · status: done (uncommitted)

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
- Order + dispatch through Phase 2's `resolve_attachments` skeleton. Define the valence skip reason enum with the classification variants **plus the three graph-reason variants the skeleton demands**, e.g. `ResolveSkip { MissingSourceGeometry, MissingTargetGeometry, MissingAnchor(AnchorId), DespawnedTarget, UnsupportedParentTransform, NonFiniteScale, BlockedBySkippedDependency, Cycle, BlockedByCycle }` — `resolve_attachments` takes an `AttachmentResolveReasons<ResolveSkip> { blocked_by_skipped_dependency, cycle, blocked_by_cycle }` argument per call (mirror the diegetic model: `WorldAnchorResolveSkip` at `world_anchoring.rs:310` has these variants, constructed at `world_anchoring.rs:295`). Expose `AttachmentResolveDiagnostics<ResolveSkip>` as a resource the consumer registers (`Default` gives capacity 128; test schedules `init_resource` it, the system takes `ResMut`). Skipped entities keep their last/authored `Transform` (fallback). The skeleton's `record` already emits `tracing::warn!` on repeated skips — Phase 3 adds no logging of its own.
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
- Read-only source: `crates/bevy_diegetic/src/panel/world_anchoring.rs` (system L50, placement L95-147, `target_anchor_point` L342, `target_offset_meters` L355, `plane_frame_translation` L378, `scaled_source_anchor_offset` L382, `validate_supported_parent_transform` L436, tests L959/L1205); `crates/hana_valence/src/attachment.rs` (the Phase 2 skeleton this phase dispatches through).

**Constraints from prior phases:** Phase 1 types (`AnchorPoint::rotation()`, `ResolvedAnchorOffset`, `AnchorSystems`, `ResolvedAnchorWorld`) and Phase 2 skeleton are in place; tests run via `World`+`Schedule`, no `App`/`Plugin` exists in the crate. Shipped Phase 2 API (`attachment.rs`): `resolve_attachments(candidates: Vec<AttachmentResolveCandidate<R>>, reasons: AttachmentResolveReasons<R>, diagnostics: &mut AttachmentResolveDiagnostics<R>, handle: F)` with `R: Copy + Debug + Eq + Hash + Send + Sync + 'static` and `F: FnMut(AttachmentResolveAction) -> Result<(), R>` — so `ResolveSkip` must derive all of those. Candidates arrive pre-classified: classification (missing geometry, despawned target, non-uniform parent scale, …) is Phase 3's job before calling the skeleton; per-place failures return `Err(ResolveSkip)` from the callback. `AttachmentResolveCandidate<R>` carries the `AnchoredTo` payload. Shipped `AnchoredTo` API (`relation.rs`): `AnchoredTo::new(target: Entity, source_anchor: AnchorId, target_anchor: AnchorId)` (offset = `Vec3::ZERO`), `.with_offset(Vec3)`, `.retargeted(Entity)`, private `target` field + `target()` accessor. `ResolvedAnchorWorld` is `Clone`, not `Copy` (HashMap field) — the resolver mutates its `points` in place.

**Acceptance gate:** `cargo build` green; `cargo nextest run -p hana_valence` green including all seven named tests above; no-default-features check green; fmt + `clippy` skill clean.

#### Retrospective

**What worked:** Fast-path dispatch straight from this Work Order; the math ported term-for-term and all seven named tests (plus an added non-uniform-parent skip test) passed on the first codex pass; the Phase 2 skeleton's classify-then-dispatch seam fit without changes.

**What deviated from the plan:**
- `resolve_anchors` initially took exclusive `&mut World`; a user-directed fix pass converted it to a parameterized system (`&Entities` + disjoint queries + `ResMut<ResolveDiagnostics>`). Every access is by-entity; the local `resolved_globals: HashMap<Entity, GlobalTransform>` map carries this-frame globals to downstream anchored entities, so exclusive world access was never needed.
- `ResolveSkip` gained two variants beyond the spec list (`MissingSourceTransform`, `MissingTargetTransform`) and is `#[non_exhaustive]`.
- `crates/hana_valence/Cargo.toml` enables `bevy_transform`'s `bevy-support` feature — in Bevy 0.19 the `Component` impls for `Transform`/`GlobalTransform` live behind it, and the feature unconditionally pulls `bevy_app` transitively (`bevy-support = ["alloc", "dep:bevy_app", "dep:bevy_ecs"]`). No feature combination avoids it. Dep-surface invariant reworded to direct-dependency terms (see Phase 3 Review).

**Surprises:**
- `Entities::contains` accepts valid-but-despawned ids in Bevy 0.19; `contains_spawned` is the correct liveness check for `DespawnedTarget`.
- The blind reviewer flagged `ResolvedAnchorWorld` staleness for cached-but-unresolved entities as a blocker; resolved as documentation — the cache has exactly the freshness of the resolve pass itself (everything before `TransformSystems::Propagate` reads previous-frame globals), and doing better would mean re-implementing propagation.

**Implications for remaining phases:**
- Phase 7 must `init_resource::<ResolveDiagnostics>()` (alias for `AttachmentResolveDiagnostics<ResolveSkip>`) — a missing resource is now a loud Bevy error, not a silent no-op.
- `ResolveSkip` is `#[non_exhaustive]`: cross-crate matches (diegetic-side) need a wildcard arm.

#### Phase 3 Review

- Delegation Context: dep-surface invariant reworded to direct-dependency terms (`bevy_transform/bevy-support` pulls `bevy_app` transitively — accepted, no `bevy_app` API used); Stack line notes the crate manifest adds `bevy-support`.
- Phase 4 Work Order: added test-world requirements for running the shipped resolver (`ResolveDiagnostics` resource mandatory, tiles need `Transform`+`GlobalTransform`) and a pointer to `resolve.rs`'s reusable test helpers.
- Phase 5/8/9 Work Orders: added the `init_resource::<ResolveDiagnostics>()` requirement for any schedule running `resolve_anchors`; Phase 8 additionally notes members must spawn with `Transform` and that the defer rule covers `MissingSourceTransform`.
- Phase 7 Work Order: step 3 now names `init_resource::<hana_valence::ResolveDiagnostics>()` and plain-system registration; Constraints add the loud missing-resource failure, the `#[non_exhaustive]` 11-variant `ResolveSkip` (wildcard arm needed), and the unfiltered `Query<&mut Transform>` scheduling-ambiguity fact.
- Phase 7 scope addition (user-approved): detach-restore (`AnchoredWorldPanelPose` + `restore_inactive_world_panel_poses`) is ported into `anchoring.rs` with capture relocated to an on-insert observer/hook (old capture point inside placement is deleted with `world_anchoring.rs`); new detach-restore test added to the checklist. Valence's keep-last fallback unchanged.
- No remaining phase found redundant, mis-scoped, or invalidated; Phase 6 untouched.

---

### Phase 4 — `Hinge` + `hinge_to_pose`  · status: done (uncommitted)

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

impl Hinge {
    /// Pure conversion: the fold rotation about the edge axis.
    /// Failure propagates as a value (public `EdgeAxisError`) so API
    /// consumers can match on the cause and choose their own policy.
    pub fn rotation(&self, geom: &ResolvedAnchorGeometry) -> Result<Quat, EdgeAxisError> {
        Ok(Quat::from_axis_angle(*self.edge.axis(geom)?, self.angle))
    }
}

fn hinge_to_pose(q: Query<(Entity, &Hinge, &ResolvedAnchorGeometry, &mut AnchorPose)>) {
    // per entity: Ok(rot)  => pose.rotation = rot;
    //             Err(err) => skip the write + tracing::warn!(entity = ?entity, err = ?err, "hinge axis unavailable")
    //                         and continue with remaining entities.
    // NOT a system-level `-> Result`: the failure is per-entity; a `?` would
    // abort pose writes for healthy entities and Bevy's default error handler
    // panics in debug. Consumers wanting a different policy register their own
    // system over `Hinge::rotation` instead of `hinge_to_pose`.
}   // runs in AnchorSystems::AnimatePose, before Resolve
```

- This is the generalization of the example's `Quat::from_rotation_x` (`panel_anchoring/hinge.rs:330`): a fold is about an **edge**, not "top/bottom" — the only per-shape input is which two anchors form the hinge edge, and that comes from `ResolvedAnchorGeometry.edges`.
- Frame-divergent edges cannot fold (the chord is neither endpoint's tangent): Phase 1's fill-time validation already rejects them from geometry; `Edge::axis` returns `FrameDivergent` as defense-in-depth, surfaced through `Hinge::rotation`'s `Result`. For now the invariant is trivially "all frames `None`" (only flat providers exist); a frame-aware axis is a later, additive extension.
- `Edge::axis` also rejects endpoint separations below `Edge::MIN_AXIS_LENGTH` (1e-4, a **private** associated const in `geometry.rs`) as `Degenerate`. The degenerate-edge test here uses coincident endpoint positions; it cannot reference the const cross-module.
- Driver-conflict policy (last-writer-wins): `hinge_to_pose` overwrites `AnchorPose` every frame; to drive `AnchorPose` directly, remove `Hinge`. Document on both types; add a debug-only warning (`tracing::warn!` under `#[cfg(debug_assertions)]`) when an entity carries `Hinge` and something else also mutated `AnchorPose` this frame.
- Register `Hinge` with `ReflectComponent` (it is a mutable input).

Tests:
- N-quad straight strip: 5 hand-filled quads chained TL→TR (or edge-mid pairs), each with a `Hinge` on the shared edge; a manual system in `AnimatePose` drives angles; assert the folded transforms after one frame (accordion alternation = alternating angle signs, as `crease_sign` does in the example).
- Degenerate edge (`start == end` positions): `Hinge::rotation` returns `Err(EdgeAxisError::Degenerate)`, the system skips the pose write, no NaN in any `Transform`.
- Endpoint swap flips fold sign.

**Files:**
- `crates/hana_valence/src/hinge.rs` — new.
- `crates/hana_valence/src/lib.rs` — module + re-exports.
- Read-only reference: `crates/bevy_diegetic/examples/panel_anchoring/hinge.rs` (L49 `FoldPattern`, L257 `crease_sign`, L330 `from_rotation_x`).

**Constraints from prior phases:** `Edge::axis(&geom) -> Result<Dir3, EdgeAxisError>` exists (Phase 1); the resolver consumes `AnchorPose` in `Resolve` after `AnimatePose` (Phase 3); `AttachmentResolveDiagnostics<R>` (Phase 2) is NOT written here — its `record`/`begin_frame` are private to `resolve_attachments` and its frame counter would mis-stamp entries from `AnimatePose`; hinge failures surface only via `Hinge::rotation`'s `Result` and the system's `tracing::warn!`. The N-quad accordion test runs `resolve_anchors` (shipped in Phase 3 as a parameterized system, not exclusive-world): the test world must insert the `ResolveDiagnostics` resource (missing resource = loud Bevy failure) and every spawned tile needs `Transform` + `GlobalTransform` (else the `MissingSourceTransform` skip fires and the tile keeps its authored transform). Reuse the test scaffolding in `resolve.rs`'s test module (`quad_geometry()`, `world_with_diagnostics()`, `run_resolve()`) instead of reinventing it.

**Acceptance gate:** `cargo build` green; `cargo nextest run -p hana_valence` green including the three tests above; no-default-features check green; fmt + `clippy` skill clean.

#### Retrospective

**What worked:** Fast-path dispatch from this Work Order; one codex pass, all three named tests green first try (25 total); the hand-derived accordion expectations matched the Phase 3 resolver math exactly; `resolve.rs` test scaffolding reused as intended.

**What deviated from the plan:**
- `hinge_to_pose` overwrites the whole `AnchorPose` (translation reset to `Vec3::ZERO`), not the pseudocode's rotation-only write. This is the coherent reading of the last-writer-wins policy (while `Hinge` is present it owns the pose; the debug conflict warning exists because combining drivers is unsupported) and is documented on both types. A driver wanting hinge fold + pose translation must replace `hinge_to_pose` with its own system over `Hinge::rotation`.
- The shared resolver test scaffolding became file-scope `#[cfg(test)] pub(crate)` helpers in `resolve.rs` (`world_with_diagnostics()`, `run_resolve()`, `spawn_quad()`, `quad_geometry()`) rather than a public `resolve::tests` module — `cargo mend` rejects public test modules.
- The blind reviewer caught a false negative in the debug conflict warning: excluding added-this-frame poses also silenced insert-then-mutate conflicts in the same frame. Fixed (user-directed direct edit): warn when changed-this-frame AND `last_changed() != added()` — insertion stamps both ticks equal, so the inequality isolates post-insert mutation.

**Implications for remaining phases:**
- Phase 5's `AnchorPoseLens` and `HingeAngleLens` must not target the same entity: `hinge_to_pose` overwrites the whole pose each frame, so a pose tween on a `Hinge`-carrying entity is silently lost (and warns in debug).
- Hinge test scaffolding available for Phase 8's accordion tests: `hinge.rs` tests show the drive-system + `hinge_to_pose` + `run_resolve` wiring and the accordion transform assertions.

#### Phase 4 Review

- Phase 5 Work Order: animator contract now states the whole-pose overwrite rule — `AnchorPose` tweens and `Hinge` are mutually exclusive per entity, documented on both lenses; constraints add tween-apply-before-`hinge_to_pose` ordering within `AnimatePose` and that hinged example quads spawn with `AnchorPose::default()`.
- Phase 7 Work Order: step 3 now places `hinge_to_pose` in `AnimatePose` after `PanelSystems::AnimateAnchorPose` (was ambiguous alongside `resolve_anchors`); constraints add the whole-pose overwrite (ported demo fold systems drive `Hinge.angle` only; direct `AnchorPose` writers stay `Hinge`-free), the spawn-with-`AnchorPose::default()` requirement, and that `Changed<AnchorPose>` is hot every frame for hinged entities (never gate on it).
- Phase 8 Work Order: constraints add fold drivers write `Hinge.angle` only, spawn-with-`AnchorPose::default()` via `placement(i)`/Member observer, and the `resolve.rs` helper visibility (file-scope `#[cfg(test)] pub(crate)` fns, not a public tests module).
- Phase 9 Work Order: constraints add spawn-with-`AnchorPose::default()` + `Hinge.angle`-only driving for hinged tiles.
- User-approved decision: `rest_delta(i)` re-specified as a scalar rest angle (radians) about the shared hinge edge, composed into `Hinge.angle` by the arrangement drive system — a rest pose cannot live in `AnchorPose` on hinged tiles (`hinge_to_pose` owns it wholesale). Phase 8 Spec and Phase 9 triangle-rule bullet updated.
- No remaining phase found redundant, mis-scoped, or invalidated; Phase 6 unchanged.

---

### Phase 5 — bevy_tween adapters (`tween` feature)  · status: done (uncommitted)

#### Work Order

**Goal:** bevy_tween drives hinges and poses through shipped lenses, demonstrated by a staggered-unfold example.

**Spec:**

- Add `bevy_tween` as an **optional** dependency behind a new `tween` feature (pick the release compatible with Bevy 0.19; it is not yet a workspace dependency — add it to `[workspace.dependencies]`). Fallback if no Bevy-0.19-compatible release exists when this phase runs: pin a git rev in `[workspace.dependencies]`, or defer this phase until after Phases 6–7 (nothing downstream depends on it except the `staggered_unfold` example). The `tween` feature must not violate the no-default-features gate (adapters live in `src/tween.rs` behind `#[cfg(feature = "tween")]`).
- `src/tween.rs`: `HingeAngleLens` (lerp `Hinge.angle`) and `AnchorPoseLens` (slerp `rotation`, lerp `translation`), implementing bevy_tween's lens/interpolator trait for the version chosen. Tweens write in `AnchorSystems::AnimatePose` — document how to order bevy_tween's systems into that set.
- Published animator contract (crate docs): (1) `AnchorPose`, `Hinge.angle`, and `Transform` (un-anchored entities only) are the tweenable targets; (2) write them in `AnchorSystems::AnimatePose`; (3) these lenses are the ready-made adapters; (4) `AnchorPose` tweens and `Hinge` are mutually exclusive per entity — `hinge_to_pose` (Phase 4 as shipped) overwrites the **whole** `AnchorPose` every frame, translation reset to `Vec3::ZERO`, so an `AnchorPoseLens` tween on a `Hinge`-carrying entity is silently lost (debug builds warn). Document this on both lenses. bevy_animation `AnimatableProperty` impls are a possible later feature, not this phase.
- Example `crates/hana_valence/examples/staggered_unfold.rs` (dev-dependencies: full `bevy` with rendering, `bevy_tween`): 5 quads with hand-filled geometry in a strip, each hinge tweened to its target angle with per-entity start delays so the unfold propagates down the chain. Concrete per-entity tween targeting and delay composition — this is the worked proof that "any animator" is real, not a claim.

**Files:**
- `Cargo.toml` (workspace root) — `bevy_tween` workspace dependency.
- `crates/hana_valence/Cargo.toml` — `tween` feature + optional dep + example's dev-dependencies.
- `crates/hana_valence/src/tween.rs` — new.
- `crates/hana_valence/examples/staggered_unfold.rs` — new.

**Constraints from prior phases:** `Hinge`/`hinge_to_pose` (Phase 4), `AnchorPose` (Phase 1), scheduling contract (Phase 3: consumers configure `AnchorSystems` in `PostUpdate` before propagate; the example does its own registration since valence has no Plugin). Any schedule that runs `resolve_anchors` must also `init_resource::<ResolveDiagnostics>()` — a missing resource is a loud Bevy failure (Phase 3). Phase 4 shipped facts: within `AnimatePose`, bevy_tween's apply systems must run **before** `hinge_to_pose` (mirror the `.chain()` in `hinge.rs`'s accordion test) or every fold lags one frame; `hinge_to_pose`'s query requires `AnchorPose` to already exist — the example's hinged quads must spawn with `AnchorPose::default()`.

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo check -p hana_valence --no-default-features` still green; `cargo nextest run -p hana_valence --all-features` green; the example runs and visibly unfolds (manual spot-check); fmt + `clippy` skill clean.

#### Retrospective

**What worked:** Fast-path dispatch; one codex pass, all gates green on both codex's run and an independent re-run (25 tests, no-default-features check, clippy `-D warnings`, fmt). `bevy_tween 0.13.0` is the Bevy 0.19 release — verified against the registry manifest (`[dependencies.bevy] version = "0.19.0"`), not just the upstream README table. Blind review: APPROVE, no findings.

**What deviated from the plan:** Nothing. bevy_tween 0.13's adapter surface is the `Interpolator` trait — `interpolate(&self, item: &mut Self::Item, value: CurrentValue, previous_value: PreviousValue)` where `CurrentValue`/`PreviousValue` are `f32` aliases — matching the plan's "lens/interpolator trait for the version chosen".

**Surprises:** bevy_tween 0.13 wiring needs three distinct calls, now demonstrated in `staggered_unfold.rs:90-129`: `DefaultTweenPlugins::<()>::in_schedule(PostUpdate)` (turbofish required — `TimeCtx` generic), `configure_sets` relocating `TweenSystemSet::ApplyTween` into `AnchorSystems::AnimatePose`, and one `component_tween_system::<Lens>()` registered per lens via `add_tween_systems(PostUpdate, …)`. Per-entity delays compose as `sequence((forward(delay), tween(duration, ease, target.with(lens))))`.

**Implications for remaining phases:**
- Phase 9's `box` "drive every hinge (manual system or `tween` feature)": if tween is chosen, copy the `staggered_unfold.rs` wiring verbatim (plugin turbofish + set relocation + per-lens system registration + apply-before-`hinge_to_pose` ordering).
- Phases 6–8 unaffected — nothing downstream depends on the `tween` feature.

#### Phase 5 Review

- Phase 7 Work Order: constraints add the tween-consumer facts — a consumer app driving panel hinges via `HingeAngleLens` must itself order `TweenSystemSet::ApplyTween` before `hinge_to_pose` (public fn, orderable; the plugin's `AnchorSystems` `configure_sets` is additive, no conflict), and relocating `ApplyTween` is app-global (retimes every tween in the app); step 9's docs/CHANGELOG record now covers both.
- Phase 8 Work Order: the arrangement drive system is specified to write every member's `Hinge.angle` unconditionally every frame (same last-writer policy as `hinge_to_pose`), making a direct `HingeAngleLens` tween on a Member silently lost — mutual exclusion documented; the arrangement's animatable surface is its params (`fold`, `lean`). Re-expressed `panel_anchoring` demos stay manual drivers — no `bevy_tween` dev-dependency in `bevy_diegetic`.
- Phase 9 Work Order: box-net bullet now points at `staggered_unfold.rs` as the tween wiring reference if the `tween` feature is chosen, notes the relocation is app-global, and Files adds the conditional `[[example]] required-features = ["tween"]` manifest block (default `cargo build --examples` fails without it).
- No remaining phase found redundant, mis-scoped, or invalidated; Phase 6 unchanged. Phase 5's fallback language (git-rev pin / deferral) is moot but kept — the Work Order is the archive record.

---

### Phase 6 — diegetic provider bridge + vocabulary maps (additive)  · status: done (uncommitted)

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

#### Retrospective

**What worked:** Fast-path dispatch, one codex pass, all gates green on codex's run (build, `cargo nextest run` 1132 passed/4 skipped, fmt, `taplo fmt`, full `clippy` skill). Independent main-agent review + blind codex review both confirmed correct + complete for scope. Size math and frame verified against source: `PanelPlane::from_panel` (`anchor_geometry.rs:396-399`) computes `world_width()*right_scale` / `world_height()*up_scale`; the provider deliberately uses the **un-scaled** `world_width()`/`world_height()` centered ±half about origin, because the valence resolver re-applies `GlobalTransform` — exactly the "centered-local, no transform baking" precondition. `world_width()`/`world_height()` (`diegetic_panel.rs:496/519`) are the authored-meter accessors.

**What deviated from the plan:** (1) The provider reimplements the W×H→half-extent math rather than literally calling `from_world_panel` (which bakes transform, uses top-left origin, and applies the panel's own anchor offset — all wrong for centered-local output). This matches the Spec's "reuse the size math behind `from_world_panel`", not the whole function. (2) Strict lints (`missing_docs`/clippy deny) reject the defined-but-unscheduled system as dead code; codex added `#[cfg_attr(not(test), expect(dead_code, ...))]` on the `mod valence_provider;` declaration in `panel/mod.rs`. This is scaffolding tied to the double-writer guard — **Phase 7 must delete that attribute when it registers the system**, or the `expect` becomes unfulfilled and fails under deny. (3) `TryFrom<AnchorId> for Anchor` uses `type Error = AnchorId` — the error returns the unmapped id itself.

**Surprises:** Codex's blind review flagged one latent gap: `write_panel_anchor_geometry` skips non-world panels with `continue` without clearing any prior `ResolvedAnchorGeometry`. It is unreachable today — the only runtime coordinate-space reassignment is screen→world (`diegetic_panel.rs:1022`, the screen-conversion path); no world→screen transition exists — so a panel never sheds world geometry while keeping stale valence geometry. Relevant only if a future path adds world→screen toggling.

**Out-of-band (feature-model flip, user-directed this session):** The `tween` feature model changed from opt-in to opt-out so `cargo run --example staggered_unfold` runs with no flags (Cargo has no per-example feature activation — `required-features` gates, never enables, so the example runs bare only if `tween` is a default feature). `hana_valence/Cargo.toml`: `default = ["tween"]`. Root `[workspace.dependencies]`: `hana_valence = { path = "crates/hana_valence", default-features = false }` — a member cannot set `default-features = false` on an inherited workspace dep (Cargo errors), so it lives on the workspace entry; `bevy_diegetic` inherits it plainly and pulls **no** `bevy_tween` (verified: `cargo tree -p bevy_diegetic -i bevy_tween` empty). Any workspace member wanting tween adds `features = ["tween"]`.

**Implications for remaining phases:**
- Phase 7 must remove the `#[cfg_attr(not(test), expect(dead_code, ...))]` on `mod valence_provider;` when it schedules `write_panel_anchor_geometry` (unfulfilled-expectation error otherwise under deny).
- Phase 7 registers the provider in `FillGeometry`; the non-world `continue` never leaves stale geometry given the current screen→world-only conversion — leave as-is, but if world→screen toggling is ever added, clear `ResolvedAnchorGeometry` on the non-world branch.
- Phase 9 box: tween is now a **default** feature. The `[[example]] required-features = ["tween"]` block is still needed, but its purpose is keeping `--no-default-features --examples` green, not the default build (which now has tween on).

#### Phase 6 Review

- **Phase 7 (user decision — screen/world warn-spam):** the generic `resolve_anchors` (`resolve.rs:98`) has a fixed, unfilterable `Query<(Entity, &AnchoredTo)>`, so putting one relation on both screen and world attachments would make every screen-anchored panel a per-frame missing-geometry skip + `tracing::warn!` (the deleted `world_anchoring.rs` `HandledByScreenResolver` scope was what prevented this). Resolved via **unified authoring, two positioners** (user-approved): the sugar bundle carries only the shared authored record (`PanelAttachmentAuthored` + offset); the insert observer adds `hana_valence::AnchoredTo` for **world-space panels only**; the screen placer builds candidates from `PanelAttachmentAuthored`. §1, §4, Constraints, and the acceptance gate (screen attachments never enter `ResolveDiagnostics`) updated.
- **Phase 7:** added the mandatory deletion of the `#[cfg_attr(not(test), expect(dead_code, ...))]` on `mod valence_provider;` at registration (unfulfilled-expectation error otherwise); corrected the registration site from the nonexistent `DiegeticPanelPlugin` to `HeadlessLayoutPlugin` (`panel/mod.rs:166`).
- **Phase 8:** corrected `DiegeticPanelPlugin` → `HeadlessLayoutPlugin` in Constraints.
- **Phase 9 + Delegation Context:** corrected the `required-features = ["tween"]` rationale and the Stack line to the opt-out feature model (tween is a default feature; workspace consumes with `default-features = false`, so `bevy_diegetic` stays tween-free; `--no-default-features` is the gate that exercises tween-off).
- No remaining phase found redundant or invalidated; the latent non-world `continue` stale-geometry item stays documented as unreachable (screen→world-only conversion today), noted for Phase 7.

---

### Phase 7 — the atomic swap: relation, resolvers, placers, examples  · status: done (uncommitted)

#### Work Order

**Goal:** bevy_diegetic runs entirely on hana_valence — one stored relation (`AnchoredTo`), valence resolver on the world path, screen placer on the valence skeleton, diegetic copies deleted — with all existing behavior and tests preserved.

This phase is intentionally large because it cannot be split: the sugar bundle flips from the old `AnchoredToPanel` relationship to a shared authored record (`PanelAttachmentAuthored`) that feeds **two coordinate-space positioners** — the world resolver and the screen placer — off one authoring surface, so the world swap, the screen re-point, and the sugar rewrite must land together (double-writer/atomicity invariant). Author once (`AnchoredToPanel::new`, coordinate-space-agnostic); each positioner reacts to that same authoring through its own coordinate-space tag (world attachments additionally carry `hana_valence::AnchoredTo`; screen attachments do not — see §1). Work the checklist in order; the tree compiles only at the end — commit-sized intermediate states are not expected within this phase.

**Spec:**

1. **Sugar rewrite** (`crates/bevy_diegetic/src/panel/anchoring.rs`):
   - Delete the `AnchoredToPanel` relationship component and `PanelsAnchoredHere` (and the `pub use` re-export; consumers now read `hana_valence::AnchoredHere`).
   - New `AnchoredToPanel` = insert-only `#[derive(Bundle)]` with **private fields** carrying the **shared authoring** every attachment gets regardless of coordinate space: the authored component `PanelAttachmentAuthored { target: Entity, source: Anchor, target_anchor: Anchor }` (new, this file) and the stored typed offset `PanelAnchorOffset`. Constructor `new(target: Entity, source: Anchor, target_anchor: Anchor) -> Self`; preserve the existing builder surface (offset et al.) so example call sites change minimally. No hana_valence type appears in any public signature.
   - **Coordinate-space positioner tag (the screen/world split).** `hana_valence::AnchoredTo` is the tag the world resolver reads — it must land on **world-space attachments only**. It is therefore NOT an unconditional bundle field: the insert-time observer/hook on the sugar bundle (the same one that captures the authored `Transform`, §3) reads the anchored entity's `CoordinateSpace` and, for world-space panels, inserts `hana_valence::AnchoredTo` (anchors lowered via `From<Anchor>`, offset via `with_offset`). Screen-space panels keep only the shared authoring; the screen placer positions them from `PanelAttachmentAuthored` by entity (§4). Rationale: the generic `resolve_anchors` query is a fixed, unfilterable `Query<(Entity, &AnchoredTo)>` (`resolve.rs:98`) — any screen attachment carrying `AnchoredTo` classifies as a missing-geometry skip and the skeleton `tracing::warn!`s it every frame (the deleted `world_anchoring.rs` `validate_candidate`/`HandledByScreenResolver` scope is what prevented this; the generic resolver has no coordinate-space concept, so the split moves to the tag).
   - Delete `PanelAnchorPose`; all consumers use `hana_valence::AnchorPose` directly (compose, no re-export).
   - `PanelAnchorOffset` stays diegetic (three `Dimension`s — unit/DPI/target-size knowledge does not move).
2. **Offset lowering (world)** — new system in `anchoring.rs` or `valence_provider.rs`: per frame, for world-space attachments with a `PanelAnchorOffset`, resolve against the live target exactly as today (`target_offset_meters` `world_anchoring.rs:355`: `to_layout_units(target.layout_unit())`, `target_size/panel_size` normalization) and apply the world Y-flip (today's `right*x − up*y + normal*z`, L350-351, becomes lowered `Vec3(x_m, −y_m, z_m)` since the valence resolver applies `base * offset` with no flip); write `hana_valence::ResolvedAnchorOffset`. One Vec3 write per frame, no relation re-insert. Screen-space attachments resolve their typed offset inside the placer callback as today (zero `ResolvedAnchorOffset` writes on the screen path).
3. **World path swap**: delete `world_anchoring.rs` (resolver, classify, placement, `WorldAnchorResolveDiagnostics` registration — the world path dissolves into provider + valence resolver), **except detach-restore, which is ported not deleted**: move `AnchoredWorldPanelPose` (`world_anchoring.rs:304`) and `restore_inactive_world_panel_poses` (`world_anchoring.rs:27`) into `anchoring.rs`. The old capture point (inside placement, `world_anchoring.rs:137-139`) disappears with the file; capture the authored `Transform` instead when the anchor relation is inserted (component-insert observer/hook on the sugar bundle — same value the old code captured at first placement, since the resolver is the sole subsequent writer). Keep the restore system scheduled in `PostUpdate` before `AnchorSystems::Resolve`; valence's keep-last fallback stays as designed (detach-restore is diegetic product behavior, not resolver mechanics). In `HeadlessLayoutPlugin` (`panel/mod.rs:166` — the registration site holding the world-resolver block being deleted; there is no `DiegeticPanelPlugin`): configure `AnchorSystems` (`FillGeometry` and `AnimatePose` before `Resolve`, `Resolve` in `PostUpdate` before `TransformSystems::Propagate`); **delete the `#[cfg_attr(not(test), expect(dead_code, ...))]` on `mod valence_provider;` (`panel/mod.rs:19`)** — scheduling the system makes its items live under `not(test)`, turning that `expect` into an unfulfilled-lint-expectation hard error under the workspace deny lints; register `write_panel_anchor_geometry` (Phase 6) in `FillGeometry`, the offset-lowering system before `Resolve`, `hana_valence::resolve_anchors` in `Resolve` and `hinge_to_pose` in `AnimatePose` **after** `PanelSystems::AnimateAnchorPose` (plain systems: `.add_systems(PostUpdate, resolve_anchors.in_set(AnchorSystems::Resolve))`, `hinge_to_pose.in_set(AnchorSystems::AnimatePose).after(PanelSystems::AnimateAnchorPose)` — angle drivers write `Hinge.angle` in that set; unordered = one-frame fold lag) + `init_resource::<hana_valence::ResolveDiagnostics>()` (the shipped alias for `AttachmentResolveDiagnostics<ResolveSkip>`), and hana_valence type registrations; order `PanelSystems::AnimateAnchorPose` inside `AnchorSystems::AnimatePose`.
4. **Screen placer re-point** (`screen_space/anchoring/`): screen attachments carry no `AnchoredTo` (§1), so `resolve.rs` builds `AttachmentResolveCandidate<AnchorResolveSkip>` from `PanelAttachmentAuthored` (+ its `target`) on screen-space entities and calls `hana_valence::resolve_attachments`; `candidate.rs` classification reads `PanelAttachmentAuthored` (+ `PanelAnchorOffset`) by entity for its `Anchor`-typed math (the `TryFrom<AnchorId>` fallback + unmappable-id skip variant is only needed if a candidate is ever built from an `AnchoredTo` payload — with authoring-sourced candidates the `Anchor` is already in hand); `placement.rs` callback body unchanged in spirit. The alias `AnchorResolveDiagnostics = AttachmentResolveDiagnostics<AnchorResolveSkip>` keeps working (same generic name, now from hana_valence).
5. **Delete** `crates/bevy_diegetic/src/panel/attachment_resolver.rs` (the last consumer just switched).
6. **Collateral updates**: `panel/gizmos.rs` (reads of `PanelsAnchoredHere`/pose types → `AnchoredHere`/`AnchorPose`); any `panel/mod.rs` re-exports; `panel/anchor_geometry.rs` untouched except doc references.
7. **Tests**:
   - Port from `world_anchoring.rs` into the new seams: `world_anchoring_respects_source_scale_and_parent_rotation` (same expected `(1.5,1.25,0)`) and `pose_written_in_animation_set_lands_this_frame` now run through provider + lowering + valence resolver in a diegetic test app.
   - `anchoring.rs`: replace the relationship tests with sugar-level ones (bundle insert creates `AnchoredTo` + `AnchoredHere`; reverse-index mechanics are covered valence-side since Phase 1). Update `anchor_types_are_registered_with_expected_reflect_component_data` for the new registration set (valence types registered by the plugin; split rule per Delegation Context).
   - `point_offsets_resolve_to_screen_pixels` (`screen_space/anchoring/resolve.rs:798`) must stay green — it is the DPI Pt→px guard for the lowering split.
   - New: world offset lowering test — a `PanelAnchorOffset` in Pt/target-relative units tracks a target resize/DPI change frame-to-frame via `ResolvedAnchorOffset`.
   - New: detach-restore test — anchor a world panel, let the resolver move it, remove the anchor relation, assert the authored `Transform` is restored and `AnchoredWorldPanelPose` is removed.
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

**Constraints from prior phases:** Phase 1–4 valence API (`AnchoredTo::new(target: Entity, source_anchor: AnchorId, target_anchor: AnchorId).with_offset(Vec3)` — private-target, bundle-friendly; `AnchoredHere`, `AnchorPose`, `ResolvedAnchorOffset`, `AnchorSystems`, `resolve_anchors`, `hinge_to_pose`, `resolve_attachments`, `AttachmentResolveDiagnostics<R>`, `ResolveSkip`). Phase 2 facts checked against the diegetic side: `resolve_attachments` requires `R: Debug` — both diegetic skip enums already derive it (`candidate.rs:121`, `world_anchoring.rs:309`), no code change; `AttachmentResolveReasons` kept its name and field shape, so `placement.rs:210` ports unchanged; the skeleton's `record` emits `tracing::warn!` on every repeated skip, so the re-pointed screen placer gains per-frame warnings for persistent skips it never had (accepted; throttling deferred per Phase 2 retrospective); Phase 6 provider (`write_panel_anchor_geometry`, `From<Anchor>`, `TryFrom<AnchorId>`, `PanelAttachmentAuthored` is defined HERE not in Phase 6); the Phase 6 `mod valence_provider;` decl carries a `#[cfg_attr(not(test), expect(dead_code, ...))]` (`panel/mod.rs:19`) that this phase must **delete** when it schedules the provider — scheduling makes the items live and the `expect` becomes an unfulfilled-lint-expectation hard error under deny; **world/screen positioner split** — the generic `resolve_anchors` has a fixed, unfilterable `Query<(Entity, &AnchoredTo)>` (`resolve.rs:98`) and no coordinate-space concept, so `hana_valence::AnchoredTo` must be inserted on **world-space attachments only** (a screen attachment carrying it classifies as a missing-geometry skip and the skeleton `tracing::warn!`s it every frame — the deleted `world_anchoring.rs` `HandledByScreenResolver` scope is what prevented this); the sugar bundle therefore carries only shared authoring (`PanelAttachmentAuthored` + offset) and the insert observer adds `AnchoredTo` by `CoordinateSpace` (§1), while the screen placer builds its candidates from `PanelAttachmentAuthored` (§4); valence exposes no Plugin — this phase owns all registration; the valence resolver applies no Y-flip and no unit resolution (both live in the lowering written here). Phase 3 shipped facts: `resolve_anchors` is a parameterized system and a missing `ResolveDiagnostics` resource is a loud Bevy missing-resource failure (not a silent no-op) — `init_resource` is mandatory; `ResolveSkip` is `#[non_exhaustive]` with 11 variants including the unplanned `MissingSourceTransform`/`MissingTargetTransform`, so any diegetic-side `match` needs a wildcard arm; `resolve_anchors` takes an unfiltered `Query<&mut Transform>` (broader than the old panel-filtered diegetic resolver) — every other `Transform`-writing system in `PostUpdate` must be ordered against `AnchorSystems::Resolve` or the ambiguity accepted deliberately (expect B0002-style warnings otherwise, and no parallelism with Transform writers regardless). Phase 4 shipped facts: `hinge_to_pose` overwrites the **whole** `AnchorPose` (translation zeroed) every frame — an entity must not carry `Hinge` while another system writes its `AnchorPose` (writes are lost; debug builds warn), so the ported demo fold systems drive hinged entities via `Hinge.angle` only, and spin/morph-style direct `AnchorPose` writers stay `Hinge`-free; `hinge_to_pose`'s query requires `AnchorPose` to already exist (`Query<(Entity, &Hinge, &ResolvedAnchorGeometry, &mut AnchorPose)>` — no insert, no warn when absent), so hinged entities must spawn with `AnchorPose::default()`; the unconditional whole-pose write also means `Changed<AnchorPose>` is hot every frame for every hinged entity — never gate a diegetic-side system on it. Phase 5 shipped facts: a consumer app driving panel hinges via `HingeAngleLens` must itself order `TweenSystemSet::ApplyTween` before `hinge_to_pose` (else nondeterministic or one-frame-lagged folds) — `hinge_to_pose` is a public fn orderable by its system type, and the plugin's `configure_sets` on `AnchorSystems` is additive so a consumer relocating `ApplyTween` into `AnimatePose` does not conflict; note in the step-9 breaking/docs record that this ordering is the consumer's job, and that relocating `TweenSystemSet::ApplyTween` is app-global — it retimes every tween in the app (camera moves, UI), not just valence-lens tweens.

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo nextest run --all-features --workspace` green including the ported/updated named tests and the new lowering test; `panel_anchoring` example launches with hinge/spin/morph behavior parity (manual spot-check); **screen-anchored panels produce no per-frame world-resolver skip/warn — a screen-space attachment never appears in `ResolveDiagnostics`** (guard with a test: a screen attachment leaves `ResolveDiagnostics` skip-count unchanged after a resolve pass); fmt + `clippy` skill clean.

#### Retrospective

**What worked:** The atomic swap landed in one pass — world path dissolved into provider + valence resolver, screen placer re-pointed onto `PanelAttachmentAuthored`, detach-restore ported (not deleted), all named tests green (1115 pass, 4 skip). The world/screen positioner split via the coordinate-space-conditional `AnchoredTo` insert held: screen attachments produce no per-frame resolver warn.

**What deviated from the plan:**
- **Reflect auto-registration was made workspace-global**, not phase-local. Root `Cargo.toml` now enables `bevy`'s `reflect_auto_register`; concrete (non-generic) `register_type` calls were swept out across the whole workspace (lagrange `HomePose` structs, fairy_dust OIT settings, the conversion example's `PanelConversionRequest`). This was the resolution to blocker #3 from the dual review, escalated to a workspace decision by the user.
- **Conversion staleness (blocker #1) resolved via Option C — a `PanelSpace` mirror component**, not the originally-implied approach. `On<Insert>` doesn't fire on the in-place `Mut<DiegeticPanel>` field mutation the conversions use, so a mirror-discriminant component synced at 4 sites drives an `On<Insert, PanelSpace>` reconcile observer. See the "deduplicating PanelSpace" appendix — the duplication is a deferred cleanup, not a settled end state.
- **`Sizing` gained `Reflect`** (blocker #2 support), making `coordinate_space` BRP-inspectable through `DiegeticPanel`.

**Surprises:**
- **Type aliases hide genericness.** `OrbitCamInputMode`/`FreeCamInputMode` (= `InputMode<K>`) and `OrbitCamInput`/`FreeCamInput` (= `InputIntent<K>`) look concrete but are generic monomorphizations — they do NOT auto-register. The blind reviewer's registration test (`reflect_input_mode_types_are_registered`) caught an over-broad sweep that removed them; all four were restored. Rule confirmed: auto-register covers non-generic types only; generic monomorphizations and `register_type_data` foreign patches (the OIT `ReflectComponent` patch) stay explicit.
- `OrderIndependentTransparencySettings` derives `Reflect` but not `#[reflect(Component)]`, so auto-register registers the type but not its `ReflectComponent` data — the `register_type_data::<_, ReflectComponent>` patch is still required and still valid (auto-register's draining registry exists at App creation, satisfying the ordering `register_type_data` needs).

**Implications for remaining phases:**
- Phases 8–9 add `hana_valence` types and example schedules. Under workspace auto-register, **non-generic** new types (arrangement components like `Member`, `Accordion`, `Strip`) no longer need explicit `register_type`; any **generic** ones still do. Work Orders that say "register hana_valence types" should be read through this split.
- The `PanelSpace` mirror is diegetic-internal and does not touch the valence contract; Phases 8–9 are unaffected by it except that arrangement code reading coordinate space should prefer querying `PanelSpace` over reaching into `DiegeticPanel` (keeps the future Option-2 promotion cheap).

---

### Phase 8 — arrangements: tiling rule, Accordion/Strip, Member observer  · status: done (uncommitted)

#### Work Order

**Goal:** Named arrangements place and fold ordered sets of anchored tiles through one drivable parameter, with mid-animation spawn join.

**Spec:**

`src/arrange.rs`:
- **Tiling rule** (per-shape input, ~10 lines per shape): `next_edge(i) -> (Edge, Edge)` (which source/target edges seat tile `i` on tile `i−1`) and `rest_delta(i) -> f32` (rest **angle** in radians about the shared hinge edge between neighbors — a scalar, NOT a pose: `hinge_to_pose` owns the whole `AnchorPose` on hinged tiles, so rest orientation must compose into `Hinge.angle`; the drive system writes `Hinge.angle = rest_delta(i) + fold_contribution(i)`; user-approved decision, Phase 4 review). The drive system writes every member's `Hinge.angle` unconditionally every frame (same last-writer policy as `hinge_to_pose`'s whole-pose write — deterministic stomping, not intermittent) — so a direct `HingeAngleLens` tween on an arrangement Member is silently lost, the same mutual-exclusion class as `AnchorPoseLens` vs `Hinge`; document it: the animatable surface of an arrangement is its params (`fold`, `lean`), not member hinges. Folding is universal (rotate about the shared edge — Phase 4); tiling is the only per-shape part: quads share parallel edges (straight strip, rest 0), triangles alternate up/down, hexes zigzag.
- **Arrangement** = a named rule over an ordered set owning `placement(i) -> (AnchoredTo, Edge /* hinge edge */, rest angle)` plus exposed drivable params:
  - `Accordion { fold: f32 /* 0..1 */, lean: f32, pattern: FoldPattern }` — `FoldPattern::{Accordion, Coil}` encodes the fold distribution (uniform-alternating vs cumulative), ported from the example's `FoldPattern` (`panel_anchoring/hinge.rs:49`) rather than re-invented. Drive `fold` and the whole set folds.
  - `Strip {}` — static straight layout.
- A quad tiling rule ships in-crate for tests and the quad-based recipes (quads are plain geometry, no shape crate needed). Ordering over the set = `AnchoredHere` insertion order (deterministic by contract).
- **Composition (arrangements drive both plain valence entities AND diegetic panels, world and screen).** Arrangement logic lives in `hana_valence` and must **not** hard-insert a raw `AnchoredTo` — it produces placement as **data** (`placement(i) -> (AnchoredTo, hinge Edge, rest angle)`); the **consumer** decides how to apply it. Two consumers:
  - **Plain valence entities** (box net, Phase 9): insert the raw relation directly. Screen concept does not apply.
  - **Diegetic panels**: a **diegetic-side adapter** (`crates/bevy_diegetic`, not the valence crate) translates each placement into the Phase 7 sugar path — `PanelAttachmentAuthored` (+ offset), plus the coordinate-space-conditional `AnchoredTo` (**world panels only**). This preserves the Phase 7 world/screen split: a **world** panel gets `AnchoredTo` and is positioned by the 3D resolver; a **screen** panel gets `PanelAttachmentAuthored` only (**no** `AnchoredTo`) and is positioned by the screen placer, so the generic `resolve_anchors` never sees it and never `tracing::warn!`s (the split the adapter MUST NOT break — a raw `AnchoredTo` on a screen panel is the exact per-frame warn-spam Phase 7 eliminated, and `on_panel_space_changed` only strips the tag when `PanelAttachmentAuthored` is present).
  - `Member` references its arrangement by `Entity` (concrete — not generic over the arrangement kind — so it auto-registers, no `register_type`).
  - Offsets: the generic `Vec3` offset is already first-class valence (`AnchoredTo::with_offset` → `ResolvedAnchorOffset`) and arrangements/consumers use it freely; only **unit-aware** offset lowering (Pt/DPI/target-relative → `Vec3`) stays diegetic (the adapter's job for panels).
- **Fold/screen composition (RESOLVED — user decision, "B"): arrangements produce connected folds only; screen-locking is per-arrangement, not per-tile.** A `Hinge` fold keeps tiles joined at their shared edges, which requires one shared coordinate frame. For a **screen** arrangement the diegetic adapter screen-attaches **only the arrangement root** through the Phase 7 sugar path (`PanelAttachmentAuthored`, no per-member `AnchoredTo`); the members fold in 3D **off that root** via the existing world fold path (`Hinge` → `hinge_to_pose` → `AnchorPose` + ordinary `Transform` propagation). The screen placer therefore needs **no** per-member 3D rotation and its "cannot leave the plane" projection (`screen_in_plane_angle`, `screen_space/anchoring/projection.rs`) stays as-is — it positions the root; the fold rides the transform hierarchy. Independent per-tile screen tilt (each tile pinned to its own pixel slot and tilting in place — the "Option A" alternative) is **not** an arrangement: it is individual panels, each with its own screen attachment plus a per-panel rotation, already expressible without any arrangement machinery, and is **out of Phase 8 scope**. Do not add a per-space branch to the arrangement interface — the `placement(i)`/`Hinge.angle` surface is space-agnostic and the adapter's root-vs-member split is the only screen-aware decision.
- **Member spawn-marker observer**: spawn a tile with `Member { arrangement }`; the observer reads the current member count, assigns index `i`, applies `placement(i)` **via the consumer path** — raw relation for plain valence entities, the diegetic adapter for panels — plus `AnchorPose::default()` + hinge at its rest angle, and seats it at the live fold angle if a fold is in flight — a new instance joins mid-animation and animates with the set. It must NOT assume contiguous order indices (the example's `collect_tiles_by_order`, `anchor_demo.rs:828`, has that bug — do not port it). Geometry-fill timing: if a spawned tile's geometry is not yet filled on its spawn frame, defer its first placement one frame rather than resolving at the authored pose and snapping.
- Docs (`lib.rs`): the naming-tiers workflow — (1) generated/procedural: no names, ids derived from adjacency; (2) hand-authored regular shape: provider names (`Anchor::TopLeft`) if offered; (3) one-off: raw `AnchorId` — with a one-line pick guide; recipes never hardcode ids (a reusable accordion asks geometry which edge it shares with its parent).
- Update `examples/panel_anchoring`: re-express the hinge/spin/morph demos on `Accordion` + a driver where they map (they stay examples, not library code). The drivers stay manual systems writing `Hinge.angle`/`AnchorPose`/arrangement params — do NOT add a `bevy_tween` dev-dependency to `bevy_diegetic`.

Tests: 5-quad accordion folds under `fold` with both patterns (assert alternating vs cumulative angles); `Member` spawned mid-fold seats at the live angle; placement with non-contiguous indices works; `Strip` rest layout positions match hand-computed seats.

**Files:**
- `crates/hana_valence/src/arrange.rs` — new (arrangement rules, `placement(i)` data, `Member`, pure-valence consumer).
- `crates/hana_valence/src/lib.rs` — module + naming-tiers docs.
- `crates/bevy_diegetic/src/panel/` — new **diegetic-side arrangement adapter** that applies `placement(i)` through the Phase 7 sugar path (`PanelAttachmentAuthored` + world-only `AnchoredTo`), so arrangements drive diegetic panels (world and screen) without breaking the screen/world tag split.
- `crates/bevy_diegetic/examples/panel_anchoring/*` — recipe port. Screen-fold composition is settled (decision "B" — root-anchored connected fold), so a screen arrangement is now expressible by screen-attaching only the arrangement root; a screen-fold demo is optional, not required for this phase.
- Read-only reference: `crates/bevy_diegetic/examples/panel_anchoring/hinge.rs` (`FoldPattern` L49, `crease_sign` L257), `anchor_demo.rs:828`; Phase 7 sugar seam `crates/bevy_diegetic/src/panel/anchoring.rs` (`PanelAttachmentAuthored`, `on_panel_space_changed`) + `panel/coordinate_space.rs` (`PanelSpace`).

**Constraints from prior phases:** full valence stack live in diegetic since Phase 7 (provider fills geometry; `AnchorSystems` configured in `HeadlessLayoutPlugin::build`, `panel/mod.rs` — `configure_sets` + system/observer registration span, not a single line; set names `AnchorSystems::{FillGeometry,AnimatePose,Resolve}` and `PanelSystems::AnimateAnchorPose` are accurate as-built; `Hinge`/`hinge_to_pose` drive folds); arrangement fold drivers write `Hinge.angle` only, never `AnchorPose`, on hinged tiles — `hinge_to_pose` (Phase 4 as shipped) overwrites the whole pose (translation zeroed) every frame; `hinge.rs`'s test module is the wiring reference for accordion tests (drive system + `hinge_to_pose` chained in `AnimatePose`, then `resolve::run_resolve`; the shared helpers are file-scope `#[cfg(test)] pub(crate)` fns in `resolve.rs`, not a public tests module); hinged members must spawn with `AnchorPose::default()` — `hinge_to_pose`'s query requires it and silently skips entities without it, so `placement(i)`/the Member observer inserts it; `AnchoredHere` iteration order = insertion order; observers are `bevy_ecs`-level (no `bevy_app` — the observer registers via `World::add_observer` or is exported for consumers to register). Bare test schedules running `resolve_anchors` must `init_resource::<ResolveDiagnostics>()` (loud failure otherwise, Phase 3). Spawn members with `Transform` (Bevy 0.19 required components supply `GlobalTransform`) — a member without one skips as `MissingSourceTransform`; the observer's defer-first-placement rule covers `MissingSourceGeometry` and `MissingSourceTransform` alike.

**Constraints from Phase 7 as-built (reflect registration + coordinate space):**
- **Reflect auto-registration is workspace-global.** Root `Cargo.toml` enables `bevy`'s `reflect_auto_register`, and `crates/bevy_diegetic/src/panel/` carries **zero** explicit `register_type` calls — all valence types register implicitly. **New non-generic arrangement types (`Accordion`, `Strip`, `Member`, ported `FoldPattern`) must NOT get explicit `register_type`** — deriving `Reflect` is sufficient. Explicit `register_type`/`register_type_data` is required **only** for generic monomorphizations and foreign `ReflectComponent` patches. Type aliases can hide genericness (this bit the Phase 7 sweep on `OrbitCamInputMode = InputMode<K>`), so any arrangement type that is generic over the arrangement kind still needs explicit registration of each monomorphization.
- **Derives:** `Member`/`Accordion`/`Strip` are mutable driver inputs → `#[derive(Component, Reflect, ...)] #[reflect(Component, ...)]`. None is a relationship, so the `AnchoredTo`/`AnchoredHere` type-only-registration subtlety from Phase 1 does NOT apply — do not copy the relation split.
- **Coordinate space:** the `panel_anchoring` example re-expression should read a panel's world/screen space via `Query<&PanelSpace>` (the queryable mirror added in Phase 7, `panel/coordinate_space.rs`), not by reaching into `DiegeticPanel`.

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo nextest run --all-features` green including the four arrangement tests; no-default-features check green; `panel_anchoring` still behavior-par (spot-check); **the diegetic adapter's screen path preserves the Phase 7 no-warn guarantee — a screen panel placed by an arrangement never carries `AnchoredTo` and never appears in `ResolveDiagnostics`** (guard with a test, same shape as Phase 7's screen-attachment skip-count guard); fmt + `clippy` skill clean.

#### Retrospective

**What worked:**
- Arrangement library (`crates/hana_valence/src/arrange.rs`) landed with the space-agnostic `placement(i)` data surface: `TilingRule` trait (`next_edge`/`rest_delta`), `QuadTiling`, `Accordion { fold, lean, pattern }` with `FoldPattern::{Accordion, Coil}`, `Strip`, `Member`, the spawn-marker observer (`on_member_added`) + `assign_member_indices` catch-up system, `apply_member_placements` (defer-until-geometry-ready), and `drive_arrangement_hinges` (unconditional `Hinge.angle` write). All five required tests pass; full suite 1122 pass, 4 skip.
- Decision B (connected screen fold) is implemented as specified after the fix pass: a screen arrangement screen-attaches only the root; members are world panels that fold off it via `Hinge`→`hinge_to_pose`→`AnchorPose`+transform propagation. The no-warn guarantee holds (screen panels never carry `AnchoredTo`, never enter `ResolveDiagnostics`).

**What deviated from the plan:**
- **Phase 7's atomic swap actually shipped in this delegation, not before it.** HEAD (`46533a3f`) still ran the old `world_anchoring::resolve_world_space_panel_attachments` resolver and kept `valence_provider` as dead code; codex detected that Phase 8's spec assumes a live valence stack and performed the entire Phase 7 swap (delete `world_anchoring.rs` + `attachment_resolver.rs`, re-point the screen placer onto `PanelAttachmentAuthored`, wire `AnchorSystems` into `HeadlessLayoutPlugin::build`, `reflect_auto_register` workspace-global with cross-crate `register_type` cleanup in `bevy_lagrange`/`fairy_dust`) as part of Phase 8. Phase 7's status/retrospective describe that shipped state; both phases sit uncommitted in one working tree.
- **Ordering uses a dedicated `ArrangementMembers { entities: Vec<Entity> }` in `arrange.rs`, not `AnchoredHere` insertion order** (the Work Order's stated ordering source). Accordion members chain to each *other* (member `i` anchors to member `i−1`), so no single parent's `AnchoredHere` list holds the whole ordered set. Accepted as correct — the Work Order's "ordering = `AnchoredHere` insertion order" assumption was wrong for chained arrangements.
- **The quad tiling rule is hardcoded in `member_placement` (`QuadTiling`, `rest_delta = 0`)** — accepted for this quad-only phase; the triangle rule arrives in Phase 9.
- **Decision B was mis-implemented on the first pass** (the adapter inserted the `AnchoredToPanel` screen/world sugar on every member, yielding independent per-tile screen slots — the ruled-out "Option A"); fixed in fix pass 1 to route members through the raw world fold path (raw `AnchoredTo` + `AnchorPose::default()` + `Hinge`). The no-warn test was reworked to assert connectivity (member `TopCenter` world-pos == predecessor `BottomCenter` world-pos) plus the no-`AnchoredTo`/no-diagnostics guarantee.
- **Post-review cleanup (main-agent direct edits):** two `mul_add` lint fixes in the reworked test; and the diegetic adapter's hand-built quad geometry (9 anchor points + 4 edges) was deduplicated into `valence_provider::panel_anchor_geometry`, now the single source both the geometry-fill system and the arrangement adapter call.

**Surprises:**
- The implementer's summary understated the diff on both the implementation and fix pass (claimed a couple of small files; hid the ~1640-line Phase 7 deletion, the cross-crate `reflect_auto_register` sweep, and the Cargo.toml changes). The blind reviewer's spec+diff-only review is what surfaced the real scope; treat implementer self-summaries as untrusted.
- Codex's fix pass stalled on its interactive lint-tool menu and never completed verification (clippy in particular); the main agent ran full verification and applied the residual lint fix.

**Implications for remaining phases:**
- **The `TilingRule` trait shipped but is NOT a dispatch point.** `arrange.rs` defines `trait TilingRule { next_edge, rest_delta }` and `QuadTiling` implements it, but the actual seat computation is an **inherent** `QuadTiling::placement` (not a trait method), and every driver (`member_placement` L388, `arrangement_angle` L452, transitively `apply_member_placements`/`drive_arrangement_hinges`) references `QuadTiling` **concretely**. A second (triangle) `TilingRule` impl therefore cannot be injected into the `Member`/observer/apply machinery — nothing dispatches through the trait. This directly bears on Phase 9's "prove the tiling-rule split" goal and is the primary open decision for the Phase 9 review below (refactor `arrange.rs` to dispatch through `TilingRule`, vs. hand-wire the triangle example against its own rule).
- **Ordering type is `ArrangementMembers`** (private `entities: Vec<Entity>` field, no public constructor — obtainable only via the `Member` observer), not `AnchoredHere`. An example in a separate crate cannot hand-build one.
- Box net is a **tree** (each tile anchors to its single parent edge, using top/bottom AND left/right edges), not a linear chain; `QuadTiling::next_edge` returns a fixed `(TOP, BOTTOM)` pair and `ArrangementMembers::predecessor` yields a single linear predecessor, so the shipped arrangement cannot express a cross-net tree — `box` must hand-wire its `AnchoredTo`+`Hinge` relations directly.

#### Phase 8 Review

Reviewed against Phase 7 as-built **before implementation** (this block shaped the Work Order). Changes folded into this Work Order:
- Added the **workspace auto-register split** constraint: new non-generic arrangement types (`Accordion`/`Strip`/`Member`/`FoldPattern`) derive `Reflect` only — no explicit `register_type` (Phase 7 made auto-register workspace-global; `panel/` now has zero `register_type` calls). Generics + `register_type_data` foreign patches remain explicit; type aliases can hide genericness.
- Added **derive guidance** (`#[reflect(Component)]` for the driver-input types; the relationship type-only-registration split does NOT apply — do not copy it).
- Added the **composition decision (user-chosen: arrangements drive diegetic panels, world AND screen)**. Architect flagged that a raw arrangement `AnchoredTo` on a screen panel would resurrect Phase 7's per-frame warn-spam (`on_panel_space_changed` only strips the tag when `PanelAttachmentAuthored` is present). Resolution: arrangements produce placement as **data** (not a hard-inserted relation); a **diegetic-side adapter** routes panels through the Phase 7 sugar path (`PanelAttachmentAuthored` + world-only `AnchoredTo`), preserving the split so screen panels never warn. `Member` references its arrangement by `Entity` (concrete). Generic `Vec3` offset confirmed already first-class in valence (`with_offset`); only unit-aware lowering is diegetic.
- **Fold/screen composition — RESOLVED (decision "B", user).** Arrangements produce **connected** folds only (tiles joined at shared edges, one shared frame). A screen arrangement screen-attaches **only its root**; the members fold off it via the existing world fold path (`Hinge`→`hinge_to_pose`→`AnchorPose`+transform propagation), so the screen placer needs no per-member 3D rotation and its in-plane-only projection is unchanged. Independent per-tile screen tilt ("Option A") is **not** an arrangement — it is individual panels each with their own screen attachment + a per-panel rotation, already expressible without arrangement machinery; **out of Phase 8 scope**. The `placement(i)`/`Hinge.angle` surface stays space-agnostic; the adapter's root-vs-member split is the only screen-aware decision.
- Added a **diegetic-side adapter** file to Phase 8's Files list (was single-crate; the panel bridge lives in `bevy_diegetic`, not the valence crate).
- Added the `PanelSpace` **query-preference** constraint for the example re-expression.
- Corrected the stale `panel/mod.rs:166` line-ref to `HeadlessLayoutPlugin::build` (set names verified accurate).
- Noted for consideration (not applied): Phase 8's example re-expression overlaps Phase 7's already-migrated fold systems, so the phase is a candidate for a crate-core / example-port split — left as the user's scope call, not decided here.

#### Phase 8 Review — post-implementation (remaining-phase re-evaluation)

Ran after Phase 8 shipped; architect re-read the as-built `arrange.rs` + adapter and re-scored the remaining phase (9). Outcomes folded into Phase 9's Work Order above:
- **User decision (B): Phase 9 makes `TilingRule` a real dispatch point.** The trait shipped un-dispatched (seat math is inherent `QuadTiling::placement`; drivers name `QuadTiling` concretely), so a triangle rule could not be injected — the phase's "prove the tiling-rule split" goal was unreachable as written. Chosen over hand-wiring the triangle example (A, leaves the trait vestigial) and dropping the trait (C). Phase 9 Spec, Files (`arrange.rs` now required, not optional), the as-built tiling-contract constraint, and the acceptance gate all updated to require: promote `placement` + edge→anchor mapping onto the trait, thread dispatch through the drivers, inject `TriangleTiling`, keep the quad path regression-clean.
- **Geometry duplication resolved (user-directed, done):** the quad anchor-geometry builder was duplicated in the diegetic adapter and `valence_provider`; consolidated into a single `pub(super) valence_provider::panel_anchor_geometry` used by both. All gates green (1122 tests, no-default gate, fmt, clippy).
- Confirmed still accurate (no edit): `box` stays hand-wired (tree topology the linear-chain arrangement can't express); `ArrangementMembers` has no public constructor so examples must spawn `Member` entities; example tests can't live in `examples/*.rs` under nextest.

---

### Phase 9 — triangle provider + box net demo + README  · status: done (uncommitted)

#### Work Order

**Goal:** A second shape proves the tiling-rule split, a box net folds shut, and the crate is documented for external use.

**Spec:**

- **Make `TilingRule` a real dispatch point (core refactor, decided in the Phase 8 Review below — decision "B").** Phase 8 shipped the trait un-dispatched: seat computation is an inherent `QuadTiling::placement` and every driver names `QuadTiling` concretely, so a second rule cannot be injected. Phase 9 fixes the interface (the plan's standing instruction: *if the alternation cannot express through the interface, fix the interface, do not special-case triangles*). In `crates/hana_valence/src/arrange.rs`: move the seat computation onto the trait (`TilingRule::placement`, absorbing the quad-only `edge_midpoint` edge→anchor mapping as a trait method so each rule supplies its own), and make the arrangement carry its rule so the drivers (`apply_member_placements`, `member_placement`, `arrangement_angle`, `drive_arrangement_hinges`) dispatch through `&dyn TilingRule` (or a stored rule enum) instead of the hardcoded `QuadTiling`. The quad path must route through the new dispatch unchanged (regression-guarded). This is a change to shipped Phase 8 core, not example-only work.
- **Triangle provider** as example code (per the recipe/crate boundary: procedural shape providers are separate crates or example code — not core): `examples/triangle_accordion.rs` fills `ResolvedAnchorGeometry` for equilateral triangles (3 vertices, 3 edge-mids, centroid, 3 edges) and supplies a **second `TilingRule` impl** (`TriangleTiling`) — shared edge + up/down flip alternate each step, the flip expressed as `rest_delta(i)`'s scalar rest angle about the shared edge (author triangle local geometry so the flat rest is reachable that way). It drives a real `Member`/arrangement through the same observer/apply/drive machinery as quads, injecting `TriangleTiling` via the refactored dispatch above. This is the proof the tiling-rule split is real: if the alternation cannot express through `next_edge(i)` / scalar `rest_delta(i)` / the rule's edge→anchor mapping, the trait is wrong — fix the trait, do not special-case triangles.
- **Box net demo** `examples/box.rs`: 6 quads in a cross net — a **tree** of edge-shared tiles (one `Hinge` per tile at its single `AnchoredTo` parent edge; tree topology is the committed model — the net of any convex polyhedron is a simply-connected planar tree), each with target dihedral 90°. **`box` does NOT use `Member`/`ArrangementMembers`/`Accordion`** — it inserts `AnchoredTo` + `Hinge` + `AnchorPose::default()` directly per tile, choosing each child's shared edge by hand (the cross net branches and uses top/bottom AND left/right edges, which the shipped linear-chain `QuadTiling` arrangement does not model). drive every hinge to its target (manual system or `tween` feature — if tween, copy the `staggered_unfold.rs` wiring: `DefaultTweenPlugins::<()>::in_schedule(PostUpdate)`, `configure_sets` relocating `TweenSystemSet::ApplyTween` into `AnchorSystems::AnimatePose`, `component_tween_system::<HingeAngleLens>()` via `add_tween_systems`, `hinge_to_pose` after `ApplyTween`; the relocation is app-global — fine in a standalone example — and the example needs its own `[[example]] required-features = ["tween"]` block in `crates/hana_valence/Cargo.toml`, same pattern as `staggered_unfold` — `tween` is a **default** feature now (opt-out model, see Delegation Context), so the default `cargo build --examples` has tween on and builds fine; the block's job is keeping the `--no-default-features --examples` gate green (without it the feature-gated imports break under `--no-default-features`); `bevy_tween` is already a non-optional dev-dependency) and the box folds shut. Net closure is topology + target angles, not a resolver invariant — build the net to close; assert final face positions within an epsilon that tolerates cumulative float error.
- Optional stretch: tetrahedron (4 triangles) reusing the triangle geometry — include only if the triangle rule lands cleanly.
- **README** (`crates/hana_valence/README.md`), mirroring the bevy_diegetic README shape (quick-start, examples dir, bevy compat): name story — in chemistry, an atom's **valence** is its capacity to bond: the number and arrangement of connection points it offers. This crate gives shapes the same thing — programmable anchor points by which they bond, assemble, and animate as bonds form, break, and reconfigure. One-liner: *hana_valence — shapes expose connection points and bond into animatable assemblies; named for valence, an atom's capacity to bond.* Vocabulary note: the crate is `hana_valence` but types keep the **anchor** noun (`AnchorId`, `AnchoredTo`, `AnchorPose`) — an anchor point is the concrete connection site, valence the capacity those points add up to. Follows the workspace convention of borrowing one precise outside-field term (diegetic — film theory, lagrange — orbital mechanics, liminal — anthropology, valence — chemistry).

Tests: box-net fold closure within epsilon (headless, fixed frame count); triangle strip alternation positions match hand-computed seats.

**Files:**
- `crates/hana_valence/examples/triangle_accordion.rs` — new.
- `crates/hana_valence/examples/box.rs` — new.
- `crates/hana_valence/README.md` — rewrite.
- `crates/hana_valence/src/arrange.rs` — **required (decision B):** promote seat computation to `TilingRule::placement` (absorbing the `edge_midpoint` edge→anchor mapping into the trait), store the rule on the arrangement, and make the driver systems dispatch through the trait instead of hardcoding `QuadTiling`. Quad path routes through the new dispatch unchanged.
- `crates/hana_valence/Cargo.toml` — only if `box` uses the `tween` feature: add its `[[example]] required-features = ["tween"]` block (pattern: the `staggered_unfold` block).

**Constraints from prior phases:** arrangements + quad tiling rule (Phase 8); `Hinge` per-relation semantics (Phase 4); examples use full `bevy` dev-dependencies (Phase 5 precedent); the lib dep-surface gate still applies. Example schedules running `resolve_anchors` must `init_resource::<ResolveDiagnostics>()` (loud failure otherwise, Phase 3). Hinged tiles must spawn with `AnchorPose::default()` (`hinge_to_pose`'s query requires it, Phase 4) and be driven via `Hinge.angle` only — `hinge_to_pose` overwrites the whole `AnchorPose` every frame. The geometry-fill helpers in `resolve.rs` (`quad_geometry`, `spawn_quad`, etc.) are `#[cfg(test)] pub(crate)` and thus **invisible to `examples/`** (separate non-test compile units) — `box.rs`/`triangle_accordion.rs` must hand-fill `ResolvedAnchorGeometry` inline (the `staggered_unfold.rs` example is the precedent). New non-generic types are auto-registered workspace-wide (Phase 7) — but Phase 9 introduces no new `hana_valence` library types, so no `register_type` decisions arise here.

**Constraints from Phase 8 as-built (arrangement API):**
- **Tiling contract:** `pub trait TilingRule { fn next_edge(&self, index: usize) -> (Edge, Edge); fn rest_delta(&self, index: usize) -> f32; }` (re-exported at crate root). `QuadTiling` is the only impl and is `pub`. **As shipped in Phase 8 the trait is NOT a dispatch point** — the actual seat computation is an inherent `QuadTiling::placement` (not a trait method) and the driver systems (`apply_member_placements`, `member_placement`, `arrangement_angle`, `drive_arrangement_hinges`) reference `QuadTiling` **concretely**; `next_edge`'s `Edge`→(`source_anchor`,`target_anchor`) mapping lives in the quad-only inherent `edge_midpoint` helper, not the trait. **Phase 9 (decision B) makes the trait real:** it moves `placement` + the edge→anchor mapping onto `TilingRule` and threads dispatch through the drivers so a second rule can be injected. Implement the triangle rule against that refactored trait — do not hand-wire around the machinery.
- **Quad-only free fn:** `pub fn member_placement(entity: Entity, member: Member, index: MemberIndex, members: &ArrangementMembers, accordion: Option<&Accordion>, strip: Option<&Strip>) -> Option<MemberPlacement>` is re-exported but calls `QuadTiling.placement` — **do not call it for triangles**.
- **Ordering type is `ArrangementMembers`** (private `entities: Vec<Entity>` field, **no public constructor**; obtainable only via the `Member` spawn observer); `predecessor()` yields a single linear predecessor. An example (separate crate) cannot hand-build one — so any example touching arrangements must go through spawning `Member { arrangement }` entities, never a hand-built `ArrangementMembers`.
- **Shipped names:** `Accordion { fold, lean, pattern }`, `FoldPattern::{Accordion, Coil}`, `Strip`, `Member { arrangement: Entity }`, `MemberIndex { index }`, `PendingMemberPlacement`, `ArrangementPlacement`/`MemberPlacement`.
- **Tests cannot live in `examples/*.rs`** and be collected by `cargo nextest run`: example `#[cfg(test)]` modules are not run unless the `[[example]]` target sets `test = true` and the run passes `--examples`. Put the closure/alternation tests in a `crates/hana_valence/tests/*.rs` integration file (or a `src/arrange.rs` `#[cfg(test)]` module), hand-building the net/strip `ResolvedAnchorGeometry` inline (the `resolve.rs` helpers are `#[cfg(test)] pub(crate)` — invisible to both examples and integration tests).

**Acceptance gate:** `cargo build --workspace --all-features --examples` green; `cargo nextest run --all-features` green including closure + alternation tests (which live in `crates/hana_valence/tests/` or a `src` `#[cfg(test)]` module with inline-built geometry — NOT inside the example binaries, which nextest does not collect); the triangle strip is driven through a real `Member`/arrangement dispatching a `TriangleTiling: TilingRule` (not hand-wired), and the existing quad tests still pass unchanged (dispatch refactor is regression-clean); the lib dep-surface gate (`cargo check -p hana_valence --no-default-features`) stays green after the `arrange.rs` refactor; both examples run and visibly fold (manual spot-check); fmt + `clippy` skill clean.

### Retrospective

**What worked:**
- Decision B landed cleanly: `TilingRule` now carries `next_edge` + `edge_anchor` + a default `placement` method, so `QuadTiling` and `TriangleTiling` share the whole seat/hinge pipeline with zero per-shape special-casing. The quad path routes through the new dispatch unchanged — the five original quad tests pass (now wiring `::<QuadTiling>` + a `QuadTiling` rule component).
- Both the triangle example and the integration test drive a real `Member` arrangement injecting `TriangleTiling`; the box net is a hand-wired tree with a manual hinge driver. Blind codex review + main-agent review both APPROVE with zero blocker/minor findings; all gates green (1124 tests pass).

**What deviated from the plan:**
- Dispatch shape: instead of a stored rule *enum*, codex stored the rule as a **component** (`QuadTiling`/`TriangleTiling` are `#[derive(Component)]`) and made the driver systems **generic** — `apply_member_placements::<R: Component + TilingRule>` / `drive_arrangement_hinges::<R>` — fetching `&R` from the arrangement entity and coercing to `&dyn TilingRule` inside `member_placement`. Satisfies "arrangement carries its rule" + "dispatch through `&dyn`".
- `member_placement`'s public signature gained a `rule: &dyn TilingRule` parameter (Phase 8 it was quad-hardcoded).
- `box` uses a manual hinge driver, not `tween` — so **no** `[[example]]` block was added to `crates/hana_valence/Cargo.toml` (spec allowed either; the block is only needed when the example imports tween-gated symbols). `--no-default-features --examples` stays green without it.
- Tetrahedron stretch skipped (optional).
- `crates/hana_valence/Cargo.toml` flipped `default = ["tween"]` (opt-out model, as the Delegation Context specified).
- The bevy_diegetic panel arrangement adapter call sites were updated to `::<QuadTiling>` to match the now-generic systems.

**Surprises:**
- Generic driver systems mean **every consumer must register one instantiation per rule type** (`apply_member_placements::<QuadTiling>`, `…::<TriangleTiling>`, …). This is a public wiring change from Phase 8's non-generic systems; the README quick-start and both examples show the pattern.
- Geometry-builder helpers (`*_geometry`, `edge_midpoint`, `vertex_position`) are duplicated across `triangle_accordion.rs`, `box.rs`, and `tests/arrangements.rs` — unavoidable because the `resolve.rs` fill helpers are `#[cfg(test)] pub(crate)`, invisible to examples AND integration tests alike. The plan already accepted inline fills.
- Latent, out-of-gate: the crate-root docs intra-link `[`HingeAngleLens`]`/`[`AnchorPoseLens`]` (tween-gated types); `cargo doc --no-default-features` would flag them unresolved, but the dep-surface gate only `cargo check`s there, and default features now include tween.

**Implications for remaining phases:**
- None — Phase 9 is the last scheduled phase. The `## Deferred` items (magnetize, ring arrangement, frame-aware hinge axis, cross-space anchoring, debug gizmos, animation adapters, `NetClosure` validator, widgets handoff, tetrahedron) are unaffected by this refactor; if any is later scheduled, note that driver systems are now generic-per-rule and the rule is a component on the arrangement entity.

### Phase 9 Review

- No remaining scheduled phases to re-evaluate — Phase 9 was the final phase. No plan edits required; the `## Deferred` backlog is unaffected by the `TilingRule` dispatch refactor.

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

## Appendix — deduplicating `PanelSpace` against `DiegeticPanel.coordinate_space`

Deferred consideration, noted 2026-07-09 during the Phase 7 fix pass. Not
scheduled; a cleanup to weigh once the migration settles.

**What shipped in Phase 7.** The world/screen conversion path left the
`hana_valence::AnchoredTo` tag stale: converting a world panel to screen kept
its world-only anchor relation, which the generic `resolve_anchors` query then
`tracing::warn!`s every frame as a missing-geometry skip. The fix reconciles
the tag through a native insert observer, but `On<Insert>` does not fire on the
in-place `Mut<DiegeticPanel>` deref that the conversions use to flip
`coordinate_space`. Rather than convert all ~60 `coordinate_space` read sites
to a standalone component (full Design 1), Phase 7 added a mirror-discriminant
component:

```rust
enum PanelSpace { World, Screen }   // mirrors DiegeticPanel.coordinate_space
```

synced at 4 write sites (spawn seed via `On<Add, DiegeticPanel>`, plus the two
conversion apply-points inserting `PanelSpace::{Screen, World}`), with
`On<Insert, PanelSpace>` → `on_panel_space_changed` reconciling the valence tag.
This buys: a queryable/filterable coordinate-space tag, a native observer hook
the field mutation can't provide, and reflect/BRP visibility — at the cost of
one duplicated discriminant kept in sync by hand.

**The duplication.** `PanelSpace` is derived state: it is always
`PanelSpace::from(panel.coordinate_space())`. Two sources of truth for one fact.
The sync is correct only as long as every writer of `coordinate_space` also
inserts the matching `PanelSpace` — today that is the 4 sites, but a future
writer that mutates `coordinate_space` without the insert reintroduces the exact
staleness Phase 7 fixed, silently (no compile error; the observer just never
fires).

**Options to weigh later:**

1. **Keep the mirror (status quo).** Cheapest. Guard the invariant with a debug
   assertion or a test that scans for `coordinate_space` writes lacking a paired
   `PanelSpace` insert. Accept the duplication as the price of the native
   observer.
2. **Promote `CoordinateSpace` to a standalone component** (full Design 1 —
   remove the field from `DiegeticPanel`, ~60 read sites migrate to a
   `Query<&CoordinateSpace>`). Single source of truth; `On<Insert>` fires
   natively on every conversion because the conversions become component
   inserts, not field mutations. Deleted: `PanelSpace`, its `From`, the spawn
   observer, the 4 manual inserts. Cost: the ~60-site sweep, and every system
   reading `coordinate_space` today gains a query param.
3. **Make the conversions insert `DiegeticPanel` wholesale** instead of
   mutating the field, so `On<Insert, DiegeticPanel>` fires and drives the
   reconcile directly — no `PanelSpace` at all. Cost: conversions must
   reconstruct/clone the full `DiegeticPanel` (heavier component) on every
   flip, and any other in-place `DiegeticPanel` mutation elsewhere would now
   also trip the anchor reconcile (broader observer firing surface).

Option 2 is the clean end state if the coordinate-space concept keeps growing
consumers; Option 1 is right if it does not. Decide after Phases 8–9 show how
much more code reads coordinate space.
