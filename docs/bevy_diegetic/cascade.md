# Cascade unification — one parent-walking hierarchy

Extracted from the slug migration plan (`slug_migration.md`, Phase 9) as a
standalone design + implementation doc. This is the final structural change
of that migration: it replaces the cascade module's three fixed-depth
topologies with a single parent-walking resolution.

## Background — the bug that surfaced it

The example smoke check found that `text_alpha` crashed on every alpha-mode
switch. Root cause — the 2-tier standalone cascade decides membership by "does
this entity hold the `Override` component?", and `WorldTextStyle` (the
`WorldTextAlpha` / `WorldFontUnit` override) lives on **both** standalone world
text and panel labels. So panel labels were enrolled as standalone targets,
carried a `Resolved<WorldTextAlpha>` nothing reads (panel labels render from
`Resolved<PanelTextAlpha>`, the 3-tier cascade), and the global-default
propagate loop wrote to them — including the frame a HUD rebuild despawned
them, landing a deferred `insert` on a freed entity → panic.

## The small fix that already landed

A scoped fix shipped ahead of this rewrite (not deferred): added `type Exclude:
Component` to `CascadeTarget`, filtered both write paths
(`on_cascade_target_added`, `propagate_global_default_to_entity`) with
`Without<A::Exclude>`, and set `Exclude = PanelChild` on `WorldTextAlpha` /
`WorldFontUnit` (other impls use the new `ExcludeNone` sentinel). The standalone
cascade now never enrolls panel labels; the crash is gone, verified by cycling
all seven alpha modes with no panic. This rewrite is the **root-structure**
follow-up that small fix points at.

## The deeper issue

The cascade module has **three fixed-depth topologies** (`mod.rs` table):
entity-targeted (entity → global), panel-targeted (panel → global), and
child-of-panel (child → panel → global). The last two are the *same chain*
(`global → panel → child`) cut at different depths and built as separate
traits / plugins. The design enumerates "how many tiers does this attribute
have" instead of propagating along the entity tree. That is why one logical
value (text alpha) exists as two `Resolved` types (`WorldTextAlpha` and
`PanelTextAlpha`) and why a category-error in membership was even possible.

## Goal

Replace the three topologies with one parent-walking resolution. One
`Resolved<A>` per attribute; one rule — *my override, else my parent's
`Resolved<A>`, else the global default at the root* — applied by following
`ChildOf` links. Standalone text is depth-1 off the root, a panel is depth-1, a
panel label is depth-2; a future deeper nesting needs no new type. Membership
becomes "position in the tree," so an entity is never enrolled into the wrong
cascade by an incidental shared component, and the `Exclude` marker introduced
by the small fix is no longer needed.

This work is independent of the rest of the migration sequence and leaves the
build green.

## Checklist

- [ ] Define one uniform "override for attribute `A` at this node" accessor
      across the node-bearing components (`WorldTextStyle`, `DiegeticPanel`,
      panel-child override) so the walk reads every level the same way — the
      main new abstraction this requires.
- [ ] Collapse the per-role attribute types into one per value: `WorldTextAlpha`
      + `PanelTextAlpha` → one `TextAlpha`; `WorldFontUnit` + `PanelFontUnit` +
      panel-child font unit → one `FontUnit`. One `Resolved<TextAlpha>` /
      `Resolved<FontUnit>` per entity, read by both standalone and panel render
      paths (drop the `Without<PanelChild>` / `With<PanelChild>` split
      on the read side). Also repoint the `Changed<Resolved<WorldTextAlpha>>` /
      `Changed<Resolved<WorldFontUnit>>` arms of the `Or<(…)>` change-detection
      filter on the world-text render queries (`world_text/mod.rs` ~36–41,
      `rendering.rs` the `ChangedWorldTextQuery` alias) to the new
      `Resolved<TextAlpha>` / `Resolved<FontUnit>`, or the render systems
      silently stop re-running on alpha/unit changes (a staleness bug, not a
      compile error if the old `Resolved` types linger).
- [ ] Replace `CascadeTarget` (2-tier) and `CascadePanelChild` (3-tier) plus
      their three plugins (`CascadeEntityPlugin`, `CascadePanelPlugin`,
      `CascadePanelChildPlugin`) with one hierarchical cascade plugin that walks
      parent links with a global-default fallback at the root.
- [ ] Remove the `Exclude` associated type and `ExcludeNone` sentinel added by
      the small fix — the tree-position membership makes them unnecessary. This
      also removes the in-module test impls' `type Exclude = ExcludeNone;`
      (`cascade/target.rs:127, 244`), the `use crate::cascade::ExcludeNone;`
      (`target.rs:111`), and the `Exclude` / `ExcludeNone` doc text
      (`resolved.rs` and the `mod.rs` re-export).
- [ ] Re-verify `text_alpha` (cycle all alpha modes), panel text, and standalone
      world text all resolve correctly, and the crash stays fixed.

## Risk

The uniform override accessor and a virtual global root at the top of every
chain are real new abstraction; this is a cascade-module rewrite, larger than
the small fix. Sequence it after the rest of the migration so it lands against a
green, slug-only tree.

---

## Mechanical findings (auto-recorded — team review)

These are deterministic, low-risk plan amendments. Apply during implementation.

- **M1 — Pre-rename collision audit.** Before renaming to `TextAlpha` / `FontUnit`,
  run a crate-wide grep to confirm neither name already exists as an unrelated type.
- **M2 — Complete the change-detection / read-site enumeration.** The plan lists the
  world-text `Or<(…)>` arms; the full set of sites that read or filter on the old
  per-role `Resolved` types is wider. Enumerate and repoint every one so none is
  missed: `render/world_text/mod.rs` (~36–41 `Changed<Resolved<WorldTextAlpha>>` /
  `Changed<Resolved<WorldFontUnit>>`), `render/world_text/rendering.rs` (the
  `ChangedWorldTextQuery` alias), `render/text_renderer/shaping.rs:42`
  (`Changed<Resolved<PanelTextAlpha>>`), `render/text_renderer/batching.rs` (~90–94),
  and `panel/compute_layout.rs:46` (`Resolved<PanelFontUnit>`, a plain `.get()`, not a
  `Changed` filter). Treat this as a checklist line, not prose.
- **M3 — Bounded walk.** Whatever the resolution mechanism, guard the parent traversal
  against a corrupt tree (self-parent / cycle) with a depth cap or visited check so a
  bad `ChildOf` cannot hang a system.
- **M4 — Keep pure-function resolvers.** The current `resolve_target` / `resolve_panel_child`
  pure functions (`cascade/resolved.rs`) are unit-testable in isolation. Preserve an
  equivalent pure `resolve_attr` as the single source of truth; the plugin is only
  plumbing that calls it. Do not embed walk logic into attribute impls or plugins.
- **M5 — Document cached + top-down (from P2, confirmed already-implemented).** State in
  the plan that resolution is cached per-entity in `Resolved<A>` and propagated top-down
  (parents before children) within `CascadeSet::Propagate`, not a lazy read-time upward
  walk. Cycle 2 confirmed the current code already does this — documentation only, no
  behavior change.
- **M6 — Global fallback wording (from P3, confirmed already-implemented).** Rename
  "virtual global root" to "global fallback"; document that a parentless node — or a
  dangling `ChildOf` after the parent despawns (Bevy does not clear it) — terminates the
  walk at `CascadeDefaults` via `.ok()` (today's behavior), never a panic.
- **M7 — Verify only the distinct failure paths (from P7, pruned by the Risk lens).**
  Expand the verify step to: (1) cross-enrollment regression — a `WorldTextStyle` on both
  a standalone entity and a panel label must not cross-resolve (assert the label has no
  standalone `Resolved`); (2) same-command-batch panel+child spawn resolves in one frame;
  (3) reparenting a child re-resolves against the new parent. Do **not** add separate
  despawn/respawn-churn or multi-window cases — `text_alpha.rs` cycling all modes already
  exercises despawn/respawn, and multi-window adds no distinct failure path.
- **M8 — Mesh-rebuild trigger gap (Correctness — a live bug, not just a rewrite concern).**
  `build_panel_slug_meshes` (`render/text_renderer/batching.rs`) wakes on
  `Changed<PanelText>`, not on the resolved-alpha/unit change, and reads the
  resolved alpha defensively with a global-default fallback — so an alpha-only change (text
  run unchanged) renders one frame stale and the fallback masks a missing `Resolved`. All
  three cycle-3 agents confirmed this exists in the **current** code, independent of the
  rewrite. Worth a small standalone fix (add `Changed<Resolved<PanelTextAlpha>>` to the
  `build_panel_slug_meshes` filter) regardless of the D1 outcome.
- **M9 — `world_scale` bypass (from D2, contingent).** Only relevant if a font-unit
  collapse actually happens. Cycle 3 confirmed the renderer already applies
  `WorldTextStyle::world_scale` as a post-cascade bypass (`rendering.rs`) and
  `Resolved<WorldFontUnit>` already encodes the `Unit` tier only — already-implemented. If
  the collapse proceeds, restate this in the new `FontUnit` docs so the bypass is not
  dropped. No code change otherwise.
- **M10 — Compile-time guard against future cross-enrollment (NEW, Risk, advisory).** The
  `Exclude` marker that prevents the original crash is an easy-to-forget trait-impl detail:
  a future `CascadeTarget` / `CascadePanelChild` impl on a component shared by two cascades
  (e.g. a new text attribute on `WorldTextStyle`) that omits `Exclude` silently re-opens the
  crash with no compile error. Consider a deny-by-default lint or a macro/const assertion in
  the cascade module that rejects a shared-component cascade without an explicit `Exclude`.
  Independent of D1 — hardens the design that already exists today.

## Proposed user decisions (team review — pending after final cycle)

Surfaced by the cycle-1 team, reconciled in cycle 2. Only `proposed` entries are
surfaced at the end; `superseded` / `dropped` are kept with reasons so a later pass does
not relitigate.

- **D1 — Design + scope: the as-written plan is unsafe and oversold; choose the real
  target (status: proposed, CRITICAL, all four lenses, cycle-2 consensus).** Cycle 2 found
  that adopting the cycle-1 recommendations (keep cached/top-down propagation, keep a typed
  membership guard, keep per-role `Resolved` markers) leaves most of the existing machinery
  intact — so the headline "replace three topologies with one parent-walk" overstates the
  change, and three as-written checklist items are actively unsafe:
  - item 4 ("remove `Exclude`") → **retract.** `Exclude = PanelChild` is the
    compile-time proof that the standalone cascade never enrolls a panel label — the exact
    category error behind the original crash. Tree position is an unenforced runtime
    convention; a future intermediate node carrying an override re-opens the bug.
  - item 2 ("drop the `With/Without<PanelChild>` read split", "one `Resolved<TextAlpha>`")
    → **unsafe as written.** Unifying the `Resolved` marker *and* dropping the split makes a
    panel-alpha change wake the standalone render query and vice-versa (cross-firing). Keep
    per-role `Resolved` markers; unify only the wrapped *value* type.
  - the "uniform override accessor" → a per-attribute dispatch fn (optional
    `from_world_text_style` / `from_panel` / `from_panel_child`), not one trait three
    unrelated types implement.

  The genuine fork for the user:
  - **(a) Minimal:** keep the three cascade traits, cached/top-down propagation, and the
    `Exclude` guard as-is; unify only the value types (`WorldTextAlpha`+`PanelTextAlpha`→
    `TextAlpha`; the three font-unit types→`FontUnit`) with per-role `Resolved` markers;
    keep the read split. Smallest, lowest-risk; the crash is already fixed, so this is a
    duplication cleanup, not a bug fix.
  - **(b) Sealed-membership redesign:** replace `Exclude` with a sealed `CascadeMember` /
    role trait + per-role `Resolved<Role>` generic + dispatch accessor (the type lens's
    design). Cleaner additive path for a future 3rd/4th attribute; more new code and more
    risk in a module that already crashed once.
  - **(c) Defer:** the small fix already shipped and the build is green; do neither now.

  *Recommendation (revised in cycle 3): **(c) Defer.*** Cycle 3 established that the
  value-unification is near-cosmetic: `WorldTextAlpha` / `PanelTextAlpha` are already thin
  newtypes over `AlphaMode` (and the font-unit types over `Unit`), so the only shared value
  is already the inner enum. The literal option (a) — delete both newtypes for one
  `TextAlpha` and one `Resolved<TextAlpha>` — is **unsafe**: a single `Resolved<TextAlpha>`
  makes `Changed<Resolved<TextAlpha>>` fire for both cascades, so a panel-alpha change wakes
  the standalone render query (cross-firing); the read split filters the wasted work but
  does not stop the spurious wake. The only safe unification — option **(a′)** — keeps
  `WorldTextAlpha(TextAlpha)` / `PanelTextAlpha(TextAlpha)` as thin per-role marker newtypes
  wrapping a shared `TextAlpha`, which removes ~40–80 lines of trivial boilerplate but
  *adds* a wrapping layer: a marginal net change in a module that just crashed and
  recovered. Since the crash is already fixed by the `Exclude` guard and the build is green,
  the cycle-3 consensus is **defer** — do neither now, revisit (a′) or (b) when a concrete
  third cascaded attribute justifies the work. If the user wants to proceed regardless,
  prefer (a′) over the literal (a) and pair M2 with M8. Supersedes P1, P4, P5 and the
  plan's Goal/checklist framing.
- **D2 — `world_scale` in the `FontUnit` collapse (status: proposed, minor, Correctness).**
  `WorldTextStyle::world_scale` short-circuits the font-unit cascade (raw meters-per-unit
  bypass of the `Unit` abstraction); the renderer applies it after the resolved unit. Both
  later cycles recommend the same answer: keep it a **post-cascade bypass** —
  `Resolved<FontUnit>` encodes the `Unit` tier only, the renderer keeps applying
  `world_scale` after — and document it so the collapse does not silently drop the bypass.
  *Recommendation:* adopt as stated (post-cascade bypass, documented). Renamed from P6.

### Reconciled / closed in cycle 2
- **P1, P4, P5 → merged into D1** — facets of one design choice.
- **P2 → superseded → mechanical M5.** Confirmed the code already caches per-entity +
  propagates top-down; no user judgment, just document.
- **P3 → superseded → mechanical M6.** Confirmed already-implemented; rename + document.
- **P6 → renamed D2.**
- **P7 → superseded → mechanical M7**, pruned by the Risk lens (despawn churn already
  covered by `text_alpha.rs`; multi-window adds no distinct failure path).

### Reconciled / closed in cycle 3
- **D1 recommendation flipped (a) → (c) Defer.** Value-unification is near-cosmetic; the
  literal (a) cross-fires; (a′) is safe but marginal; the crash is already fixed.
- **D2 → superseded → mechanical M9** — contingent on a collapse happening; already
  implemented as a post-cascade bypass otherwise.
- **M8 upgraded** — confirmed a live current bug, fixable independent of D1.
- **M10 added** — advisory compile-time `Exclude` guardrail for the design that exists today.

Surviving decision after the final cycle: **D1**.

**D1 outcome — chosen: (b) full unified redesign (2026-05-25).** The "defer" recommendation
was rejected: it rested on "the crash is already fixed," which is irrelevant — Phase 9
targets the structural defect (one logical value living as two `Resolved` types because the
module enumerates tiers instead of propagating along the tree), not the crash. The review
also mis-analyzed membership: it reasoned about removing `Exclude` *while the three cascades
persist* (unsafe), but the chosen design collapses to **one** cascade, where there is no
second cascade to mis-enroll into and `Exclude` is structurally unnecessary. The legitimate
refinements from the review are folded in below (keep the entity-selection read filters;
per-attribute dispatch accessor; M2/M3/M6/M8/M9). See **Chosen design** below.

---

## Chosen design — unified parent-walking cascade (D1 = b)

One cascade. One `Resolved<A>` per logical attribute. Resolution rule, applied by following
`ChildOf`: *my own override, else my parent's `Resolved<A>`, else the global default at the
root.* This is the original Goal, made concrete and hardened against the review's valid
points.

### Attribute value types — the marker *is* the value

Today `WorldTextAlpha(AlphaMode)` and `PanelTextAlpha(AlphaMode)` are two newtypes over the
same `AlphaMode`; the font-unit trio are three newtypes over `Unit`. Collapse each logical
value to one type that is itself the cascade attribute:

```rust
#[derive(Component, Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct TextAlpha(pub AlphaMode);

#[derive(Component, Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct FontUnit(pub Unit);

/// Cached resolution: exactly one per attribute per entity.
#[derive(Component, Clone, Copy, Reflect)]
pub(crate) struct Resolved<A: CascadeAttr>(pub A);
```

There are no per-role marker newtypes (`Resolved<WorldTextAlpha>` / `Resolved<PanelTextAlpha>`
both disappear). This is the duplication the rewrite removes — and it is removed for real,
not re-introduced under another name.

### The override accessor (enumerated sources + exhaustive match — D4 + D6)

The override-bearing node components are enumerated once. Precedence is variant declaration
order. Each attribute supplies a `read` that is an *exhaustive* `match` over the enum — so
adding a source variant stops every attribute's `match` from compiling until it handles the new
source. A forgotten source is a compile error, not a silently-skipped if-let arm; that
exhaustiveness is also the cross-enrollment guardrail (no separate `Exclude`, const-assert, or
sealed trait — see D6).

```rust
#[derive(Clone, Copy, EnumIter)]   // `iter()` is generated from the variants, in order
pub(crate) enum OverrideSource {
    WorldTextStyle,                 // precedence = declaration order
    DiegeticPanel,
    PanelText,
}

pub(crate) trait CascadeAttr: Component + Copy + PartialEq + Reflect + 'static {
    /// This node's override for `A` from one specific source. `None` if that
    /// source is absent or sets no override for `A`.
    fn read(source: OverrideSource, node: EntityRef<'_>) -> Option<Self>;
    /// Root fallback.
    fn global_default(defaults: &CascadeDefaults) -> Self;

    /// The node's own override, first source that answers. Provided — attributes
    /// supply only `read`. Cannot drift from the source set: the walk is driven by
    /// `EnumIter`, the handling by the exhaustive `match` in `read`.
    fn override_at(node: EntityRef<'_>) -> Option<Self> {
        OverrideSource::iter().find_map(|s| Self::read(s, node))
    }
}

impl CascadeAttr for TextAlpha {
    fn read(source: OverrideSource, node: EntityRef<'_>) -> Option<Self> {
        match source {                                  // no `_` arm — see discipline note
            OverrideSource::WorldTextStyle =>
                node.get::<WorldTextStyle>()?.alpha_mode().map(TextAlpha),
            OverrideSource::DiegeticPanel =>
                node.get::<DiegeticPanel>()?.text_alpha_mode().map(TextAlpha),
            OverrideSource::PanelText =>
                node.get::<PanelText>()?.alpha_mode.map(TextAlpha),
        }
    }
    fn global_default(d: &CascadeDefaults) -> Self { TextAlpha(d.text_alpha) }
}
```

A future third attribute is a pure additive `impl CascadeAttr` — one `read` match, no new
plugin, trait, or topology. A new override-bearing component is one new `OverrideSource` variant,
which the compiler then forces every attribute to handle. **Discipline:** no `_ =>` wildcard arm
in any `read` (a wildcard would silently absorb a new variant); enforce by lint or review.

**Open follow-on (B), not solved here.** The enum guarantees every source *type* is handled; it
does not stop a single *entity* from carrying two sources. A panel label carries both
`WorldTextStyle` and `PanelText`, so `override_at` returns `WorldTextStyle` first — safe today
only because a label's `WorldTextStyle` leaves the attribute unset, an unchecked runtime
convention. Making that unrepresentable means changing what components a label is spawned with
(one cascade source per node), not the dispatch.

### Membership — `Exclude` is structurally gone, not removed-by-convention

With one cascade, every text/panel node resolves by the same rule, so there is no "standalone
cascade" vs "panel cascade" for a shared component (`WorldTextStyle`) to be mis-enrolled
into. The original crash — a panel label enrolled as a standalone target — *cannot be
expressed*: the label resolves `Resolved<TextAlpha>` from its own override → its panel parent
→ root, which is exactly what it should do. `Exclude` / `ExcludeNone` delete cleanly (review
M10's "forgot-to-add-Exclude" hazard also evaporates: there is no second cascade to forget to
exclude from). This is the structural win the crash *pointed at* but did not, by itself,
require.

### Resolution: cached, top-down, one pass (review P2/M5)

A single system resolves in hierarchy order (roots first), inserting `Resolved<A>` only on
change so `Changed<Resolved<A>>` fires for downstream readers:

```
for node in hierarchy_order(roots_first):          // parent resolved before child
    let resolved = A::override_at(node)            // provided default: walks OverrideSource::iter()
        .or_else(|| parent_of(node).and_then(get::<Resolved<A>>).map(|r| r.0))
        .unwrap_or_else(|| A::global_default(defaults));
    if get::<Resolved<A>>(node).map(|r| r.0) != Some(resolved) { insert(Resolved(resolved)); }  // M12: compare inner value
```

Resolving roots-first in one pass means a child reads its parent's value *resolved this same
pass* — no multi-frame lag. Runs in `CascadeSet::Propagate`. A parentless node, or a dangling
`ChildOf` after a parent despawns, terminates at the global default via `.ok()` (review M6),
never a panic; the walk carries a depth cap against a corrupt tree (review M3).

### Read side — keep entity selection, unify the cascade (corrects the original "drop the split")

The original checklist said "drop the `With/Without<PanelChild>` split on the read side."
That was imprecise, and the review correctly flagged it — but the fix is *not* per-role
`Resolved` markers. Those `With`/`Without<PanelChild>` filters select **which entities a
render system draws** (standalone world text vs panel text are drawn by different systems);
they are orthogonal to the cascade. Keep them. What unifies is the value each system reads:
both now read `Resolved<TextAlpha>` / `Resolved<FontUnit>`.

The review's "cross-firing" objection reduces to a cheap spurious wake: a panel-alpha change
bumps `Changed<Resolved<TextAlpha>>`, waking `render_world_text`, which then matches zero
in-scope changed entities (its `Without<PanelChild>` filter) and does nothing. That is a
filtered no-op, not a correctness bug. Acceptable; revisit only if it ever shows on a profile.

### `world_scale` (review M9/D2)

`WorldTextStyle::world_scale` stays a **post-cascade bypass**: `Resolved<FontUnit>` encodes
the `Unit` tier only, the renderer keeps applying `world_scale` after. Restate this in the
`FontUnit` doc comment so the collapse does not drop it. (Already how the renderer behaves.)

### Revised checklist

- [ ] Define `TextAlpha(AlphaMode)` / `FontUnit(Unit)` as the cascade attributes and the
      generic `Resolved<A: CascadeAttr>`; delete `WorldTextAlpha`, `PanelTextAlpha`,
      `WorldFontUnit`, `PanelFontUnit`, and the panel-child font-unit type.
- [ ] Define the `OverrideSource` enum (`#[derive(EnumIter)]`) and implement `CascadeAttr::read`
      (exhaustive `match`, no `_` arm) per attribute; `override_at` is the provided default walk.
- [ ] Replace `CascadeTarget` (2-tier), `CascadePanelChild` (3-tier), and the three plugins
      with one hierarchical cascade plugin that runs the roots-first resolution pass in
      `CascadeSet::Propagate`.
- [ ] Delete `Exclude` / `ExcludeNone` and the test impls (`cascade/target.rs:127,244`,
      `target.rs:111`, the `resolved.rs` / `mod.rs` doc + re-export) — now structurally
      unnecessary under one cascade.
- [ ] Repoint every read/filter site to `Resolved<TextAlpha>` / `Resolved<FontUnit>` (review
      M2: `world_text/mod.rs` ~36–41, `rendering.rs` `ChangedWorldTextQuery`,
      `text_renderer/shaping.rs:42`, `batching.rs` ~90–94, `panel/compute_layout.rs:46`).
      **Keep** the `With`/`Without<PanelChild>` entity-selection filters.
- [ ] Add the resolved-attribute arm to `build_panel_slug_meshes` so an alpha-only change
      rebuilds the mesh (review M8 — also a live bug today).
- [ ] Verify (review M7): cross-enrollment (shared `WorldTextStyle` on a standalone entity and
      a panel label resolve independently — now by construction); same-batch panel+child spawn
      resolves in one frame; reparenting re-resolves against the new parent; plus the existing
      alpha-mode cycle in `text_alpha.rs`.

### Standalone pre-work (independent of the rewrite)

- [x] **M8 mesh-trigger bug** — fixed ahead of the rewrite (2026-05-25):
      `build_panel_slug_meshes` now wakes on
      `Or<(Changed<PanelText>, Changed<Resolved<PanelTextAlpha>>)>`
      (`batching.rs:90–94`). The rewrite only repoints that arm to
      `Changed<Resolved<TextAlpha>>`.

---

## Team review round 2 (2026-05-25) — strengthen posture

Second review, bound to the chosen design's intent (one parent-walking cascade);
premise-challenges quarantined. Four lenses (Correctness, Architecture, Risk,
Type-system), two cycles. No admissible premise-challenge surfaced — every finding
strengthens the committed design.

### Mechanical findings (auto-recorded — round 2)

- **M11 — `Reflect` bound on `CascadeAttr`.** `Resolved<A>` derives `Reflect`, so its
  type parameter must be `Reflect`. Add `Reflect` to the `CascadeAttr` bounds
  (`Component + Copy + PartialEq + Reflect + 'static`) and confirm `TextAlpha` /
  `FontUnit` carry `#[derive(Reflect)]`, or `Resolved<A>` will not compile / not register.
- **M12 — Resolution pseudocode precision.** The change-gate compares the inner value, not
  the wrapper: write `if get::<Resolved<A>>(node).map(|r| r.0) != Some(resolved) { … }` (or
  `old.0 != resolved`), matching today's `target.rs:98` / `panel_child.rs:115` pattern.
  Doc-only; no behavior change.
- **M13 — Per-attribute reflect registration.** Each new `impl CascadeAttr` needs a paired
  `app.register_type::<Resolved<A>>()` in the plugin `build` (as `target.rs:59` does today),
  or reflection silently drops the type. Add as a checklist reminder.
- **M14 — Extend the M2 repoint sweep to reflection/serialization sites.** The crate is
  demo-only (no saved scenes expected), but the rename `Resolved<WorldTextAlpha>` →
  `Resolved<TextAlpha>` must also be checked against any `.ron`/scene/asset data and
  `register_type` calls, not only `Changed`/read filters. Verify none exist; repoint any found.
- **M15 — Bounded-walk concretization (refines M3).** Implement the parent walk as an
  iterative loop with an explicit depth cap that terminates at the global default on exceed,
  plus a debug-only visited check for cycles; add a self-parent / 2-cycle test that asserts
  graceful termination (no hang, no panic).
- **M16 — Doc clarity (two phrasings).** (a) State cascade-unification (one `Resolved<A>` per
  entity) and read-path selection (`With`/`Without<PanelChild>`) as *orthogonal*: the
  cascade is blind to the render filters. (b) Note the read side is not zero-cost-additive —
  each new cascaded attribute adds a `Changed<Resolved<A>>` arm to every reader `Or<(…)>` it
  affects. Both are documentation, not behavior.

The spurious cross-wake (a panel-alpha change waking `render_world_text` to a zero-entity
no-op) is already documented in the Chosen design as acceptable; consensus across cycle-1
lenses confirms that judgment — no new action.

### Proposed user decisions (round 2 — pending after final cycle)

- **D3 — Resolution-pass execution model (status: proposed, CRITICAL, Architecture + Risk +
  Correctness consensus).** The Chosen design states "one pass, roots-first, resolved this
  same pass," but Bevy does not spawn entities in `ChildOf`-tree order and `ChildOf` can be
  mutated mid-frame. Three concrete windows: (1) a same-command-batch panel+child spawn where
  the child is processed before the parent's `Resolved<A>` exists → child falls to global
  default for a frame; (2) reparenting — when/how the child re-resolves against the new
  parent; (3) the original-crash class — a deferred `insert(Resolved)` landing on an entity
  freed by a HUD rebuild in the same frame. The current 3-tier code dodges (1) with spawn-time
  observers reading the parent's *raw* override. Decision: how does the unified pass run? Candidates:
  - **(i)** keep spawn-time observers for initial resolution + a roots-first pass only for
    downstream propagation (two write paths, proven-race-free spawn);
  - **(ii)** one depth-sorted pass that runs after command flush each frame (single path; relies
    on sorting parents-before-children and on the despawn check);
  - **(iii)** accept a one-frame lag at spawn/reparent (simplest; the pass re-runs every frame).
  *Highest-stakes item — the original crash was exactly a deferred insert on a freed entity.*
- **D4 — Override-accessor signature / pure-resolver boundary (status: proposed, important,
  Architecture + Correctness + Type-system).** `override_at(node: EntityRef<'_>)` does component
  I/O inside the trait impl, which (a) is not the pure resolver M4 asks for — it can't be
  unit-tested without a `World`, and (b) ties resolution to how `EntityRef` is obtained/passed
  from the plugin loop. Alternative: a pure `fn resolve_attr(world_override, panel_override,
  panel_child_override: Option<Self>) -> Option<Self>` that the plugin feeds pre-projected
  component values, keeping the if-let component probing in the plumbing. Decision: keep
  `override_at(EntityRef)` (ergonomic, one method) or split into pure-resolver + plugin
  projection (testable, M4-compliant)?
- **D5 — Panel-child font-unit override (status: proposed, important, Correctness).** The
  `TextAlpha` accessor reads `PanelText.alpha_mode` (field exists). The `FontUnit`
  accessor has no analogous source: `PanelText` carries no font-unit field. So the
  `FontUnit` collapse can't follow the same 3-tier pattern as alpha. Decision: add
  `font_unit: Option<Unit>` to `PanelText` (per-run override, true 3-tier parity), or
  declare `FontUnit` 2-tier for panel children (no child override; resolves panel → root)?
  This is a product/semantics call, not mechanical.
- **D6 — Cross-enrollment guardrail after `Exclude` removal (status: proposed, important, Risk
  + Architecture + Type-system).** Removing `Exclude` is safe *today* because there is exactly
  one cascade. But the `override_at` dispatch is an if-let ladder with no compile-time
  exhaustiveness, and a future *second* cascade on a component shared by two roles re-opens the
  original cross-enrollment crash with no compiler error (the M10 hazard, now concrete under the
  unified design). Decision: land the rewrite with (i) a compile-time guard (macro / const
  assertion / deny-lint) that rejects a shared-component cascade or a missing dispatch arm, (ii)
  a documented convention only, or (iii) nothing now (revisit if a 2nd cascade lands)?

### Cycle 2 reconciliation

Cycle 2 grounded every decision in the current code and converged. No premise-challenge in either
cycle. Recommendations below are the cross-lens consensus; the final call is the user's.

- **D3 → recommend (i): spawn-time observers + roots-first propagation pass.** Unanimous across all
  four lenses. The current 3-tier `on_panel_child_added` (`panel_child.rs:76–90`) already proves
  this race-free: the observer reads the **parent's raw override component** (not its `Resolved<A>`),
  so a same-command-batch panel+child spawn resolves correctly even before the parent's queued
  `insert(Resolved)` flushes. It also closes the original-crash window — the observer inserts
  synchronously during command flush rather than as a deferred command that could land on an entity a
  HUD rebuild freed the same frame. Two write paths (observer = initial; pass = downstream
  propagation), both calling the same pure resolver. Options (ii)/(iii) reintroduce a spawn-order lag
  or an unmitigated despawn race. **Strong recommendation; near-settled.**
- **D4 → split 2–1 toward keeping `override_at(EntityRef)`.** Architecture and Type-system lenses:
  keep the single `override_at(EntityRef)` method (ergonomic, fine for 3 fixed sites) and add a doc
  comment on each impl enumerating its override sources so a missed arm shows on review; reframe M4
  from "pure function" to "deterministic resolution logic." Correctness lens dissents: extract a pure
  `resolve_attr(Option<Self>×3)` + plugin projection to preserve unit-testability without a `World`.
  Genuine fork — pick ergonomics (keep `override_at`) or isolated-unit-testability (pure split).
- **D5 → genuine product fork, confirmed concrete.** `PanelText` has `alpha_mode` but **no**
  font-unit field (`batching.rs:37–50`), so `FontUnit` cannot mirror `TextAlpha`'s 3-tier read. Either
  add `font_unit: Option<Unit>` to `PanelText` (true 3-tier parity) or declare `FontUnit`
  2-tier for panel children (panel → root; matches today's behavior — no per-run override exists now).
  Type-safe either way; this is a feature decision about whether per-run font-unit overrides exist.
- **D6 → recommend doc convention now, defer the compile-time guard.** Convergence across all four
  lenses: a Rust compile-time exhaustiveness check for the if-let dispatch needs a sealed per-site
  trait or a phantom-trait const-assertion, which couples the attribute to concrete component types —
  friction not justified for 3 sites and a single cascade. Document the rule (every `impl CascadeAttr`
  enumerates all override sources it reads) and revisit a guard if/when a second cascaded attribute on
  a shared component lands. Optional middle path: a const-assert safety net in the plugin `build()` —
  surfaced for the user, not recommended now.

### Mechanical findings (auto-recorded — cycle 2)

- **M17 — Observer→Propagate ordering note (contingent on D3 = (i)).** If spawn-time observers do
  initial resolution, document the frame-order invariant in `mod.rs` / `CascadeSet`: observers fire
  during command flush (before systems), so a same-batch spawn carries `Resolved<A>` before
  `CascadeSet::Propagate` runs. Add a docstring so a future reorder cannot silently break it.
- **M15 sharpened** — implement the walk as an iterative `for _ in 0..MAX_DEPTH` loop terminating at
  the global default on cap, with a debug-only `HashSet<Entity>` visited check; test a `ChildOf(self)`
  self-parent and a 2-cycle. (`MAX_DEPTH` ~ 32–256; mirror `bevy_hierarchy`'s own cap.)
- **M14 sharpened** — concrete sweep: `rg "Resolved<(World|Panel)(TextAlpha|FontUnit)>" crates/` to
  catch lingering old type references, and confirm every new `impl CascadeAttr` has a paired
  `app.register_type::<Resolved<A>>()`.
- **M16 absorbs cycle-2 "unification scope"** — also state explicitly that there are no per-role
  `Resolved` wrappers: exactly one `Resolved<TextAlpha>` / `Resolved<FontUnit>` per entity, and the
  spurious cross-wake is absorbed by the `With`/`Without<PanelChild>` render filters.

### User decisions (2026-05-25)

- **D3 → (i) spawn-time observers + roots-first propagation pass.** Initial resolution via an
  `On<Add>` observer (reads own override → parent's raw override → global default); downstream
  propagation via a pass that fires on a parent's `Changed<Resolved<A>>` and flows the value through
  `Children`/`iter_descendants`. Standalone world text degenerates to today's 2-tier behavior (no
  parent → falls to global default; propagation triggers only on global-default change). Both paths
  call the same pure resolver.
- **D4 + D6 → `OverrideSource` enum walked via `EnumIter`, exhaustive per-attribute `read` match.**
  Supersedes the if-let ladder, and the enum's exhaustiveness *is* the cross-enrollment guardrail —
  no separate const-assert or sealed trait. A shared `enum OverrideSource { WorldTextStyle,
  DiegeticPanel, PanelText }` (precedence = declaration order). `CascadeAttr::read(source, node) ->
  Option<Self>` is an exhaustive `match` per attribute (no `_` arm); the provided `override_at`
  default method walks `OverrideSource::iter().find_map(|s| Self::read(s, node))`. Adding a variant
  breaks every attribute's match until handled, so a forgotten source is a compile error (this
  closes D6). Still reads via `EntityRef`, so D3 is untouched. Discipline: no `_ =>` wildcard arm
  (lint or review).
  - **Open follow-on (B), deliberately not solved by the enum:** a node carrying *two* source
    components stays expressible. A panel label carries both `WorldTextStyle` and `PanelText`;
    precedence silently picks `WorldTextStyle` first, safe today only because a label's
    `WorldTextStyle` leaves the attribute unset — an unchecked runtime convention. Eliminating it
    means changing what components a label is spawned with (one source per node), not the dispatch.
- **D5 → add `font_unit: Option<Unit>` to `PanelText` (true 3-tier parity).** Rejected the
  2-tier "match today's behavior" option: the stated intent is to genuinely cascade *more*
  attributes, so a panel child that cannot override font unit is a hole in the implementation, not a
  simplification. `FontUnit::override_at` reads the new field as its tier-1 source, mirroring
  `TextAlpha`.
