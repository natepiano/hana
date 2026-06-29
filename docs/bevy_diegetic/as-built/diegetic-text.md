# DiegeticText: one text type, element ids, and TextContent as the single source

> **Archived 2026-06-07 — implemented, with axis 3 later inverted.** Axes 1 and 2
> hold in the code today: `DiegeticText::world()/screen()` (`src/fluent.rs`)
> replaced `WorldText`/`ScreenText`, and runs carry stable `PanelElementId`s
> (`layout/builder.rs`). Axis 3 (Scope B: `TextContent` as the single source,
> tree `El` storing only an id) shipped, then the ownership direction was
> reversed during the June 2026 perf work: the tree's `El.text` is now
> authoritative, and `TextContent` is derived output that reconcile rewrites
> each pass (`render/world_text/mod.rs`). Runtime retexting goes through
> `PanelText::set_text` / `DiegeticTextMut` (`render/panel_text/access.rs`),
> which write the tree, not `TextContent`. Read Scope B sections as the
> intermediate state, not the current one.

## Goal

Continue the text unification begun in [`unify_text.md`](unify-text.md) along
three axes, all driven by the same constraint — **fewest types, one place per
fact**:

1. Collapse `WorldText` + `ScreenText` into a single **`DiegeticText`** with
   `DiegeticText::world(text)` / `DiegeticText::screen(text)` constructors,
   mirroring `DiegeticPanel::world()` / `DiegeticPanel::screen()`.
2. Give panel **text elements stable identifiers**, reusing `PanelElementId`, so a
   specific text run can be addressed and retexted at runtime.
3. Make **`TextContent` the single source of truth** for every text string
   (Scope B): the layout `El` stores an id, not a copy of the string, and the
   string lives once on the text child's `TextContent`.

This doc records the decisions reached in design discussion and hands the
implementation mechanics — especially the layout-engine resolve path — to team
review as genuine forks.

## What shipped already (lineage)

`unify_text.md` delivered: one `TextStyle` (markers collapsed), `WorldText` /
`ScreenText` fluent sugar over a one-element `DiegeticPanel`, lighting/sidedness
as cascade attributes, and the sugar terminal as `.bundle() -> impl Bundle` with
`.spawn()` over it. This session renamed the sugar terminal `.bundle()` →
`.build()` to match `DiegeticPanelBuilder::build()` (`fluent.rs:301`, `:411`).

This doc supersedes the `WorldText`/`ScreenText` *naming* (D1) and the
single-store text model still implicit in that work (D2).

## Where text lives today

Tracing the string through the pipeline:

```
fluent path:   TextContent(panel entity) ──► El.text (tree) ──► TextContent(child entity)
hand-built:                                   El.text (tree) ──► TextContent(child entity)
```

- `ElementContent::Text { text: String, config: TextStyle }` — the layout `El`
  node stores the string and style (`layout/element.rs:118-123`).
- The **layout engine is a pure function over the tree** —
  `MeasureTextFn = Arc<dyn Fn(&str, &TextMeasure) -> TextDimensions>`
  (`layout/engine/layout_engine.rs:27`), no `World` access. It reads
  `ElementContent::Text { text }` to **measure** (`layout_engine.rs:162`) and to
  **word-wrap** (`wrapping.rs:170`).
- `reconcile_panel_text_children` reads the computed tree and spawns/updates one
  `PanelTextChild` per run, copying the string into a child `TextContent`
  (`render/panel_text/reconcile.rs:208`, `:261`). `PanelTextChild` is
  `#[require(TextStyle, Transform, Visibility)]` (`render/world_text/mod.rs:85`).
- Child reuse across rebuilds is keyed by `(element_idx, command_index)`
  (`reconcile.rs:41-47`), surfaced on the child as `PanelTextLayout`
  (`render/panel_text/layout.rs:11-16`). The code comment flags this key as
  **not content-stable**: a row reorder changes `command_index`, so an unchanged
  run respawns rather than reuses.
- The fluent path additionally carries `TextContent` on the **panel entity**,
  read by `rebuild_fluent_text` (`fluent.rs:483`) to regenerate the one-element
  tree on string/style change. This panel-entity copy exists only to seed
  rebuilds.

So the same string lives in up to three places, and the only existing
named-element mechanism is `PanelElementId` (`ime/ids.rs:62`, a `String` newtype),
scoped to editable fields via `El::editable_field(field_id, …)`
(`layout/builder.rs:231-236`) with not-found / duplicate-id lookup errors
(`layout/element.rs:41-43`, `:403-414`).

## How other systems address a child element

| System | Mechanism |
|---|---|
| **Bevy UI** | None. ECS-native: marker components, the `Name` component (debug/inspector, no built-in `get_by_name`), or a saved `Entity`. |
| **Clay** (this engine's parity target) | String-hashed ids (`CLAY_ID`/`CLAY_IDI`), stored in a hashmap; the id is **both** addressing and reconciliation identity. |
| **egui** | `Id` = hash of label/source + parent scope; `push_id` to disambiguate loops. |
| **Flutter** | `Key` (`ValueKey`, `GlobalKey`); drives reconciliation and lookup. |
| **React** | `key` for list reconciliation; `ref` for direct addressing. |

The retained/immediate systems converge on one idea: a user-assigned key that
does double duty — addressing **and** reconciliation identity. Bevy is the
outlier only because every node already has a stable `Entity`. This engine sits
between the two (a pure-function layout tree *plus* materialized ECS children),
so it wants the Clay-style id for the tree and the Bevy-style entity handle for
mutation — bridged by an index.

## Decisions

### D1 — One `DiegeticText`, `::world` / `::screen` constructors

Collapse `WorldText` + `ScreenText` into a single `DiegeticText`. Coordinate
space is picked by the constructor name (mirroring `DiegeticPanel::world()` /
`screen()`, `diegetic_panel.rs:255`, `:261`); the **string is the constructor
arg** — it is the one required input. Size/anchor/wrap have defaults, so there is
no `NeedsSize` typestate; `DiegeticText::world("Hi")` is immediately buildable.

`DiegeticText` is a **facade, never a tree element**. It holds no string of its
own (see D2); it produces a one-element `DiegeticPanel` whose single text run
carries the string. It is not an `El`, so there is no recursion (a text node
inside a panel uses `El::text(...)`, not a nested `DiegeticText`).

Renames `WorldText`/`ScreenText` and the `FluentText` marker; folds
`rebuild_fluent_text` into the general panel rebuild path.

### D2 — `TextContent` is the single source of truth (Scope B)

One physical copy of each string. `TextContent` on the text-child entity is the
source. Consequences:

- The layout `El` stores a **`PanelElementId`, not the string**
  (`ElementContent::Text` drops its `text: String` field, keeps `config`).
- The layout pass resolves `id → &str` from `TextContent` transiently for
  measure/wrap — no stored second copy.
- `reconcile_panel_text_children` stops copying the string into children; the
  child already owns it. Reconcile builds meshes / `PanelTextLayout`, not the
  string.
- The fluent panel-entity `TextContent` seed is removed.

This is the change that makes "mutate `TextContent`" actually drive relayout
uniformly for single- and multi-element panels — today the flow is tree→child,
so mutating a child `TextContent` does not propagate back. **The mechanics of the
resolve path and the reconcile inversion are the central forks — see OQ1, OQ2.**

> **Caveat (as built, DT1=(b)).** The bullets above describe DT1=(a), which drops
> `El.text`. The DECIDED variant is DT1=(b): `El.text` is **kept as a derived
> cache** (see *Team review — cycle 1*). So the single-source claim is exact only
> for a **single-line** run, where the line-0 child's `TextContent` equals the
> run string. A **wrapped** run materializes as one child per visual line, each
> holding a per-line *slice*; the authoritative full run string is then `El.text`
> (the cache), not any child. The Phase 3 mutation API accounts for this:
> `set_text` writes the line-0 child (reactor → `El.text` → re-wrap); `text` reads
> `El.text`. See Phase 3 step 11.

### D3 — Element identifiers reuse `PanelElementId`

`El::text(text, config).id("title")` assigns a panel-local id, reusing
`PanelElementId`. The id does double duty:

- **Reconciliation identity** — replaces `(element_idx, command_index)` as the
  child reuse key, so a named run survives reorders (the fragility the
  `reconcile.rs:35-39` comment calls out).
- **Lookup handle** — the public way to address a run (D5).

Panel-local, like `PanelElementId` today, so two panels may both use `"title"`.

### D4 — Auto-id for unnamed text

Under D2 the id is the **resolution key** for the string, so every text leaf must
have one — it is no longer optional addressing sugar. Therefore:

- `.text(text, config)` stays unchanged and **auto-assigns** an id. Explicit
  `.id("…")` is added only where a run is addressed/mutated. This keeps the
  100+ static-label call sites untouched.
- **Namespace named vs auto** so they cannot collide by construction —
  `PanelElementId` distinguishes them (enum `{ Named(String), Auto(u32) }`, or auto
  uses a reserved form `From<&str>` cannot produce). See OQ3.
- Auto-id source: a **per-tree build-order counter** — i.e. exactly today's
  positional identity.
- **Duplicate explicit ids** are caught at build time (the builder tracks a set →
  `debug_assert!` or `Result`), reusing the editable-field duplicate-id error
  path.
- **Stability gradient**: named ids are content-stable and addressable; auto ids
  are positional-stable and not publicly addressable. *Name it to address it* —
  unnamed text still renders, you just cannot grab it later.

### D5 — Lookup and mutation API (relationship-based; revised by D7)

> The original D5 (a SystemParam-only `PanelText` over `text_index`) shipped in
> Phase 3. A post-Phase-3 design pass replaced its traversal with the D7
> relationship; the revised three-layer API is below. All three layers read or
> write a single source string — `TextContent` on the run child (D2).

- **Traversal — the `TextRunOf` / `PanelTextRuns` relationship (D7).** Each run
  child carries `TextRunOf(panel)`; the panel carries the Bevy-maintained
  `PanelTextRuns(Vec<Entity>)`. A system reaches a panel's runs by querying the
  component: `Query<&PanelTextRuns, With<Marker>>`. For a `DiegeticText` (one
  run) `PanelTextRuns::sole() -> Option<Entity>` returns it with no id — this is
  what makes a standalone label addressable from a marker on the panel entity.
- **Named lookup — `DiegeticPanel::text_child(&PanelElementId) -> Option<Entity>`,**
  the O(1) `id → Entity` map the panel retains, for a multi-run panel where a
  scan over `PanelTextRuns` would be O(n) (a large, possibly hidden panel must
  not table-scan). Unchecked: it returns the stored entity, which may be dead;
  the caller's content query or relationship membership confirms liveness.
- **Mutation — `DiegeticTextMut<M>` (the ergonomic public path; see Phase 4
  step 15).** A crate user retexts a marked label with one SystemParam and one
  call — `fn rename(mut labels: DiegeticTextMut<CubeFaceLabel>) { labels.set("hi"); }`
  — never touching `PanelTextRuns`/`TextContent`/`sole()`. The two-query form below
  is the mechanism `DiegeticTextMut` wraps internally, shown for understanding, not
  as the API users are expected to write:

  ```rust
  fn rename(
      labels: Query<&PanelTextRuns, With<MyLabelMarker>>,
      mut content: Query<&mut TextContent>,
  ) {
      for runs in &labels {
          let Some(run) = runs.sole() else { continue };
          let Ok(mut text) = content.get_mut(run) else { continue };
          text.set_text("hello world");
      }
  }
  ```

  `PanelText` / `PanelTextReader` stay as the id-addressed convenience for
  multi-run panels (`set_text(panel, &id, …)`) and for `sole_text` /
  `set_sole_text`; their internals resolve through the relationship + the
  `text_child` map rather than the old `Children` walk.

No string duplication: the relationship and the map both store entity
references, never text. The string lives in one `TextContent`. (A deferred
`Commands` write extension — `commands.set_panel_text(panel, id, "new")` — stays
optional, deferred until a consumer needs it; reads must be the query/method
forms above, since commands return nothing. See OQ4.)

### D6 — `TextStyle` unchanged

`TextStyle` stays exactly as `unify_text.md` left it: the authoring config
(`El::text(.., TextStyle)`, held by `DiegeticText`) and the per-child component
(`#[require]` on `PanelTextChild`) are deliberately the same type. No change.

### D7 — Panel↔run relationship (`TextRunOf` / `PanelTextRuns`)

A typed Bevy 0.19 relationship links a panel to its text runs, alongside the
`ChildOf` hierarchy the runs already sit in (syntax mirrors `ChildOf`/`Children`,
`bevy_ecs-0.19.0-rc.2/src/hierarchy.rs:105-152`):

```rust
#[derive(Component)]
#[relationship(relationship_target = PanelTextRuns)]
pub struct TextRunOf(#[entities] pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TextRunOf)]
pub struct PanelTextRuns(Vec<Entity>);
```

- **Why this, not just `Children`.** `Children` mixes text runs with a panel's
  other children (SDF geometry, images). `PanelTextRuns` holds only text runs, so
  traversal needs no `With<PanelTextLayout>` filter and the lone run of a
  `DiegeticText` is found with `PanelTextRuns::sole()`.
- **The problem it fixes.** A standalone `DiegeticText` is a one-element panel: a
  user marker sits on the **panel** entity, but `TextContent` sits on the run
  **child**, so `Query<&mut TextContent, With<Marker>>` matches nothing (the
  cube-face label path `orthographic`/`input_keyboard` is broken by exactly this).
  The relationship gives a marker query on the panel a typed hop to its run.
- **What it replaces.** The hand-built `text_index` full rebuild every reconcile
  pass (reconcile.rs:167-237) for the *liveness + traversal* job; Bevy maintains
  `PanelTextRuns` as runs spawn/despawn. The `id → Entity` map is retained **only**
  for O(1) `PanelElementId` lookup — a relationship target is an ordered `Vec`, not
  keyed by id.
- **No duplication.** Entity references only; the string stays single-homed in the
  run child's `TextContent` (D2). The run keeps `ChildOf(panel)` too (transform
  propagation, despawn); `TextRunOf` is an additive typed index over the text-run
  subset.

## Open questions — resolved in cycle 1

All seven are settled by decisions DT1–DT6 and the auto-recorded team-review
items; resolution is noted on each. Kept here for the reasoning trail.

> **OQ1 → DT1=(b).** **OQ2 → DT2=(a).** **OQ3 → DT3** (enum + shared namespace).
> **OQ4 → DT4-ii** (marker component + single-string helper, not a mutate-target).
> **OQ5 → DT4 / TR-D** (gradient accepted: named content-stable, auto positional).
> **OQ6 → TR-F** (defer the Commands extension). **OQ7 → TR-C** (perf gate with
> measurable criteria; DT1=(b) removes the per-pass gather, leaving the cache sync).

1. **OQ1 — The layout resolve path (central fork).** The engine is a pure
   `Fn(&str, &TextMeasure)` over the tree with no `World` access, yet under D2 the
   `El` no longer holds the string. Where does the `&str` come from at measure
   time? Candidates: (a) a pre-layout pass walks the tree and builds an
   `id → &str` map (borrowed from `TextContent`) passed alongside the tree to the
   engine; (b) the `El` keeps a `text` field demoted to a **layout cache** synced
   from `TextContent` before each layout (two physical copies again, but one
   logical source); (c) change `MeasureTextFn` to resolve ids itself. (a) keeps
   the tree string-free at rest but threads a side table through the engine; (b)
   is the smallest code change but reintroduces the copy D2 set out to remove.

2. **OQ2 — Reconcile inversion / chicken-and-egg.** Today reconcile derives the
   child `TextContent` from the tree (tree→child). D2 wants `TextContent` as the
   source. So how is the child first created? Likely the authoring step
   (`El::text` / `DiegeticText`) spawns the child entity with its `TextContent`
   and the `El` references it by id, so reconcile only attaches render data
   (meshes / `PanelTextLayout`) to a pre-existing child. This restructures both
   reconcile and the relationship between `LayoutBuilder` (pure data) and entity
   spawning — the largest change in the doc.

3. **OQ3 — `PanelElementId` representation.** Enum `{ Named(String), Auto(u32) }`
   vs `String` with a reserved auto-form. The enum is collision-proof by
   construction and keeps `From<&str>` always-`Named`; it is a small extension to
   the "PanelElementId is fine for now" decision.

4. **OQ4 — `DiegeticText` as a live component.** Does `DiegeticText` persist as a
   component you can also mutate, or is it pure authoring sugar with all mutation
   through `TextContent`-by-id? The session leaned toward the latter (uniform
   mutation), which argues for `DiegeticText` being build-time only.

5. **OQ5 — Auto-id stability.** Accept that unnamed text keeps today's positional
   reuse semantics (auto-id = build-order counter), reserving content-stability
   for named runs? This is the proposed gradient; confirm it is acceptable.

6. **OQ6 — Commands write-extension scope.** Ship `set_panel_text` now, or defer
   until a consumer needs the deferred form (the SystemParam covers the
   in-system case)?

7. **OQ7 — Perf of per-element resolve.** OQ1(a)/(b) add an `id → &str` gather
   per layout pass. Given the known freeze path
   (`project_diegetic_panel_freeze.md`) and debug draw-call cost
   (`project_units_glacial_perf.md`), confirm the resolve does not regress the
   high-label-count examples in release.

## Migration inventory (examples — separate, post-core pass)

The affected surface is ~31 example files, but `LayoutBuilder::text(...)` is used
100+ times for static labels that **do not change** thanks to auto-id (D4). The
substantive work:

- **`WorldText`/`ScreenText` → `DiegeticText`** — no external call sites today
  (the sugar is new); effectively nothing to migrate.
- **Runtime mutation sites** — `bevy_lagrange/examples/input_manual.rs:277`,
  `input_keyboard.rs:192`, `orthographic.rs:127`, `bevy_diegetic/examples/`
  `typography.rs:641-642`. **Correction (D7):** the three cube-face sites
  (`input_manual`, `input_keyboard`, `orthographic`) do **not** keep working — a
  `DiegeticText` marker sits on the panel entity while `TextContent` sits on the
  run child, so their `&mut TextContent` queries match nothing. They are migrated
  to `DiegeticTextMut<M>` in Phase 5 step 16c (this is required, not optional).
  Sites that mutate a run they already hold the `Entity`/id for (panel-internal
  cases) still work via `TextContent` directly.
- **`.text()` callers that get mutated** — add explicit `.id(...)` to just those
  few runs.
- **Standalone `TextContent` spawn docs** (`render/world_text/readiness.rs:15`) —
  retire the legacy pattern.

Run this as a dedicated pass **after** the core API lands, tracked separately so
example churn does not muddy the core diff.

## Implementation plan of record (library-first)

The six DT decisions are settled (see *Team review — cycle 1*). DT1=(b) +
DT2=(a) keep the layout engine's signature untouched, so the old "Scope B
inversion" is no longer the risky core — it reduces to a cache + an observer.
Each phase below cites the decision it implements.

### Phase 0 — types + the `PanelTextChild` collapse (mechanical, no behavior change)
1. **DiegeticText (DT1 naming, DT4-i/ii).** Collapse `WorldText`/`ScreenText`
   into one `DiegeticText` with `world(text)` / `screen(text)` constructors;
   space is a runtime `CoordinateSpace` field, not a type param. `DiegeticText`
   is a lightweight marker component on the spawned text entity. Reuse the
   internal one-element-panel builder.
2. **`PanelElementId` → enum (DT3).** `enum { Named(String), Auto(u32) }`;
   `From<&str>`/`From<String>` always yield `Named`. Editable-field call sites
   (`impl Into<PanelElementId>`) keep compiling.
3. **Remove the panel-root `TextContent` seed first (DTX-1).** Before any filter
   swap, delete the fluent panel-root `TextContent` seed (`fluent.rs:328`, `:441`)
   and the `FluentText` marker, so the only `TextContent` left is on run entities.
   This pulls Phase 2 step 9 forward — it must precede step 4 below, or a child
   query would transiently match one-element fluent roots. (Relayout-on-string-edit
   for the fluent path now goes through the Phase 2 observer, step 10, not the old
   panel-root seed.)
4. **Delete `PanelTextChild` (DT4-iii).** Move `#[require(TextStyle, Transform,
   Visibility)]` onto `TextContent`; delete `PanelTextChild`; swap every
   `With<PanelTextChild>` → `With<TextContent>` and `Without<PanelTextChild>` →
   `Without<TextContent>` (`shaping.rs:33` already filters `With<TextContent>`).
   With the root seed gone (step 3), `With<TextContent>` now matches run entities
   only. Broad but mechanical; no behavior change.

### Phase 1 — element ids
5. **Id field + setter (DT3, DTX-3).** Add a `PanelElementId` to
   `Element` (alongside the `text` cache from Phase 2); add
   `El::id(...)` and `Text::id(...)`. `.id()` returns `Self` to keep the
   builder chain; callers that need the id at lookup bind it as a value first
   (`let id = PanelElementId::named("title"); ….id(id.clone())`), mirroring
   `editable_field`'s arg-passed id (DTX-3=a) — no chain-returned handle. Auto-id
   from a per-tree build-order counter (`u32`, TR-E), reset per build (TR-N);
   element ids and editable-field ids share one panel-local namespace and one
   duplicate check (TR-O). `Auto` is not publicly constructible (TR-K).
6. **Duplicate ids → `Result` at build (DT6-i).** A repeated explicit id is an
   error on the existing `build() -> Result`; no silent release shadowing.
7. **Id-keyed reconcile + index (DT3, TR-A).** Switch the reconcile reuse key
   from `(element_idx, command_index)` to the id (named runs survive reorder;
   auto runs keep positional semantics, TR-D). Build the `id → Entity` index
   from scratch each reconcile; clear it on `set_tree`.

### Phase 2 — `TextContent` as the source (DT1=b, DT2=a)
**Status: ✅ complete.**

8. **`El.text` becomes a synced cache (DT1-b).** Add a sync step that writes
   `TextContent → El.text` before layout, ordered `.before(ApplyTreeChanges)`
   (the ordering `rebuild_fluent_text` already uses). The engine's read sites
   (`layout_engine.rs:162`, `wrapping.rs:170`, `positioning.rs`) are unchanged;
   `TextContent` is the logical single source, `El.text` a derived cache.
9. **Panel-root `TextContent` copy already gone (DTX-1).** The rebuild seed and
   `FluentText` marker were removed in Phase 0 step 3; nothing to do here.
10. **Relayout-on-edit observer (DT2-a, DTX-2).** Add an observer on
   `Changed<TextContent>, Without<ReconcileOwned>` (now only run entities) that
   writes the new string into the parent's `El.text` cache and dirties
   `ComputedDiegeticPanel`, re-running reconcile/relayout. To stop reconcile's own
   `TextContent` write from re-firing the observer (a 2× layout pass per edit),
   reconcile inserts a `#[doc(hidden)] ReconcileOwned` marker on the runs it writes
   and the observer filters it out; the marker is cleared the next frame (DTX-2=a).
   First-frame bootstrap is trivial — the string is in the cache at build time.

### Retrospective

**What worked:**
- The cache+gate model landed exactly as DT1=(b)/DT2=(a)/DTX-2=(a) specified: a
  `Changed<TextContent>` reactor writes `El.text` and dirties the panel; a
  `ReconcileOwned` marker keeps reconcile's own writes from re-firing it.
- The relayout property (edit a child `TextContent` → tree re-wraps) is proven by
  `editing_child_text_content_relayouts_and_syncs_the_cache`.

**What deviated from the plan:**
- Steps 8 and 10 collapsed into **one** system, not two. They describe the same
  mechanism (cache sync + dirty) from two angles; there is no separate per-frame
  sync walk — that would be the O(n_elements) pass TR-L forbids.
- "Observer" is a **regular system with a `Changed<TextContent>` query filter**,
  not a Bevy `On<…>` observer. A `&mut TextContent` deref-mutation (how `PanelText`
  will edit in Phase 3) sets the `Changed` flag but fires no `Insert` lifecycle
  event, so an `On<Insert>` observer would miss the edit. Scheduled
  `sync_run_text_to_cache.before(PanelSystems::ApplyTreeChanges)` +
  `clear_reconcile_owned.after(sync_run_text_to_cache)` in `Update`.

**Surprises:**
- The cache write **must bump `tree_revision`** or `ScaledLayoutTreeCache` serves a
  stale scaled tree (keyed on revision). Added `DiegeticPanel::sync_run_text_cache`
  to pair the `El.text` write with the bump; `LayoutTree::set_element_text` returns
  whether it changed so an equal string skips the bump (and the re-measure, per
  TR-L).
- `ReconcileOwned` must survive into the **next frame's** sync pass: reconcile
  writes in `PostUpdate`, sync reads in the following `Update`, so the marker is
  cleared `.after(sync)`, not eagerly.
- `clippy::pedantic` surfaced latent Phase 0/1 debt (a lossy `f32 as u32` auto-id
  cast, a non-`const` `take_auto_id`, and `reconcile_panel_text_children` over 100
  lines). Fixed in passing — the >100-line function was split via
  `collect_text_commands`. Phases 0/1 had been built/tested but not clippy-gated.

**Implications for remaining phases:**
- **Phase 3** mutates via `PanelText`'s `Query<&mut TextContent>` — a deref write,
  which the Phase 2 reactor already observes. The relayout wiring Phase 3 depends on
  is in place; Phase 3 adds only the lookup (`text_child`) and the write ergonomics.
- The `text_index` (line-0 only) means `text_child(id)` returns the run's **first
  line**; the Phase 2 reactor reads that child's full string into `El.text`, so a
  multi-line run is edited by setting the whole run text on the line-0 entity. Phase
  3's `set_text` helper and docs should say so.
- **Phase 5** should run `cargo clippy` as a first-class gate (not only `build` +
  tests), since pedantic catches what `build` does not.

### Phase 2 Review

Remaining phases reviewed against the Phase 2 retrospective by a `Plan` subagent.
Outcomes folded in:

- **Phase 3 step 12 — `PanelText` name collision.** The crate already has a
  private `pub(super) struct PanelText`; rename it (`PreparedPanelText`) and keep
  the public SystemParam named `PanelText`. Scope reader/writer queries
  `With<PanelTextLayout>`, not bare `With<TextContent>`.
- **Phase 3 step 11 — liveness re-scoped.** `DiegeticPanel::text_child` is an
  unchecked `&self` index lookup (no `World` access); the TR-Q liveness `None`
  lives in the SystemParam's `Query::get`. Phase 5's orphan test drives through
  the SystemParam, not the bare method.
- **Phase 3 step 11 — wrapped-run read/write semantics decided.** A wrapped run is
  one child per line; `set_text` writes the line-0 child (reactor re-wraps),
  `text` reads `El.text`. D2 got a matching caveat. The per-line-vs-per-run model
  was raised by the user and left as a separate future investigation (per-line
  keeps wrapping in the pure layout pass and gives per-line culling for scroll).
- **Phase 3 step 11 — same-frame `set_tree` window.** `set_text` in the same frame
  as `set_tree` no-ops until reconcile rebuilds the index; documented.
- **Phase 4 step 13 — crate boundary.** The migration spans `bevy_lagrange`'s
  examples (a sibling crate); build/verify each crate separately.
- **Phase 5 step 14 — added gates/tests.** Clippy is a first-class gate; added a
  wrapped multi-line `set_text` test and a single-pass assertion (exactly one
  `ComputedDiegeticPanel` change per edit, proving the `ReconcileOwned` gate).

### Phase 3 — lookup + mutation API
**Status: ✅ complete.**

11. **`text_child` + helper (DT5, DT6-ii, DT4-ii).**
    `DiegeticPanel::text_child(&PanelElementId) -> Option<Entity>` is an
    **unchecked** index lookup — it returns the stored `Entity`, which may be
    dead (the method takes `&self` and has no `World`/`Entities` access, so it
    cannot validate liveness). The TR-Q liveness guarantee ("a despawned child
    returns `None`") therefore lives one layer up, in the SystemParam (step 12):
    its layout-query check (`self.layouts.contains(child)`) fails on a
    dead/despawned entity, yielding `None`.
    *Miss diagnostic (DT6-ii, discriminated).* A `text_index` miss is ambiguous —
    a genuine typo vs. the id not yet materialized (first frame / post-`set_tree`,
    while reconcile has not rebuilt the index). The index alone cannot tell them
    apart, so a blind `warn!` on every miss would spam the not-ready window. The
    **authoritative** oracle is the layout tree, which holds every valid id at
    build time independent of reconcile timing. So on a miss the SystemParam
    consults `panel.tree().contains_text_id(id)`: id absent from the tree → genuine
    typo → debug-only `warn!`; id present in the tree (just not in the index yet,
    or its entity died mid-frame) → silent. The `warn!` is `#[cfg(debug_assertions)]`
    (zero release cost); `contains_text_id` is an O(elements) walk that runs only on
    a miss. The one-run helper shipped as
    `PanelText::sole_text(panel)` / `set_sole_text(panel, …)` (**not** the
    originally-planned `text()`/`set_text()`: those names are the id-addressed
    methods and Rust has no overloading) — no id needed; the helper resolves the
    panel-root marker to its lone run via the SystemParam (`Query<&Children>` +
    `With<PanelTextLayout>`, the `line_index == 0` child), since the run's `Auto`
    id is not caller-addressable.
    *Wrapped-run read/write semantics (decided).* A wrapped run materializes as
    **one child per visual line** (the engine emits one `RenderCommandKind::Text`
    per line, `positioning.rs`), and `text_index` keeps only the `line_index == 0`
    child; the authoritative full run string is `El.text`
    (`tree().element_text(idx)`), not any single child slice. Therefore:
    `set_text(id, s)` **writes the line-0 child's `TextContent`** — the Phase 2
    reactor syncs it to `El.text` and re-wraps (works single- and multi-line, the
    caller passes the whole string); `text(id)` **reads `El.text`**, never the
    line-0 slice (or a wrapped run's getter returns only its first line). The
    per-line-entity model is pre-existing layout-engine architecture (wrapping is
    resolved in the pure pass so the engine can size the container); a per-run
    single-entity model was considered and left as a separate future
    investigation, not a Phase 3 change.
    *Same-frame caveat:* `set_tree` clears `text_index` immediately
    (`diegetic_panel.rs` `set_tree_command`) but reconcile only repopulates it on
    the next `Changed<ComputedDiegeticPanel>`, so a `text_child`/`set_text` call
    in the same frame as a `set_tree` returns `None`/no-ops until the rebuild —
    document this.
12. **`PanelText` SystemParam + reader (TR-B).** `PanelText` bundles the panel +
    run queries for get/set by id; add a read-only `PanelTextReader` so reader
    systems don't serialize on `&mut TextContent`. The deferred
    `commands.set_panel_text(…)` extension is deferred until a consumer needs it
    (TR-F). **Name collision:** `render/panel_text/mod.rs` already has a private
    `pub(super) struct PanelText` (the prepared-run component used by shaping /
    mesh spawning). Free the name for the public SystemParam by renaming the
    internal struct (e.g. `PreparedPanelText`); keep the public API named
    `PanelText`. Scope the reader/writer run queries `With<PanelTextLayout>`
    (not bare `With<TextContent>`) so they match only reconcile-spawned run
    children, never a future standalone `TextContent`.

### Retrospective

**What worked:**
- `PanelText`/`PanelTextReader` landed as one nested SystemParam pair
  (`render/panel_text/access.rs`): `PanelText` embeds `PanelTextReader` as a field
  and delegates every read to it, so the read logic exists once. Reads come from
  the `El.text` cache (`tree().element_text(element_idx)`); the writer adds only
  `Query<&mut TextContent, With<PanelTextLayout>>`.
- TR-Q liveness is the SystemParam's layout-query check, not the bare lookup:
  `entity()` does `text_child(id)` then `self.layouts.contains(child)`, so a run
  despawned out of flow (before the next reconcile rebuilds `text_index`) reads
  back `None`. Proven by `orphaned_run_resolves_to_none_through_the_system_param`.
- The `PreparedPanelText` rename was confined to three files
  (`mod.rs`/`shaping.rs`/`mesh_spawning.rs`), all `pub(super)`; done with
  token-anchored edits, not a substring replace (which would have hit
  `PanelTextLayout`).

**What deviated from the plan:**
- The one-run `DiegeticText` helper is `sole_text(panel)` / `set_sole_text(panel,
  s)`, **not** the planned `text()` / `set_text()`. Rust has no overloading and
  those names are the id-addressed methods (`text(panel, id)` /
  `set_text(panel, id, s)`); a no-id overload is impossible, so the one-run path
  got distinct names.
- The liveness comment in step 11 promised a "debug-only `warn!`" on a lookup
  miss. Not implemented: a miss is a quiet `None` (the `then_some` / `Query::get`
  path). DT6-ii's `warn!` was never wired; left out to avoid log spam on the
  legitimate "panel not built yet" case. Plan text overstates current behavior.
- The named-run authoring API is `LayoutBuilder::text_id(id, text, config)`
  (built in Phase 1), not the `El::text(..).id(..)` chain the prose still
  references in places.

**Surprises:**
- Reading `text(id)` needs the run's `element_idx`, which lives on the child's
  `PanelTextLayout`, not in `text_index` (id → Entity only). So a read is
  id → Entity (index) → `element_idx` (layout query) → `El.text` (tree). Three
  hops, all O(1).
- `bevy_diegetic`'s **own** examples don't compile: Phase 1's `build() ->
  Result<_, PanelBuildError>` broke `font_features.rs` (and others) that still
  match `InvalidSize`. `cargo build -p bevy_diegetic` hides this (examples aren't
  built); `cargo nextest`/`--examples` surfaces it. This is Phase 4 work but it
  means the crate's example target is currently red.

**Implications for remaining phases:**
- **Phase 4** must fix the `InvalidSize` → `PanelBuildError` example breakage in
  *this* crate's examples too, not only adopt the new API in `bevy_lagrange`. The
  step 13 "crate boundary" note already says build/verify each crate separately;
  the concrete first task is un-breaking `bevy_diegetic/examples/*`.
- **Phase 5** test list (step 14) still references `DiegeticPanel::text_child` for
  the orphan case and `set_text` for the wrapped case — the orphan test must go
  through `PanelText`/`PanelTextReader` (it does, in `access.rs`); the wrapped
  multi-line `set_text` test and the single-pass `ComputedDiegeticPanel`
  assertion are **not yet written** (Phase 3 added single-line coverage only).
- Plan prose mentioning `text()`/`set_text()` for the one-run case and the
  debug-`warn!` on miss should be reconciled with the shipped `sole_text` /
  quiet-`None` reality.

### Phase 3 Review

Remaining phases (4, 5) reviewed against the Phase 3 retrospective by a `Plan`
subagent. Outcomes folded in:

- **Phase 5 step 14 — re-scoped.** Six listed tests already shipped in
  `access.rs` (named resolve, unknown-id, auto-not-addressable, mutate-relayouts,
  orphan-through-SystemParam, `set_sole_text`); marked done so they aren't
  re-authored. Genuinely open: duplicate-id-at-build, reorder (named survives /
  auto respawns), `set_tree`-clears-index, wrapped multi-line `set_text`, and the
  single-pass `ComputedDiegeticPanel` assertion.
- **Phase 4 step 13 — split required/optional + scope corrected.** New 13a
  (**required**): un-break the `InvalidSize`→`PanelBuildError` example breakage
  from Phase 1, confirmed across `bevy_diegetic/examples/{font_features,units,
  aa_text,cascade}.rs` and `bevy_lagrange/examples/{focus_bounds,follow_target,
  animation}.rs`. 13b (optional): adopt `text_id` + `PanelText`. The old inventory
  claim that examples "keep working" was wrong.
- **Phase 5 step 15 — sequencing + path preconditions.** The perf gate can't run
  until 13a makes `cascade` (et al.) compile; and since `PanelText` adoption is
  optional (TR-I), at least one cube-face example must adopt it so the gate
  profiles the new write path, not the old marker-query path.
- **Prose corrected** in step 11 / step 13: `sole_text`/`set_sole_text` (not
  `text`/`set_text`) for the one-run helper; `LayoutBuilder::text_id(id, …)` (not
  a `.text(..).id(..)` chain) for named-run authoring.
- **DT6-ii `warn!` decision (user-approved, option c).** The promised debug `warn!`
  was not implemented (shipped a quiet `None`). Rather than strike it, wire it
  *correctly*: discriminate a real typo from the not-yet-materialized window via
  `LayoutTree::contains_text_id` (the tree is authoritative at build time; the
  index is a reconcile-timed cache). `warn!` only when the id is absent from the
  tree, `#[cfg(debug_assertions)]`-gated. Implemented as a Phase 3 follow-up (one
  tree method + the debug check in `access.rs` + a test).
- **Plan revised after Phase 3 — Panel↔run relationship inserted (D7).** A
  design pass found that the marker-on-panel + `TextContent`-on-child split
  leaves a standalone `DiegeticText` unaddressable by the natural
  `Query<&mut TextContent, With<Marker>>` (marker and text on different entities;
  the cube-face label path is broken by it). The fix is a typed
  `TextRunOf`/`PanelTextRuns` relationship — it becomes the new **Phase 4**,
  pushing examples → **Phase 5** and verify → **Phase 6**. Phase 3's
  `PanelText`/`text_index` access still stands; Phase 4 re-homes its traversal
  onto the relationship.

### Phase 4 — Panel↔run relationship (D7)
**Status: ✅ complete.**

Inserted after Phase 3. Delivers the simple `DiegeticText` mutation API and fixes
the broken marker path before any example migration depends on it.

13. **Introduce the relationship pair.** Add `TextRunOf` (on each run child,
    `#[relationship(relationship_target = PanelTextRuns)]`, `#[entities] pub Entity`)
    and `PanelTextRuns` (on the panel, `#[relationship_target(relationship = TextRunOf)]`,
    `Vec<Entity>`). In `spawn_panel_text_child` (reconcile.rs:305-336), insert
    `TextRunOf(panel_entity)` on the spawned child so Bevy maintains
    `PanelTextRuns` automatically. The run keeps `ChildOf(panel)` for
    transform/despawn — `TextRunOf` is an additive typed index over the text-run
    subset. Add `PanelTextRuns::sole() -> Option<Entity>` (the lone run iff the set
    has exactly one), plus a thin `iter()` passthrough.

    *Verified setup details (Phase 4-6 review, checked against `bevy_ecs-0.19.0-rc.2/src/hierarchy.rs`):*
    - **Module home + exports.** Define both in a new
      `render/panel_text/relationship.rs`; re-export from `panel_text/mod.rs` and
      `lib.rs`. `PanelTextRuns` is part of the public API — a consumer writes
      `Query<&PanelTextRuns, With<Marker>>`, so it must be nameable.
    - **No `linked_spawn` on `PanelTextRuns`.** `ChildOf`/`Children` already
      `linked_spawn`-despawns the runs when the panel dies (hierarchy.rs:148); a
      second `linked_spawn` on `PanelTextRuns` would be a second recursive-despawn
      path (double-despawn). Without it, `PanelTextRuns` still auto-empties when a
      run despawns — the relationship's on-remove hook drops it from the `Vec`
      regardless. `ChildOf` owns despawn; `TextRunOf` is a typed traversal index.
    - **Field form matches `ChildOf`.** `ChildOf(#[entities] pub Entity)` ships a
      public field plus a `parent()` accessor (hierarchy.rs:107, :112); mirror it —
      `TextRunOf(#[entities] pub Entity)` with a `panel()` accessor. The TR-K
      unforgeability rule is for the `PanelElementId::Auto` value variant, not a
      relationship source; the relationship machinery is the writer here.
    - **`Deref<[Entity]>` for `PanelTextRuns`.** `Children` impls it
      (hierarchy.rs:255); do the same so `sole()` reads the private `Vec` via
      `len()`/indexing and consumers get `len()`/`iter()`/`for run in runs` free.
    - **Reuse path must not re-insert `TextRunOf`.** Insert it only on *newly
      spawned* runs (the `with_children` spawn). The reconcile reuse branch
      (reconcile.rs:~200-244) must skip it — a reused run already carries it, and
      re-inserting fires the relationship hook and mutates `PanelTextRuns` on a
      no-op, breaking the "mutates only on spawn/despawn" perf invariant (step 18).
    - **`register_type` is reflection-only.** The `#[relationship]` derive registers
      the maintenance hooks; `app.register_type::<TextRunOf>()` /
      `::<PanelTextRuns>()` is needed only for inspector/reflection parity, not for
      the relationship to populate. Add it for parity; correctness does not depend
      on it.

14. **Rewire reconcile + retain the named map.** Reconcile's reuse pass scans
    `existing_children` filtered by `child_of.parent() == panel_entity`
    (reconcile.rs:147-164); source the panel's existing runs from `PanelTextRuns`
    instead. Keep the `text_index` (`id → Entity`) map for O(1) `PanelElementId`
    lookup — the relationship target is an unkeyed `Vec`, and a large (possibly
    hidden) multi-run panel must not table-scan. Division of labor: the
    relationship owns liveness + traversal + single-run findability; the map owns
    named O(1) lookup. `set_tree` still empties `text_index` immediately
    (diegetic_panel.rs:478), but the old runs stay alive — and stay in
    `PanelTextRuns` — until reconcile despawns them next pass. So in the
    same-frame-after-`set_tree` window, `sole()`/`text_child(id)` can hand back a
    run about to be despawned; this is a wasted write, not corruption (the
    documented Phase 3 same-frame caveat), and the SystemParam's liveness check
    (`self.layouts.contains(child)`, access.rs) resolves a genuinely-dead entity to
    `None`. `PanelTextRuns` and `text_index` can transiently disagree across this
    window; neither is authoritative outside reconcile.

15. **Re-home sole-run access on the relationship.** `PanelTextReader::sole_text`
    / `PanelText::set_sole_text` resolve the lone run via `Query<&PanelTextRuns>` +
    `sole()` instead of the `Query<&Children>` walk filtered by
    `With<PanelTextLayout>` (access.rs:92-108). Named `text`/`set_text` keep
    `text_child(id)`. Document the canonical `DiegeticText` retext as the two-query
    marker form (D5): `Query<&PanelTextRuns, With<Marker>>` + `Query<&mut
    TextContent>`, `runs.sole()` → `content.get_mut(run)` → `set_text`. Add a
    `PanelTextRuns::sole()` helper test and the end-to-end marker-retext test
    (Phase 6).

    *Marker-driven convenience SystemParam — `DiegeticTextMut<M>` (Phase 4-6
    review, user-decided).* The bare two-query `sole()` form is the wrong public
    ergonomics for "retext a label": no crate user should hand-write a `sole()` +
    `get_mut()` + loop to set one string. Add a marker-generic SystemParam that owns
    both queries so the caller names only its own marker:

    ```rust
    #[derive(SystemParam)]
    pub struct DiegeticTextMut<'w, 's, M: Component> {
        runs:    Query<'w, 's, (Entity, &'static PanelTextRuns), With<M>>,
        content: Query<'w, 's, &'static mut TextContent>,
    }
    impl<M: Component> DiegeticTextMut<'_, '_, M> {
        pub fn set(&mut self, text: impl Into<String>) -> usize;       // every M: same string
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, Mut<TextContent>)>;
    }
    ```

    Caller: `fn rename(mut labels: DiegeticTextMut<CubeFaceLabel>) { labels.set("hi"); }`
    — one param, one call, no `PanelTextRuns`/`TextContent`/`sole()` in user code.
    `set` covers the single-label and uniform cases; `iter_mut` covers per-entity
    different strings (cube faces) while still hiding the `sole()`/`get_mut()` hop.
    Internally `set`/`iter_mut` resolve each matching panel's lone run via
    `PanelTextRuns::sole()`. Monomorphization is per *distinct marker type used in a
    system* (a handful), independent of label-entity count; an unused marker costs
    nothing.
    *Keep `PanelText`/`PanelTextReader`* for the entity-addressed named-multi-run
    case (`set_text(panel, &id, …)` via `text_index`). Division of the public
    surface: marker → `DiegeticTextMut<M>`; named id on a multi-run panel →
    `PanelText`. Document both in the `access.rs` module doc and link from D5.

    *As built (Phase 4 review — supersedes the snippets above on three points).*
    - **`DiegeticTextMut` holds three queries, not two.** Resolving a label's lone
      run filters `line_index == 0` (so a *wrapped* label still resolves), which
      needs `PanelTextLayout`: `runs: Query<(&M, &PanelTextRuns)>`,
      `layouts: Query<&PanelTextLayout>`, `content: Query<&mut TextContent>`. The
      marker `&M` is read directly, so there is no `With<M>` filter.
    - **`for_each_mut`, not `iter_mut`.** The shipped per-label method is
      `for_each_mut(&mut self, impl FnMut(&M, &mut TextContent)) -> usize` — a Bevy
      mutable many-entity query is a lending iterator and cannot be an
      `impl Iterator`. It yields the marker `&M` (more useful than the run entity
      for per-label dispatch). `set(text) -> usize` is unchanged.
    - **The lone run is resolved by the `line_index == 0` helper, not
      `PanelTextRuns::sole()`.** `sole()` is count-based (`Some` only for a
      one-entity set), so it returns `None` for a wrapped label. The two-query
      `runs.sole()` → `get_mut` snippet (D5) is therefore correct *only for a
      single-line label* — treat it as illustration, not the canonical pattern.
      The canonical retext is `DiegeticTextMut<M>::set` / `for_each_mut`; the
      `access.rs` `lone_run` helper does the wrapped-safe resolution behind it.

### Retrospective

**What worked:**
- The relationship pair landed in a new `render/panel_text/relationship.rs`
  (`TextRunOf` / `PanelTextRuns`), re-exported through `panel_text/mod.rs`,
  `render/mod.rs`, and `lib.rs`. `PanelTextRuns` auto-populates from the
  `#[relationship]` derive's hooks — the plugin only `register_type`s the pair
  for reflection parity. Proven by `panel_text_runs_populates_and_sole_returns_the_lone_run`.
- `DiegeticTextMut<M>` retexts marked labels end to end: `set` (uniform) and the
  per-label form both relayout. Proven by `diegetic_text_mut_set_retexts_a_marked_label`
  and `diegetic_text_mut_for_each_mut_sets_per_label_strings`.
- Reconcile now sources a panel's existing runs from `PanelTextRuns` (reuse pass
  + despawn pass), not a world-wide `TextContent` scan filtered by parent.
- 246 `bevy_diegetic` tests pass (+3 new), `cargo build --workspace --examples`
  is green, and clippy (pedantic/nursery) is clean.

**What deviated from the plan:**
- **`iter_mut` → `for_each_mut`.** The planned
  `iter_mut() -> impl Iterator<Item = (Entity, Mut<TextContent>)>` is not
  implementable: a Bevy mutable many-entity query is a *lending* iterator
  (`QueryManyIter` / `fetch_next`) and cannot be returned as `impl Iterator`, and
  collecting `Vec<Mut<_>>` aliases the query. Shipped
  `for_each_mut(&mut self, impl FnMut(&M, &mut TextContent)) -> usize`. It yields
  the **marker `&M`** (not the run `Entity`), which is strictly more useful for
  per-label dispatch — a cube face maps its string off the marker, which the run
  entity cannot provide.
- **`sole_run_entity` keeps the `line_index == 0` filter** instead of calling
  `PanelTextRuns::sole()`. A wrapped `DiegeticText` materializes as one run entity
  per visual line, so its set holds >1 entity and count-based `sole()` returns
  `None` — using it would regress wrapped-label resolution that the old `Children`
  walk supported. `sole()` (exactly-one-entity) is kept for the genuinely-single
  case (tests, simple labels). `DiegeticTextMut` shares the same `line_index == 0`
  helper (`lone_run`), so it is wrapped-correct too — at the cost of an internal
  third query (`PanelTextLayout`) the plan's two-query snippet did not show. The
  public surface is unchanged: the caller still names only `M`.
- **`existing_runs` reconcile query dropped `Entity` + `&ChildOf`** — runs now
  arrive from `PanelTextRuns`, so the query is random-access (`get(run)`) and
  needs neither the entity column nor the parent filter.

**Surprises:**
- The `#[relationship_target]` derive generates an inherent
  `iter(&self) -> impl Iterator<Item = Entity>` (by value). My own inherent
  `iter()` returning `slice::Iter<&Entity>` shadowed it with different item
  semantics and broke the `for &run` loops; removed it and rely on the derive's
  `iter()` (yields `Entity`) plus `Deref<[Entity]>` for `len()`/indexing. Loops
  bind `for run in runs.iter()` with `run: Entity`.
- `Reflect` registration needs `FromWorld` for `TextRunOf` (mirrors `ChildOf`):
  Reflect deserialize constructs-then-patches, so a relationship source needs a
  placeholder ctor. Added `impl FromWorld for TextRunOf` returning
  `Entity::PLACEHOLDER`.

**Implications for remaining phases:**
- **Phase 5 step 16c/16d** must use `for_each_mut(|face, content| …)` (not the
  planned `iter_mut`) for per-label strings, and `set` for uniform updates. The
  marker `M` must carry the face/label identity (cube-face marker already does),
  since `for_each_mut` yields `&M`, not the run entity.
- **Phase 6 step 17** — some relationship tests shipped early in `access.rs`
  (`PanelTextRuns` populate + `sole`, `DiegeticTextMut::set`,
  `for_each_mut` per-label). Still to write: panel despawn drops all runs with no
  panic/double-despawn; two no-op reconcile passes mutate `PanelTextRuns` zero
  times (the per-frame-free invariant); `set_tree` empties then repopulates;
  `sole()` is `None` for a multi-run panel.

### Phase 4 Review

Remaining phases (5, 6) reviewed against the Phase 4 retrospective by a `Plan`
subagent. All twelve findings had a single in-intent outcome (align the plan with
the shipped API or cover a test/site gap) — none was a user decision, so all were
applied straight into the plan:

- **Step 16c — third broken site added.** `input_manual.rs:277`
  (`Query<(&ManualFaceLabel, &mut TextContent)>`) is broken by D7 exactly like
  `input_keyboard`; added as a required migration target →
  `DiegeticTextMut<ManualFaceLabel>::for_each_mut`. Migration inventory's "keeps
  working" claim corrected.
- **Step 16c — per-face identity source clarified.** The per-face dispatch keys on
  the example-local enum (`KeyboardFaceLabel`/`ManualFaceLabel`), not on
  `fairy_dust::CubeFaceLabel` (a unit struct with no identity — only
  `orthographic`'s uniform `set` keys on it). Audit grep widened to bare
  `&mut TextContent` (markers sit in the query tuple, not a `With<>` filter).
- **Step 16c / step 15 — canonical doc-example corrected.** The `CubeFaceLabel`
  doc-example must show a `DiegeticTextMut::set` call, never the two-query
  `sole()` form (wrong for a wrapped label). Step 15 gained an *As built* note:
  `DiegeticTextMut` holds three queries, ships `for_each_mut` (not `iter_mut`),
  and resolves the lone run by `line_index == 0`, not `PanelTextRuns::sole()`.
- **Step 16d — distinct marker types are required, not stylistic.**
  `DiegeticTextMut<M>` is type-keyed, so the two standalone texts need distinct
  markers (`WorldLabel`/`ScreenLabel`); the two panels address by `PanelElementId`.
- **Step 17 — `iter_mut` test struck (already shipped as `for_each_mut`).** Added
  the wrapped-label resolution test through `DiegeticTextMut`/`sole_text` (a new
  untested path — all existing tests are single-line). Specified the no-op
  reconcile probe (settle, then force a `ComputedDiegeticPanel` change without a
  run spawn/despawn, assert `Changed<PanelTextRuns>` is false) since a literal
  no-op never runs the reuse branch and the spawn pass would false-positive.
- **Step 18 — `pausing` re-bucketed** from per-frame-`set_text` to the tree-swap
  churn group (it mutates via `set_tree`); stale `PanelText` → `DiegeticTextMut`;
  added the precondition that the perf gate waits on 16c landing.

### Phase 5 — examples migration (rewritten on the relationship)
**Status: ✅ complete.**

16. **Three tasks:**

    **16a (required, ✅ done) — un-break the example targets.** Phase 1's
    `build() -> Result<_, PanelBuildError>` (was `InvalidSize`) is already
    resolved across both crates — `cargo build --workspace --examples` is green.
    No `InvalidSize` references remain in any example. Kept as the record; no work
    left. (Phase 3's retrospective predates this fix landing.)

    **16b (✅ done) — `aa_text` status panel → named runs.** `cube_status_panel`'s
    three fixed rows are now `text_id`-named (`STATUS_FIELD_{MSAA,OIT,POST}`) and
    `refresh_cube_status_panels` retexts them via `PanelText::set_text` instead of
    a full `set_tree` rebuild. This is the named multi-run case; it stays as-is.
    Structural panels where a run appears/disappears
    (`refresh_cube_compatibility_panels`, message is `Option`) keep `set_tree` —
    `set_text` can neither add nor remove a run.

    **16c (required) — cube-face labels → the relationship path.** Three example
    sites mutate cube-face `DiegeticText` labels via a `&mut TextContent` query
    that no longer matches (marker on the panel entity, `TextContent` on the run
    child):
    - `orthographic.rs` — `Query<&mut TextContent, With<CubeFaceLabel>>`; every
      face gets the **same** string → `DiegeticTextMut<CubeFaceLabel>::set(text)`.
    - `input_keyboard.rs:192` — `Query<(&KeyboardFaceLabel, &mut TextContent)>`;
      **per-face** strings keyed on the `KeyboardFaceLabel` enum (Orbit/Pan/Zoom)
      → `DiegeticTextMut<KeyboardFaceLabel>::for_each_mut(|kind, content| …)`.
    - `input_manual.rs:277` — `Query<(&ManualFaceLabel, &mut TextContent)>`,
      structurally identical to `input_keyboard` →
      `DiegeticTextMut<ManualFaceLabel>::for_each_mut(…)`. **This third site was
      omitted from earlier drafts** (the migration inventory wrongly called it a
      "keeps working" site); it is broken by D7 exactly like the other two.

    Use `for_each_mut` (not the planned `iter_mut`) for the per-face cases — it
    yields the marker `&M`, which is where the face identity lives. Note the
    identity is on the **example-local enum** (`KeyboardFaceLabel`/`ManualFaceLabel`),
    *not* on `fairy_dust::CubeFaceLabel` (a unit struct with no identity — only
    `orthographic`'s uniform `set` keys on it). Rewrite the stale `CubeFaceLabel`
    doc (`fairy_dust/primitive.rs`, still names the removed `WorldText` and shows
    the now-broken `Query<&mut WorldText, With<CubeFaceLabel>>`): name
    `DiegeticText`, and show a **`DiegeticTextMut<CubeFaceLabel>::set`** call as the
    doc example (never the bare two-query `sole()` form — it is wrong for a wrapped
    label, see step 15 *As built*), so a copy-paste compiles, runs, and is
    wrapped-safe.
    *Audit precision.* The marker sits in the query *tuple*
    (`(&KeyboardFaceLabel, &mut TextContent)`), not a `With<>` filter, so a
    `&mut TextContent.*With<` grep finds only `orthographic`. Audit with a bare
    `rg -n '&mut TextContent'` across `bevy_lagrange`/`fairy_dust` **and**
    cross-check every `cube_face_label`/`cube_face_text`/`DiegeticText` spawn; list
    the migrated sites with line numbers in the PR so coverage is explicit.

    **16d (required) — canonical four-flavor mutation example.** Add a new
    `crates/fairy_dust/examples/diegetic_mutation.rs` (fairy_dust has no `examples/`
    dir yet) whose sole purpose is to make runtime text mutation unambiguous across
    all four diegetic flavors: `DiegeticText::world` (world-space text),
    `DiegeticText::screen` (screen-space text), `DiegeticPanel::world` (world-space
    panel), `DiegeticPanel::screen` (screen-space panel). Each gets a distinct
    marker and a system that retexts it every frame (e.g. a ticking counter), so a
    reader sees the exact call for each case side by side:
    - **The two standalone texts** mutate through `DiegeticTextMut<M>` (step 15) —
      `labels.set(format!("world {n}"))` — proving the one-param/one-call path is the
      same whether the text is world- or screen-space (only the constructor differs).
      The two texts **must use distinct marker types** (e.g. `WorldLabel`,
      `ScreenLabel`): `DiegeticTextMut<M>` is keyed by one marker type, so a shared
      marker's single `set` would rewrite both with one string and defeat the
      side-by-side demo. The two *panels* address by `PanelElementId`, not a marker,
      so they need no distinct marker types.
    - **The two panels** carry a named field (`PanelElementId`) and mutate through
      `PanelText::set_text(panel, &id, …)` — the named-run path — proving the panel
      case uses the id-addressed SystemParam, not `DiegeticTextMut`.
    This makes the step-15 "which API when" split concrete and runnable: marker →
    `DiegeticTextMut<M>`; named field on a panel → `PanelText`. Organize the file
    per `/apply_example_layout`: module doc names `DiegeticTextMut<M>` and
    `PanelText`/`PanelElementId` as the demonstrated API; `main()`; a primary
    banner-section holding the four spawns + four mutation systems (lead with a
    short "How it works" paragraph naming the order they fire); camera/scene
    scaffolding last. Gate it in Phase 6 (build + clippy + fmt like every example).

    **Crate boundary:** build/verify `bevy_diegetic` and `bevy_lagrange`
    separately (sibling crates, own `Cargo.toml`s).

### Retrospective

**What worked:**
- All three 16c sites converted exactly as planned: `orthographic.rs` → `DiegeticTextMut<CubeFaceLabel>::set` (uniform); `input_keyboard.rs` + `input_manual.rs` → `for_each_mut(|kind, label| …)` (per-face). The `for_each_mut` closure yielding `&M` mapped one-to-one onto the existing `match kind { … }` body — the loop became a closure with no logic change.
- The 16d four-flavor example built first try once the cast was fixed; the `DiegeticTextMut<M>` (marker) vs `PanelText::set_text` + `PanelElementId` (id) split read cleanly side by side, which was the example's whole purpose.

**What deviated from the plan:**
- `time.elapsed_secs() as u64` tripped `clippy::cast_possible_truncation` + `cast_sign_loss` (workspace denies pedantic). Switched to `time.elapsed().as_secs()` (Duration → `u64`, no cast). The new example needs no `#[allow]`.
- 16c named one stale-`WorldText` doc site (the `CubeFaceLabel` marker). `cargo doc -D warnings` surfaced **three more** broken `[`WorldText`]` intra-doc links for the same labels: `Face` doc and `cube_face_text` doc (`src/primitive.rs`), and `PrimitiveBuilder::face_text` (`builder/primitive.rs:75`). All four fixed. Also updated the three migrated examples' `CUBE FACE LABELS` banner comments (`WorldText` → world-space `DiegeticText`) for in-file consistency.

**Surprises:**
- A standalone `DiegeticText` panel carries its marker **and** `PanelTextRuns` on the same (panel-root) entity, so `DiegeticTextMut<M>`'s `Query<(&M, &PanelTextRuns)>` matches directly — no child indirection in the example. Confirmed the Phase 4 design end-to-end through real call sites.
- One pre-existing broken doc-link remains out of scope: `builder/primitive.rs:55` links `SprinkleBuilder::with_camera_control_panel_background_color`, unrelated to text. `cargo doc -D warnings` is **not** a stated Phase 6 gate; it only surfaced here because I ran it to verify my doc edits.

**Implications for remaining phases:**
- Phase 6 step 18's 16c precondition is satisfied: `orthographic` (uniform `set`), `input_keyboard` + `input_manual` (`for_each_mut`) all profile the `DiegeticTextMut` write path now. The perf gate is unblocked.
- Phase 6 now also has a fourth `DiegeticTextMut`/`PanelText` call site to profile if desired: `fairy_dust/examples/diegetic_mutation.rs` (per-second, gated on a `Tick` resource — not per-frame, so a light load, but it exercises all four flavors).

### Phase 5 Review

- **Step 18 perf gate re-scoped (findings 1, 2, 8).** Corrected the "per-frame-`DiegeticTextMut` cube-face examples" framing: `orthographic` mutates only on keypress and `input_keyboard`/`input_manual` relayout only on string change, so their relayout load is on-input, not per-frame. Named `diegetic_mutation.rs` (16d) the canonical write-path subject and collapsed the now-satisfied 16c precondition to one line.
- **Step 17 gained `--examples` (per crate) and a `cargo doc -D warnings` gate (findings 10, 4).** Plain `cargo build` skips examples, so `fairy_dust`'s new `examples/` dir would never be gated; the rename churn makes intra-doc-link rot a live defect class. Noted the one pre-existing out-of-scope broken link (`builder/primitive.rs` → `SprinkleBuilder::…`) that must be fixed for the doc gate to pass.
- **Step 17 test list sharpened (findings 3, 6, 9).** The multi-run/zero-run `sole()` test must name the access-layer `sole_run_entity` (`line_index == 0`) vs raw `PanelTextRuns::sole`; added a TR-L assertion that an unchanged-string `set_text` re-invokes no `MeasureTextFn`; noted that `PanelElementId` duplicate detection is panel-local, with `diegetic_mutation.rs`'s shared `"counter"` id across two panels as the proof no test may assert global id uniqueness.
- **No-change confirmations (findings 5, 7):** the wrapped-label resolution tests (step 17) are genuinely absent and correctly scoped; the `text.as_str()`-in-loop borrow pattern in the panel mutators is correct as shipped and needs no test.

### Phase 6 — verify

**Status: ✅ complete (step 17 relationship tests + test-encodable step 18
invariants). Empirical frame-time profiling moved to Phase 7 (author chose to
build the stress subjects rather than defer).**

17. `cargo build --examples && cargo +nightly fmt`, `/clippy` — clippy is a
    **first-class gate**, not implied by `build`: pedantic caught latent Phase 0/1
    debt that a plain build passed (see the Phase 2 retrospective).
    - **`--examples` is required, per crate.** Plain `cargo build` skips examples;
      gate `bevy_diegetic`, `bevy_lagrange`, **and** `fairy_dust` separately with
      `--examples` (crate boundary, step 16's note). `fairy_dust` gained its first
      `examples/` dir in 16d (`diegetic_mutation.rs`) — without `--examples` on
      `fairy_dust` that example is never compiled and 16d's "gate it in Phase 6"
      silently no-ops.
    - **In-scope doc-link gate: no broken `WorldText`/`PanelTextChild` links.**
      The whole plan renamed `WorldText` → `DiegeticText` and deleted
      `PanelTextChild`, so rename-induced intra-doc-link rot is this plan's defect
      class — Phase 5 caught and fixed four broken `[`WorldText`]` links
      (`Face` doc, `cube_face_text`, `PrimitiveBuilder::face_text`, plus the 16c
      one). **Phase 6 verified this class is now clean: zero leftover
      `WorldText`/`PanelTextChild` intra-doc links remain.** The gate's intent is
      satisfied.
      *Re-scoped during the Phase 6 review (was "`cargo doc -D warnings` per
      crate"):* a full `cargo doc -D warnings` is **not** achievable in this
      plan's scope — it surfaces 33 pre-existing broken links across
      `bevy_diegetic` + `fairy_dust` (`TextConfig`, `LineMetricsSnapshot`,
      `ShapedGlyph`, `CameraGuidance`, `LayoutEngine`, `Mm`, `Fit`, and a dozen
      private-item links: `Element`, the `World`/`Screen`/`NeedsSize`/`HasSize`/
      `Ready` typestate markers, `Resolved`, `ensure_plugin`). None come from the
      rename; they are unrelated doc debt from other refactors. The
      `with_camera_control_panel_background_color` link the Phase-5 review named no
      longer exists (renamed to `SprinkleBuilder::n`). Spin the 33-link cleanup out
      as a standalone doc-hygiene task — do not hold this plan's close on it.
    *Relationship tests already shipped in Phase 4 (`access.rs`, do not
    re-author):* `PanelTextRuns` populates on run spawn and `sole()` returns the
    lone run for a single-line `DiegeticText`
    (`panel_text_runs_populates_and_sole_returns_the_lone_run`); a
    `DiegeticTextMut<M>::set` call retexts a marked label end-to-end
    (`diegetic_text_mut_set_retexts_a_marked_label`); `for_each_mut` (the shipped
    per-label method — **not** the planned `iter_mut`) updates two marked labels to
    different strings (`diegetic_text_mut_for_each_mut_sets_per_label_strings`).
    *Relationship tests — ✅ all six shipped in Phase 6 (green):*
    - **`sole()` is `None` for a multi-run panel** (two distinct elements) and for
      a zero-run panel — the count-based contract. Targets the **access-layer**
      `sole_run_entity` (the `line_index == 0` filter), not just
      `PanelTextRuns::sole` (raw slice count): the two diverge on wrapped runs.
      → `sole_resolution_is_none_for_a_multi_run_panel` +
      `sole_resolution_is_none_for_a_zero_run_panel` (`access.rs`). Surprise: a
      zero-run panel carries **no** `PanelTextRuns` component at all (the target
      only materializes when a `TextRunOf` source points at it), so the assertion
      is "component absent → `None`", not "empty set".
    - **Sync skips `MeasureTextFn` for an unchanged cached string (TR-L).** The
      `Changed<TextContent> → El.text` sync (step 8) must not re-measure an
      unchanged string. → `an_unchanged_set_text_fires_no_measure` (`reconcile.rs`):
      a measurer wrapping an `Arc<AtomicUsize>` proves a byte-identical `set_text`
      fires `MeasureTextFn` zero more times.
    - **Wrapped-label resolution through `DiegeticTextMut`/`sole_text`.** A
      *wrapped* `DiegeticText` materializes as one run entity per line, so its
      `PanelTextRuns` set holds >1 entity; `DiegeticTextMut::set` /
      `PanelTextReader::sole_text` still resolve the `line_index == 0` entity and
      relayout. → `a_wrapped_label_resolves_through_sole_text_and_diegetic_text_mut`
      (`access.rs`), using `TextWrap::Newlines` + an explicit `\n` for a
      deterministic multi-line run.
    - **Panel despawn drops all runs, no panic / no double-despawn** — proves
      `ChildOf` `linked_spawn` is the sole despawn path and `PanelTextRuns` adds
      none. → `panel_despawn_drops_all_runs_without_double_despawn` (`reconcile.rs`).
    - **Two no-op reconcile passes mutate `PanelTextRuns` zero times** — the
      per-frame-free invariant (step 18). Settle, then force a
      `ComputedDiegeticPanel` change via a visual-only recolor `set_tree` twice and
      assert `Changed<PanelTextRuns>` is false across both. →
      `two_no_op_reconcile_passes_leave_panel_text_runs_unchanged` (`reconcile.rs`).
    - **`set_tree` empties the run set and reconcile repopulates it** next pass;
      O(1) named `text_child` lookup is unchanged on a multi-run panel. →
      `set_tree_empties_the_run_set_then_reconcile_repopulates_it` (`reconcile.rs`).
    *Already shipped in Phase 3 (`access.rs` tests, do not re-author):*
    `text_child(id)` resolves a named run
    (`reader_resolves_a_named_run_and_reads_its_text`); an auto-id'd run is not
    addressable (`auto_id_run_is_not_addressable_but_sole_text_reads_it`); an
    unknown id resolves to `None` (`unknown_id_resolves_to_none`); mutating a run's
    `TextContent` relayouts, the property D2 buys
    (`set_text_through_panel_text_relayouts`); the orphan/liveness case **through
    the SystemParam** (`orphaned_run_resolves_to_none_through_the_system_param`);
    `set_sole_text` retexts a one-element panel
    (`set_sole_text_retexts_a_one_element_panel`).
    *Status corrected during the Phase 6 review — two of these already shipped,
    three remain genuinely open:*
    - ✅ **Duplicate explicit ids error at build** — already shipped (not by Phase
      6): `duplicate_named_text_ids_error_at_build`,
      `text_id_colliding_with_editable_field_errors` (TR-O), and
      `many_unnamed_runs_never_collide` (`panel/builder.rs:806/835/851`). The check
      is panel-local per DT3 (`diegetic_mutation.rs` shares `"counter"` across its
      world and screen panels), so no test asserts global id uniqueness.
    - ✅ **`set_tree` clears stale index entries** — covered by the repopulate half
      of `set_tree_empties_the_run_set_then_reconcile_repopulates_it`, which
      asserts `text_child` resolves O(1) on the rebuilt index.
    - ⬜ **A reorder keeps named runs and respawns auto runs (TR-D)** — still open.
      `reconcile_keys_by_run_id_and_line_index` only asserts a HashMap built from
      *synthetic* `PanelTextLayout`s distinguishes `(id, line_index)` from id-alone;
      no test does a sibling-reordering `set_tree` and asserts a named run keeps its
      `Entity` while an auto run respawns. TR-D's actual acceptance criterion.
    - ⬜ **A wrapped multi-line run edited via the *named* `PanelText::set_text`** —
      still open. The wrapped path is now covered via `DiegeticTextMut`/`sole_text`,
      but the id-addressed write on a wrapped run (extend
      `set_text_through_panel_text_relayouts` with a wrapping width; assert the full
      new string relayouts, no line dropped — the line-0-index edge, step 11) is
      untested.
    - ⬜ **Single-pass `ComputedDiegeticPanel` change** — still open. No test counts
      `Changed<ComputedDiegeticPanel>` to assert a `set_text` edit fires exactly one
      relayout pass (the `ReconcileOwned` / DTX-2 gate). Adjacent
      (`reconcile_owned_marker_gates_then_clears`, `an_unchanged_set_text_fires_no_measure`)
      but neither asserts the one-pass count.
18. **Perf gate with criteria (TR-C, TR-L, TR-M).** *Precondition met (16c
    landed):* `orthographic` (uniform `set`), `input_keyboard` + `input_manual`
    (`for_each_mut`) all run the `DiegeticTextMut` write path now, so the gate
    profiles it directly. Target < 16.7 ms/frame release; flag > 5% over a `main`
    baseline.
    **Pick the workload that actually exercises the write path.** The three
    cube-face examples are weaker subjects than the plan first assumed: `orthographic`
    mutates only on an O/P keypress (`switch_projection`), and
    `input_keyboard`/`input_manual` iterate `for_each_mut` every frame but call
    `set_text` only on an actual string change (`if label.text() != next`) — so the
    **relayout** (`Changed<TextContent>`) fires on input, not per frame; only the
    cheap `(&M, &PanelTextRuns)` iteration is per-frame. The canonical write-path
    subject is **`diegetic_mutation.rs`** (16d): it drives all four flavors and both
    APIs (`DiegeticTextMut<M>::set` and `PanelText::set_text`) on a fixed cadence,
    and its per-second `Tick` gate converts trivially to per-frame for a stress run.
    Profile it first, then the input examples as the realistic on-change load — not
    only the static `cascade`/`paper_sizes`/`world_text` panels. `pausing` belongs in
    the **tree-swap churn** bucket below, not here: it mutates via `set_tree`
    (despawn/respawn every run), not `DiegeticTextMut`/`set_text`. Add a resize
    pass on a complex-font panel (the known freeze path,
    `project_diegetic_panel_freeze.md`, which DTX-2's double-layout would amplify
    if the `ReconcileOwned` gate regressed). Criterion restated for DT1=(b)
    (TR-L): there is no OQ1(a) gather — the `TextContent → El.text` sync (step 8)
    must be **O(n_changed)** (driven off `Changed<TextContent>`, never a full
    `n_elements` walk) and must not re-invoke `MeasureTextFn` for an unchanged
    cached string; target < 0.5 ms on the 100-label panels. Confirm the
    relationship adds no per-frame cost — `PanelTextRuns` mutates only on run
    spawn/despawn, not on a layout pass — by asserting two consecutive no-op
    reconcile passes mutate it zero times (step 17), which holds only if the reuse
    branch skips re-inserting `TextRunOf` (step 13). Add a `set_tree`-on-a-complex-font-panel
    edit to the resize scenario, not only per-frame `set_text`: a tree swap
    despawns every run and respawns it, the heaviest relationship-churn path and an
    amplifier for the known freeze. Regression fallback: the `unify_text.md`
    D1(c) lightweight single-element path.

### Retrospective

**What worked:**
- All six step-17 relationship bullets landed as seven tests, green on the first
  full run: three in `access.rs`
  (`sole_resolution_is_none_for_a_multi_run_panel`,
  `sole_resolution_is_none_for_a_zero_run_panel`,
  `a_wrapped_label_resolves_through_sole_text_and_diegetic_text_mut`) and four in
  `reconcile.rs` (`two_no_op_reconcile_passes_leave_panel_text_runs_unchanged`,
  `set_tree_empties_the_run_set_then_reconcile_repopulates_it`,
  `panel_despawn_drops_all_runs_without_double_despawn`,
  `an_unchanged_set_text_fires_no_measure`).
- `TextWrap::Newlines` + an explicit `\n` string is a deterministic way to force
  a multi-line run (one entity per line, shared id) without depending on
  measurer width math — it cleanly separates the count-based `PanelTextRuns::sole`
  (`None` on >1 entity) from the access-layer `sole_run_entity` (`line_index == 0`
  filter still resolves).
- The TR-L "no re-measure on an unchanged string" criterion converted directly
  into a test: a measurer wrapping an `Arc<AtomicUsize>` counter proves a
  byte-identical `set_text` fires `MeasureTextFn` zero more times, because
  `sync_run_text_to_cache` compares before writing and never dirties
  `DiegeticPanel`.

**What deviated from the plan:**
- The plan's step-17 doc gate said only one pre-existing `SprinkleBuilder`
  intra-doc link needed fixing for `cargo doc -D warnings` to pass. The named
  link (`with_camera_control_panel_background_color`) no longer exists (renamed to
  `SprinkleBuilder::n`), and the gate actually surfaces **33** broken doc links
  across `bevy_diegetic` + `fairy_dust` — `TextConfig`, `LineMetricsSnapshot`,
  `ShapedGlyph`, `CameraGuidance`, `LayoutEngine`, `Mm`, `Fit`, and a dozen
  private-item links (`Element`, the `World`/`Screen`/`NeedsSize`/`HasSize`/`Ready`
  typestate markers, `Resolved`, `ensure_plugin`). None come from this plan's
  `WorldText` → `DiegeticText` rename — that defect class is **clean** (zero
  leftover `WorldText`/`PanelTextChild` links). The 33 are pre-existing rot from
  other refactors, outside this plan's scope.
- Fixed one release-only defect the gate exposed that *is* in scope: `cargo build
  --release` warned `contains_text_id` is never used, because its sole caller is
  the `#[cfg(debug_assertions)]` typo-warn path in `PanelTextReader::resolve`.
  Added `#[cfg_attr(not(debug_assertions), expect(dead_code))]` so both profiles
  are warning-clean. Auto-applied (determined fix, no behavior change).

**Surprises:**
- A panel with no text run gains **no** `PanelTextRuns` component at all (the
  relationship target only materializes when a `TextRunOf` source points at it),
  so the zero-run `sole` path is "component absent → `None`", not "empty set →
  `None`". The test asserts the absence directly.
- The empirical half of step 18 is not runnable as written: the < 0.5 ms
  criterion targets "the 100-label panels," but no example spawns 100 labels —
  `diegetic_mutation.rs` drives four. A real frame-time gate needs (a) a
  100-label stress scenario that does not exist, and (b) a `main`-baseline
  harness for the > 5%-regression flag. The *structural* invariants behind the
  perf gate (O(n_changed) sync with no re-measure; `PanelTextRuns` untouched on a
  reuse-only pass) are now test-locked, but the wall-clock numbers are not
  measured.

**Implications for remaining phases:**
- Phase 6 is the last phase; nothing downstream depends on it. The open item is
  internal to step 18 (empirical profiling), not a later phase. Whether to close
  the plan with that item noted-as-deferred or to add a Phase 7 (build the
  100-label scenario + baseline harness, then profile) is the one decision the
  review must settle.

### Phase 6 Review

A `Plan` subagent re-evaluated the plan against the seven shipped tests
(seven findings; six applied straight in, one surfaced to the author).

- **Step-17 first list → ✅ closed.** All six relationship-test bullets map
  one-to-one onto shipped green tests; marked done in place with the test names.
- **Step-17 second list → status corrected.** Two items already shipped
  (duplicate-id-at-build via `panel/builder.rs:806/835/851`; `set_tree` clears
  stale index entries via the repopulate test) and were mislabeled "still to
  write"; three are genuinely open and now flagged ⬜: TR-D reorder end-to-end,
  the *named* `PanelText::set_text` wrapped-run write, and the single-pass
  `Changed<ComputedDiegeticPanel>` assertion.
- **Doc-link gate → re-scoped.** Narrowed from "`cargo doc -D warnings` per crate"
  to "no broken `WorldText`/`PanelTextChild` links" (verified clean). The full
  gate surfaces 33 pre-existing out-of-scope broken links; spun out as a separate
  doc-hygiene task rather than holding this plan's close.
- **`contains_text_id` release dead-code fix → recorded.** The verify pass found
  and fixed a release-only `never used` warning (sole caller is the
  `#[cfg(debug_assertions)]` typo-warn); `#[cfg_attr(not(debug_assertions),
  expect(dead_code))]` keeps both profiles warning-clean. In-scope (DT6-ii path).
- **DT4-ii prose noted as superseded.** DT4-ii's decided wording promised a
  `diegetic_text.text()` / `.set_text(…)` helper *on the spawned marker*; what
  shipped is the `DiegeticTextMut<M>` SystemParam + `TextContent::text()/set_text()`
  (Phase 4), which supersedes it and is strictly better. The DT4-ii sentence is
  historical record; the shipped surface is `DiegeticTextMut` (step 15). No code
  change.
- **No new architectural risk.** TR-Q / TR-L / TR-K are test-locked; the one new
  behavioral fact (zero-run panel carries no `PanelTextRuns`) is handled
  correctly.
- **Surfaced to author (the one open decision):** the empirical step-18 perf gate
  — close-and-defer (recommended) vs add a Phase 7. **→ Author chose Phase 7:**
  build the missing stress subjects and run the profile (see Phase 7). The
  re-scoped doc-link gate is also reversed — the author wants the full
  `cargo doc -D warnings` clean, so the 33-link cleanup is in-scope under Phase 7,
  not spun out.

### Phase 7 — empirical perf gate + doc-link cleanup

**Status: ✅ complete.** Both stress subjects built; doc-link debt cleared (full
`cargo doc -D warnings` passes on all three crates); `diegetic_text_stress`
profiled in release (results under step 20). The three open Phase-6 step-17
acceptance tests were also written and are green (see "Phase 6 step-17 close"
below).

The Phase 6 review surfaced two items the author pulled back into scope rather
than defer: the empirical frame-time profile (no 100-label subject existed) and
the full `cargo doc -D warnings` gate (33 pre-existing broken links). Phase 7
builds the missing subjects, runs the profile, and clears the doc debt.

19. **Two named stress subjects, one per write axis.**
    - **`diegetic_text_stress` (new, `bevy_diegetic/examples/`)** — the write-path
      subject the step-18 gate needs. A 10×10 grid (`LABEL_COUNT = 100`) of
      standalone `DiegeticText::world` labels, each carrying a `StressLabel(index)`
      marker, all retext every frame through `DiegeticTextMut::for_each_mut` — the
      worst-case `O(n_changed)` load (all 100 `Changed<TextContent>` per frame).
      `Space` pauses mutation so the moving-vs-idle delta is directly visible. A
      bottom-left overlay reports fps / frame-ms / `DiegeticPerfStats.compute_ms`
      (layout) / `panel_text.total_ms` (text) with a 5-second peak column.
    - **`diegetic_panel_stress` (renamed from `text_stress`)** — the tree-churn /
      `set_tree` subject (panels grow by rebuilding the active panel's tree). The
      rename disambiguates the two axes named in step 18: `diegetic_text_stress` =
      per-frame `DiegeticTextMut` write path; `diegetic_panel_stress` = panel
      tree-build churn. `text_stress` had no explicit `Cargo.toml` entry (examples
      auto-discover), so the rename is the file move plus the in-file title /
      module-doc / error-string updates.
20. **Run the profile against the step-18 criteria.** Release build, capture
    `diegetic_text_stress` at 100 labels moving vs paused, then `diegetic_panel_stress`
    under row growth. Criteria (from step 18): < 16.7 ms/frame release; the
    `TextContent → El.text` sync < 0.5 ms at 100 labels and O(n_changed) (the
    `text` sub-timing, not a full `n_elements` walk); flag > 5% over a `main`
    baseline. Add the complex-font resize pass (the `project_diegetic_panel_freeze.md`
    path). The structural invariants behind these are already test-locked in Phase 6
    (`an_unchanged_set_text_fires_no_measure`,
    `two_no_op_reconcile_passes_leave_panel_text_runs_unchanged`); this step is the
    wall-clock confirmation.
21. **Clear the `cargo doc -D warnings` debt, per crate.** Resolve all 33
    pre-existing broken intra-doc links across `bevy_diegetic` + `fairy_dust`
    (`bevy_lagrange` is already clean) so the full gate passes — links to private
    items are delinked (kept as inline code), renamed/moved types are repathed to
    their current names. This promotes the Phase-6 in-scope-only doc gate to the
    full `cargo doc -D warnings` the author asked for.

#### Step 20 — measured (release, `diegetic_text_stress`, 100 labels)

Captured via BRP from the running release example (`DiegeticPerfStats` resource +
frame diagnostics), moving state (all 100 labels retext every frame):

| metric | value |
| --- | --- |
| frame time | ~49 ms (~20 fps) |
| layout (`compute_ms`, 100 panels) | 5.82 ms |
| text (`panel_text.total_ms`) | 1.86 ms (`shape_ms` 0.36 / `parley_ms` 0.05 / `mesh_build_ms` 1.50) |
| diegetic per-frame total | ≈ 7.7 ms |
| non-diegetic remainder | ≈ 41 ms |

**Reading.** The unify-text write path is **not** the frame-time bottleneck: at
the worst case (100 standalone labels, every one changing every frame) the
diegetic layout+text work is ≈ 7.7 ms. The ~41 ms remainder is non-diegetic scene
render — 100 world-space PBR-lit text meshes with shadows + stable transparency,
plus present. So the < 16.7 ms/frame target is **not** met by this scene, but the
overage is render/scene cost, not the text-mutation path. Each standalone
`DiegeticText` is its own one-element panel, so 100 labels = 100 panel layouts
(5.82 ms ≈ 58 µs/panel) — the expected `O(n_changed)` relayout, not a full-walk
regression. The O(n_changed) property itself is test-locked
(`an_unchanged_set_text_fires_no_measure`,
`two_no_op_reconcile_passes_leave_panel_text_runs_unchanged`).

**Not captured / N/A.**
- **Paused (idle) delta** — the app was shut down before the paused sample was
  read; rerun and press `Space` to confirm `compute_ms`/`text_ms` drop to ≈ 0 at
  idle (the visual confirmation of `O(n_changed)`).
- **`main` baseline (> 5% flag)** — **N/A.** The showcase branch has diverged
  drastically from `main` (in `../bevy_hana`), so a cross-branch frame-time
  comparison measures the divergence, not this change. Dropped from the gate.
- **Complex-font resize pass** — the freeze path
  (`project_diegetic_panel_freeze.md`) is guarded structurally by the single-pass
  test (`a_set_text_edit_fires_exactly_one_relayout_pass`, Phase-6 step-17 close)
  rather than re-measured here.

#### Phase 6 step-17 close (the three remaining acceptance tests)

The three step-17 criteria left open at the Phase 6 review are now written and
green:
- **TR-D — named survives, auto is positional:**
  `a_structural_edit_keeps_named_runs_but_repositions_auto_runs` (`reconcile.rs`).
  Auto ids come from a per-build counter over `text()` calls (`text_id` does not
  consume it), so inserting an auto sibling ahead of an existing one shifts its id
  and lands its text on a fresh entity, while the named run keeps its entity.
- **Named `PanelText::set_text` on a wrapped run:**
  `set_text_on_a_named_wrapped_run_replaces_the_whole_string` (`access.rs`) — a
  three-line replacement round-trips through the cache and re-wraps into three
  run entities, no line dropped.
- **Single-pass relayout (DTX-2 gate):**
  `a_set_text_edit_fires_exactly_one_relayout_pass` (`access.rs`) — a `Last`-schedule
  probe counts `Changed<ComputedDiegeticPanel>` and asserts exactly one across the
  frames following an edit.

## Risks

- **Per-frame `set_text` relayout cost** — many one-element panels each re-run the
  layout engine per edit (carried from `unify_text.md` R13/R21). DT1=(b) avoids a
  per-pass gather, but the cache sync + observer still run per edit. The
  `ReconcileOwned` gate (DTX-2=a, step 10) keeps it to one layout pass per edit
  instead of two. This is the primary residual risk; the Phase-5 perf gate
  (TR-C/TR-L/TR-M) is the guard, D1(c) the fallback.
- **`El.text` cache staleness (DT1-b).** The cache is only correct if the sync
  (step 8) runs before every layout that could see changed text. The observer
  (step 10) and the `.before(ApplyTreeChanges)` ordering close this; the relayout
  test in step 14 guards it.
- **`PanelTextChild` deletion breadth (DT4-iii).** Mechanical but touches every
  filter site; do it in Phase 0 before behavioral changes so later phases review
  against the final `With/Without<TextContent>` form.
- **Naming**: `DiegeticText` lands alongside the still-open `DiegeticPanel →
  Panel` rename (`unify_text.md` R15, out of scope here); the `Diegetic*` prefix
  asymmetry is accepted for now.

---

## Team review — cycle 1

Five lenses (architecture, correctness/completeness, Rust type-system, risk/
failure-modes, ergonomics) reviewed this doc against its stated intent
(strengthen posture). No premise-challenge survived the firewall: several agents
flagged OQ1/OQ2/index-staleness as blockers, but each supplied a working path, so
Scope B is achievable — they are recorded below as design forks, not challenges.

### Auto-recorded resolutions (converged, single in-intent outcome)

- **TR-A — id→Entity index lifecycle.** Reconcile **rebuilds the index from
  scratch** at the start of each run (O(n), no worse than today's key-building
  loop at `reconcile.rs:99`); `set_tree` clears it; stale entries are dropped when
  the child they map to is gone. `text_child(id)` must tolerate a despawned child
  (validate liveness / return `None`), so an out-of-flow `despawn` cannot hand
  back a dangling `Entity`. Add tests: resolve a named id; `None` for an orphaned
  entity; `set_tree` clears stale entries. (risk, architecture)
- **TR-B — read-only `PanelTextReader` + parallelism contract.** `PanelText`
  holds `Query<&mut TextContent>`, which serializes against any other
  `TextContent` accessor. Add a read-only `PanelTextReader` variant and document
  that one system should own `PanelText` writes per frame. (ergonomics)
- **TR-C — perf gate gets measurable criteria.** Phase 5 must set a concrete
  threshold (e.g. < 16.7 ms/frame release; flag > 5% over a `main` baseline) and
  profile the per-frame-`set_text` cube-face examples (`input_keyboard`,
  `orthographic`, `pausing`), not only the static demos. The resolve gather (OQ1)
  must be O(n_ids), not O(n_elements); target < 0.5 ms on the 100-label panels.
  Regression response: fall back to the `unify_text.md` D1(c) lightweight
  single-element path. (risk, ergonomics)
- **TR-D — auto-id framing + reorder test.** Document that id-based reuse gives
  content-stability only to **named** runs; auto-ids keep today's positional
  semantics (an auto run respawns on reorder, same as the current
  `(element_idx, command_index)` key). Add a reorder test and a "name it to keep
  identity across reorders" note. (correctness, type-system)
- **TR-E — auto-id counter width.** Use `u32` per-tree build-order counter;
  document that auto-ids are not stable across rebuilds and never relied on for
  persistence (named ids are the only persistence path). Overflow is unreachable
  in practice; revisit only if a tree rebuilds >2³² times. (risk)
- **TR-F — Commands write-extension (OQ6) deferred.** Ship only the `PanelText`
  SystemParam now; add `commands.set_panel_text(…)` when a consumer needs the
  deferred form. If shipped, document the one-frame latency vs the synchronous
  SystemParam. (type-system, risk)
- **TR-G — `DiegeticText` delegation doc.** Note that `DiegeticText` is a facade
  that builds and returns a one-element-panel value; sizing/scaling setters
  forward to the internal `DiegeticPanelBuilder`; `paper()`/`layout()` are
  intentionally absent; `Fit` height is enforced. (architecture)
- **TR-H — `text_style_setters!` macro contract.** Comment that the macro
  generates only context-free typography setters; context-specific setters
  (`world_height`, anchor, lighting/sidedness defaults) live on the builder.
  (type-system)
- **TR-I — migration framing.** Clarify in the migration inventory that auto-id
  is automatic and invisible; static labels need no change; runtime-mutable labels
  may keep the existing marker + `Query<&mut TextContent>` pattern **or** adopt
  `.id()` + `PanelText`. The new API is a convenience, not a forced refactor.
  (ergonomics, correctness)
- **TR-J — `DiegeticText::world(text)` consistency claim reframed.** The parallel
  to `DiegeticPanel::world()` (which takes no args) is loose: a panel is a
  container sized later; text is a filled value whose string is the one required
  input. Reframe D1's justification accordingly rather than claiming a 1:1
  mirror. (ergonomics)

### Proposed user decisions

Status legend: `proposed` = awaiting author choice.

- **DT1 — OQ1 resolve path. (critical, architecture/correctness/risk, proposed)**
  The engine is `Fn(&str, &TextMeasure)` over the tree; under Scope B the `El`
  holds no string. Pick the mechanism: **(a)** a pre-layout pass builds an
  `element_idx → &str` map (borrowed from `TextContent`) threaded into
  `compute()` — keeps the engine pure, team-preferred, adds a side-table param;
  **(b)** the `El` keeps a `text` field demoted to a layout cache synced from
  `TextContent` before each pass — smallest change, but reintroduces the second
  physical copy D2 set out to remove; **(c)** change `MeasureTextFn` to resolve
  ids itself. Couples to DT2's bootstrap.
  **→ DECIDED: (b).** Keep a derived `text` cache on the `El`, synced from
  `TextContent` before layout. The engine is untouched (read sites still do
  `Text { text }`); the only new machinery is one sync step. `TextContent` stays
  the logical single source — the `El` copy is a cache, not a rival store. The
  sync must run before every layout that could see changed text (the ordering
  `rebuild_fluent_text` already uses, `.before(ApplyTreeChanges)`), or the cache
  drifts. Makes DT2's first-frame bootstrap trivial (the string is in the tree at
  build time). Trade: avoids (a)'s gather pre-pass + threaded `compute()` param +
  three rewritten read sites, for the cost of a few strings of cached memory.

- **DT2 — OQ2 inversion: where text children are created + how a child edit
  relayouts. (critical, correctness/architecture, proposed)** Two coupled gaps.
  (i) **Bootstrap**: `LayoutBuilder` is pure data and `TextContent` is spawned by
  reconcile *after* layout, so the first layout has no child string to resolve
  (DT1). (ii) **No system observes `Changed<TextContent>` on a child** — reconcile
  runs on `Changed<ComputedDiegeticPanel>`, so the doc's "mutating a child
  `TextContent` relayouts" is currently unimplemented. Pick the model:
  **(a)** keep reconcile as the spawner, author the string into the tree at build
  for the first pass + resolve/cache thereafter, and add an observer on
  `Changed<TextContent, With<PanelTextChild>>` that dirties the parent
  `ComputedDiegeticPanel`; **(b)** authoring eagerly spawns each child + its
  `TextContent` and the `El` references it by id, reconcile only attaches render
  data — the literal "inversion," largest change, needs a spawn path out of the
  pure builder.
  **→ DECIDED: (a).** Reconcile stays the spawner and the pure builder is
  untouched. DT1=(b) already closes gap (i) (the string is in the `El` cache at
  build time). For gap (ii), add an observer on
  `Changed<TextContent, With<PanelTextChild>>` that writes the new string into the
  parent's `El` cache and dirties `ComputedDiegeticPanel`, re-triggering reconcile/
  relayout. The literal inversion (b) is dropped — moot once the `El` holds the
  cached string.

- **DT3 — `PanelElementId` representation (OQ3). (important, type-system, proposed)**
  Unanimous team rec: **enum `{ Named(String), Auto(u32) }`** — encodes the
  named-vs-auto invariant at the type level, `From<&str>` always yields `Named`,
  `Eq`/`Hash`/`Reflect` derive cleanly, editable-field call sites still compile.
  Alternative kept on the table because you said "PanelElementId is fine for now":
  the `String` newtype with a reserved auto-form (no public type change, but a
  runtime escape hatch). Also decide whether element ids and editable-field ids
  share one panel-local namespace (and one duplicate check) or stay separate.
  **→ DECIDED: enum + shared namespace.** `PanelElementId` becomes
  `enum { Named(String), Auto(u32) }`; `From<&str>`/`From<String>` always produce
  `Named`, so no `&str` can forge an `Auto`. Element ids and editable-field ids
  live in one panel-local id space with a single duplicate check and a single
  "address any element in a panel" lookup — consistent with the fewest-types lean.
  Auto ids are assigned from the per-tree build-order counter (TR-D/TR-E).

- **DT4 — `DiegeticText` space encoding + persistence (OQ4). (important,
  type-system/architecture, proposed)** (i) Encode world/screen as a **runtime
  `CoordinateSpace` field** (team-preferred — mirrors the spawned `DiegeticPanel`,
  keeps the builder chain free of type params) or as **typestate
  `DiegeticText<World/Screen>`** (compile-time rejection of `world_height` on
  screen text, at the cost of generic noise). (ii) Does `DiegeticText` persist as
  a live, mutable component, or is it pure build-time sugar with all mutation
  through `TextContent`-by-id? The session leaned pure-sugar.
  **→ DECIDED.**
  - **(i) runtime `CoordinateSpace` field.** `DiegeticText` records space as the
    existing `CoordinateSpace` enum (`coordinate_space.rs:48`), set by
    `::world`/`::screen`. The wrapped `DiegeticPanelBuilder` already enforces space
    at compile time, so type params on the facade would be redundant noise.
  - **(ii) marker component + single-string helper.** `DiegeticText` is a
    lightweight marker on the spawned text entity, so a single label is queryable
    via `With<DiegeticText>` and a user-named marker — no ids for the common case.
    A helper returns/sets the one-and-only string directly
    (`diegetic_text.text()` / `.set_text(…)`), hiding the plumbing.
  - **(iii) delete `PanelTextChild`.** Once D2 removes the panel-root
    `TextContent` copy, `TextContent` lives only on text-run entities, so
    `PanelTextChild` is redundant with it. Move `#[require(TextStyle, Transform,
    Visibility)]` onto `TextContent` (spawning a `TextContent` then yields a
    complete run), delete `PanelTextChild`, and swap every `With<PanelTextChild>`
    → `With<TextContent>` and `Without<PanelTextChild>` → `Without<TextContent>`
    (`shaping.rs:33` already filters `With<TextContent>`). A single
    `DiegeticText`'s text lives on its run entity, so the user marker lands there.

- **DT5 — runtime lookup handle: typed vs stringly. (important, type-system/
  ergonomics, proposed)** `.id("title")` at authoring + `text_child(&PanelElementId)`
  at runtime is a stringly reuse — a typo is a silent `None`. Options:
  **(a)** the `.id(...)` builder call returns the `PanelElementId` for the caller to
  hold and reuse; **(b)** authoring returns an opaque `TextId` handle (cannot be
  forged); **(c)** keep stringly lookup but make `text_child` return a helpful
  error for an unknown id ("did you forget `.id()`?"). Mirrors the existing
  editable-field handle pattern (`set_field_display_text(&field_id, …)`).
  **→ DECIDED: look up by `PanelElementId` (no new handle type).** A run is
  addressed by the same `PanelElementId` from DT3 — `text_child(&id)` — built from a
  string at the lookup site, consistent with the editable-field path and the
  fewest-types lean. No separate `TextId`. The cost is that a wrong id is a runtime
  miss, not a compile error; DT6 decides whether that miss is a quiet `None` or a
  loud error.

- **DT6 — error behavior for ids. (important, correctness/ergonomics, proposed)**
  The doc left "duplicate explicit ids → `debug_assert!` or `Result`" open. Pick:
  **`Result` at build** (forces handling, consistent with `build() -> Result`
  today, no silent release shadowing) vs **`debug_assert!`** (terser, silent in
  release). And the lookup side: `text_child(id) -> Option<Entity>` vs `-> Result`
  so callers can distinguish "no such id" from "child despawned" (ties to TR-A).
  **→ DECIDED.** (i) **`Result` at build** — duplicate explicit ids are a build
  error on the existing `build() -> Result`, so a duplicate can't silently shadow
  in release. (ii) **`text_child(id) -> Option<Entity>`** — idiomatic; liveness is
  validated inside (TR-A), so "no such id" and "child despawned" both return
  `None`. A debug-only `warn!` on miss surfaces typos without changing the type.

---

## Team review — cycle 2 (mechanical, auto-recorded)

A second team pass (correctness, architecture, type-system, risk, ergonomics)
against the stated intent. The lenses that read the doc as already-built code and
reported "Phase N not implemented" were filtered out — an unbuilt phase is the
plan, not a defect. The findings below are in-intent clarifications with one
sensible outcome; they refine the plan text, not its structure.

- **TR-K — `Auto` variant must be unforgeable.** `PanelElementId::Auto(u32)`
  (DT3) must not be publicly constructible, or in-crate/external code could mint
  an `Auto(0)` that collides with the builder counter. Make the variant private
  (module-private constructor) or `#[doc(hidden)]` with a private tuple field;
  `From<&str>`/`From<String>` stay the only public path and always yield `Named`.
  Add a test asserting no public API produces `Auto`. (type-system, risk)
- **TR-L — perf-gate wording fixed for DT1=(b).** TR-C's "the resolve gather
  (OQ1) must be O(n_ids), not O(n_elements)" is a holdover from the rejected
  DT1=(a) pre-pass. Under DT1=(b) there is no gather — there is a
  `TextContent → El.text` sync. Restate the criterion: the sync is
  **O(n_changed)** (drive it off `Changed<TextContent>`, never a full
  `n_elements` walk) and **must not re-invoke `MeasureTextFn` for an unchanged
  cached string**. Keep the < 0.5 ms / 100-label target against that. (risk,
  ergonomics)
- **TR-M — perf gate includes a resize pass on complex fonts.** The known
  resize/complex-font freeze (`project_diegetic_panel_freeze.md`) is amplified if
  a `set_text` edit now triggers two layout passes (see DTX-2). Phase 5's gate
  must add a resize test on a complex-font panel, not only per-frame `set_text`.
  (risk)
- **TR-N — auto-id counter is per-build and resets each build.** Document that
  the per-tree `u32` counter (TR-E) restarts from 0 on every build/`set_tree`, so
  an auto run is `Auto(k)` only within one build and always respawns on reorder —
  the positional semantics TR-D promises. Never persist or cross-panel-compare
  auto ids. (risk, correctness)
- **TR-O — one duplicate-id check spans both id kinds.** DT3's shared namespace
  means `build()`'s duplicate check must collect element ids **and**
  editable-field ids into one panel-local set before erroring — not two separate
  checks. Name this explicitly where Phase 1 step 6 reuses the editable-field
  duplicate path. (type-system, correctness)
- **TR-P — `coordinate_space` is read-only post-spawn.** DT4(i)'s runtime
  `CoordinateSpace` field is safe only because `DiegeticText` is build-time sugar
  (DT4-ii) with no public space setter; the wrapped builder enforces space during
  authoring. Document that the spawned marker exposes no mutation of space, and
  space-specific setters (`world_height`) live on the builder, never on a spawned
  `DiegeticText`. This closes the "field drifts from the panel's space" footgun
  without typestate. (type-system, architecture)
- **TR-Q — `text_child` liveness is an explicit step, not an assumption.** TR-A
  says the lookup tolerates a despawned child; the implementation must actually
  check entity liveness (verify the child is still in the panel's children / still
  exists) before returning `Some`, so an out-of-flow `despawn` cannot hand back a
  dangling `Entity`. Call this out at Phase 3 step 11. (risk, type-system)
- **TR-R — TR-G facade-delegation doc names the surfaced setters.** Spell out, on
  the builder, which setters the facade exposes (typography via
  `text_style_setters!`, plus world size / anchor / position forwarding to the
  internal `DiegeticPanelBuilder`) and which are intentionally absent
  (`paper`/`layout`/full panel API), so a reader can tell "forbidden" from "not
  yet added." (architecture, type-system)

## User decisions — cycle 2 review (resolved)

Status: all three **DECIDED** by author (each took the team-preferred option). No
premise-challenge survived — the typestate-vs-runtime challenge was already weighed
and decided in DT4, and runtime validation per TR-P closes the safety gap.

> **DTX-1 → (a)** move the panel-root `TextContent` seed removal into Phase 0,
> before the filter swap. Folds Phase 2 step 9 into Phase 0; step 9 becomes
> degenerate.
> **DTX-2 → (a)** `ReconcileOwned` marker: reconcile inserts a `#[doc(hidden)]`
> marker on its own `TextContent` write, the observer filters
> `Without<ReconcileOwned>`, cleared next frame.
> **DTX-3 → (a)** bind the id as a value and pass it in
> (`let id = PanelElementId::named("title"); El::text(..).id(id.clone());
> panel.text_child(&id)`), mirroring `editable_field`'s arg-passed id.

- **DTX-1 — Phase 0 filter swap collides with fluent panel roots.
  (critical, risk/architecture/correctness, proposed)** The original Phase 0 plan
  deleted `PanelTextChild` and swapped every `With<PanelTextChild>` →
  `With<TextContent>` in one step. But the fluent sugar seeds `TextContent` on the
  **panel root** entity (`fluent.rs:328`, `:441`), and that seed was not removed
  until **Phase 2 step 9**.
  So from the end of Phase 0 through the start of Phase 2, a `With<TextContent>`
  child query (shaping, reconcile, render) also matches one-element fluent panel
  roots — shaping/positioning the root as if it were a run.
  *Cycle 2 verified the collision against code:* `reconcile.rs:99-117` filters
  children only by `ChildOf`/parent (no marker), and `shaping.rs:32` filters by
  `With<PanelTextChild>` today — so the swap is what introduces the false match;
  the root is not excluded by any structural filter. Options: **(a)** move the
  panel-root seed removal (step 9) into Phase 0, before the swap — narrowest fix,
  team-preferred, makes step 9 degenerate; **(b)** filter child queries
  structurally — `With<TextContent>` + `With<ChildOf>` or `Without<DiegeticPanel>`
  — so a root never matches regardless of the seed; **(c)** keep a dedicated child
  marker (don't delete `PanelTextChild`; instead `#[require]` it from
  `TextContent` on spawned children) and keep filtering on it.

- **DTX-2 — Observer double-layout per `set_text`.
  (important, risk/architecture, proposed)** DT2=(a) keeps reconcile as the
  spawner: reconcile writes the new string into the child `TextContent` when it
  changes. The Phase 2 step 10 observer fires on `Changed<TextContent>` for
  children and dirties `ComputedDiegeticPanel` to relayout. When the *source* of a
  change is reconcile itself, the observer re-fires on reconcile's own write →
  a second layout pass per edit (a 2× cost, and an amplifier for the known resize
  freeze). Options: **(a)** gate the observer to fire only on out-of-flow edits —
  a `#[doc(hidden)]` `ReconcileOwned` marker reconcile inserts on its own write
  and the observer filters `Without<ReconcileOwned>`, cleared next frame
  (team-preferred — precise, no change-detection internals); **(b)** accept the 2×
  pass and lean on the TR-C/TR-M perf gate to catch regressions, with D1(c) as the
  fallback; **(c)** have reconcile write `TextContent` via
  `bypass_change_detection()` so the reconcile-owned write sets no `Changed` flag
  (terser, but risks masking a legitimately-coincident user edit the same frame).

- **DTX-3 — does `.id("title")` return the `PanelElementId`?
  (important, ergonomics/type-system, proposed)** DT5 keeps lookup stringly
  (`text_child(&PanelElementId)`, no new handle type) — a typo at the lookup site is
  a silent `None` + debug `warn!`. Three lenses independently flagged that the
  caller must hand-rebuild the exact string at the lookup site. This does **not**
  reopen DT5 (no new type): the question is only how a caller avoids retyping the
  exact string at the lookup site. *Cycle 2 found a constraint:* `.id()` sits
  mid-builder-chain, so it must return `Self` to keep chaining — it **cannot** also
  return the `PanelElementId`. So the realistic options are: **(a)** bind the id as a
  value first and pass it in — `let id = PanelElementId::named("title");
  El::text(..).id(id.clone()); panel.text_child(&id)` — which mirrors how
  `editable_field(PanelElementId::from("name"), …)` already takes the id as an arg
  (team-preferred — one pattern for both id families, no new surface); **(b)** keep
  `.id("title")` taking a `&str` and rebuild `PanelElementId::from("title")` at the
  lookup site (status quo, accepts the typo cost, callers use a `const` if reused);
  **(c)** add an id-registry helper (`panel_ids! { TITLE = "title" }`) so the
  string is defined once and shared by both sites.
