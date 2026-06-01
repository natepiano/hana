# DiegeticText: one text type, element ids, and TextContent as the single source

## Goal

Continue the text unification begun in [`unify_text.md`](unify_text.md) along
three axes, all driven by the same constraint — **fewest types, one place per
fact**:

1. Collapse `WorldText` + `ScreenText` into a single **`DiegeticText`** with
   `DiegeticText::world(text)` / `DiegeticText::screen(text)` constructors,
   mirroring `DiegeticPanel::world()` / `DiegeticPanel::screen()`.
2. Give panel **text elements stable identifiers**, reusing `PanelFieldId`, so a
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
named-element mechanism is `PanelFieldId` (`ime/ids.rs:62`, a `String` newtype),
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

- The layout `El` stores a **`PanelFieldId`, not the string**
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

### D3 — Element identifiers reuse `PanelFieldId`

`El::text(text, config).id("title")` assigns a panel-local id, reusing
`PanelFieldId`. The id does double duty:

- **Reconciliation identity** — replaces `(element_idx, command_index)` as the
  child reuse key, so a named run survives reorders (the fragility the
  `reconcile.rs:35-39` comment calls out).
- **Lookup handle** — the public way to address a run (D5).

Panel-local, like `PanelFieldId` today, so two panels may both use `"title"`.

### D4 — Auto-id for unnamed text

Under D2 the id is the **resolution key** for the string, so every text leaf must
have one — it is no longer optional addressing sugar. Therefore:

- `.text(text, config)` stays unchanged and **auto-assigns** an id. Explicit
  `.id("…")` is added only where a run is addressed/mutated. This keeps the
  100+ static-label call sites untouched.
- **Namespace named vs auto** so they cannot collide by construction —
  `PanelFieldId` distinguishes them (enum `{ Named(String), Auto(u32) }`, or auto
  uses a reserved form `From<&str>` cannot produce). See OQ3.
- Auto-id source: a **per-tree build-order counter** — i.e. exactly today's
  positional identity.
- **Duplicate explicit ids** are caught at build time (the builder tracks a set →
  `debug_assert!` or `Result`), reusing the editable-field duplicate-id error
  path.
- **Stability gradient**: named ids are content-stable and addressable; auto ids
  are positional-stable and not publicly addressable. *Name it to address it* —
  unnamed text still renders, you just cannot grab it later.

### D5 — Lookup and mutation API

- **Primitive**: `DiegeticPanel::text_child(&PanelFieldId) -> Option<Entity>`,
  backed by an `id → Entity` index the panel holds and reconcile maintains (O(1);
  reconcile already walks the children, so upkeep is negligible). For a
  `DiegeticText` (single element) it resolves the one child.
- **SystemParam** for synchronous get/set inside a system:

  ```rust
  #[derive(SystemParam)]
  pub struct PanelText<'w, 's> {
      panels:  Query<'w, 's, &'static DiegeticPanel>,
      content: Query<'w, 's, &'static mut TextContent, With<PanelTextChild>>,
  }

  impl PanelText<'_, '_> {
      pub fn entity(&self, panel: Entity, id: &PanelFieldId) -> Option<Entity> {
          self.panels.get(panel).ok()?.text_child(id)
      }
      pub fn set(&mut self, panel: Entity, id: &PanelFieldId, text: impl Into<String>) -> bool {
          let Some(child) = self.entity(panel, id) else { return false };
          self.content.get_mut(child).map(|mut c| c.set_text(text)).is_ok()
      }
  }
  ```

  Cost is exactly the two queries plus the O(1) index hit — synchronous, no
  deferral. Scoping `With<PanelTextChild>` limits the mutable-access claim.

- **Deferred write convenience (optional)**: a `Commands` extension mirroring
  `DiegeticPanelCommands::set_tree` (`diegetic_panel.rs:400-411`, which runs via
  `run_system_cached_with`), e.g. `commands.set_panel_text(panel, id, "new")`,
  queuing a one-shot that resolves via the index and writes. A getter can **not**
  be a command — commands are deferred and return nothing; reads must be the
  SystemParam/method above. See OQ4.

### D6 — `TextStyle` unchanged

`TextStyle` stays exactly as `unify_text.md` left it: the authoring config
(`El::text(.., TextStyle)`, held by `DiegeticText`) and the per-child component
(`#[require]` on `PanelTextChild`) are deliberately the same type. No change.

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

3. **OQ3 — `PanelFieldId` representation.** Enum `{ Named(String), Auto(u32) }`
   vs `String` with a reserved auto-form. The enum is collision-proof by
   construction and keeps `From<&str>` always-`Named`; it is a small extension to
   the "PanelFieldId is fine for now" decision.

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
  `typography.rs:641-642`. These keep working (`TextContent` stays the source
  under D2); optionally adopt named ids + the `PanelText` SystemParam where it
  reads cleaner than the current marker-component queries.
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
2. **`PanelFieldId` → enum (DT3).** `enum { Named(String), Auto(u32) }`;
   `From<&str>`/`From<String>` always yield `Named`. Editable-field call sites
   (`impl Into<PanelFieldId>`) keep compiling.
3. **Remove the panel-root `TextContent` seed first (DTX-1).** Before any filter
   swap, delete the fluent panel-root `TextContent` seed (`fluent.rs:328`, `:441`)
   and the `FluentText` marker, so the only `TextContent` left is on run entities.
   This pulls Phase 2 step 9 forward — it must precede step 4 below, or a child
   query would transiently match one-element fluent roots. (Relayout-on-string-edit
   for the fluent path now rides the Phase 2 observer, step 10, not the old
   panel-root seed.)
4. **Delete `PanelTextChild` (DT4-iii).** Move `#[require(TextStyle, Transform,
   Visibility)]` onto `TextContent`; delete `PanelTextChild`; swap every
   `With<PanelTextChild>` → `With<TextContent>` and `Without<PanelTextChild>` →
   `Without<TextContent>` (`shaping.rs:33` already filters `With<TextContent>`).
   With the root seed gone (step 3), `With<TextContent>` now matches run entities
   only. Broad but mechanical; no behavior change.

### Phase 1 — element ids
5. **Id field + setter (DT3, DTX-3).** Add a `PanelFieldId` to
   `ElementContent::Text` (alongside the `text` cache from Phase 2); add
   `El::text(...).id(impl Into<PanelFieldId>)`. `.id()` returns `Self` to keep the
   builder chain; callers that need the id at lookup bind it as a value first
   (`let id = PanelFieldId::named("title"); ….id(id.clone())`), mirroring
   `editable_field`'s arg-passed id (DTX-3=a) — no chain-returned handle. Auto-id
   from a per-tree build-order counter (`u32`, TR-E), reset per build (TR-N);
   text-run ids and editable-field ids share one panel-local namespace and one
   duplicate check (TR-O). `Auto` is not publicly constructible (TR-K).
6. **Duplicate ids → `Result` at build (DT6-i).** A repeated explicit id is an
   error on the existing `build() -> Result`; no silent release shadowing.
7. **Id-keyed reconcile + index (DT3, TR-A).** Switch the reconcile reuse key
   from `(element_idx, command_index)` to the id (named runs survive reorder;
   auto runs keep positional semantics, TR-D). Build the `id → Entity` index
   from scratch each reconcile; clear it on `set_tree`.

### Phase 2 — `TextContent` as the source (DT1=b, DT2=a)
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

### Phase 3 — lookup + mutation API
11. **`text_child` + helper (DT5, DT6-ii, DT4-ii).**
    `DiegeticPanel::text_child(&PanelFieldId) -> Option<Entity>`, backed by the
    Phase-1 index, validating liveness (a despawned child returns `None`) with a
    debug-only `warn!` on miss. `DiegeticText` gets a single-string helper
    (`text()` / `set_text(…)`) for the one-run case — no id needed.
12. **`PanelText` SystemParam + reader (TR-B).** `PanelText` bundles the panel +
    run queries for get/set by id; add a read-only `PanelTextReader` so reader
    systems don't serialize on `&mut TextContent`. The deferred
    `commands.set_panel_text(…)` extension is deferred until a consumer needs it
    (TR-F).

### Phase 4 — examples migration (separate pass)
13. Apply the migration inventory (TR-I): auto-id leaves static `.text()` calls
    unchanged; runtime-mutation sites keep their marker + `Query<&mut TextContent>`
    pattern **or** adopt `.id()` + `PanelText`.

### Phase 5 — verify
14. `cargo build && cargo +nightly fmt`, `/clippy`. Tests:
    `text_child(id)` resolves a named run; an auto-id'd run is not addressable;
    duplicate explicit ids error at build; mutating a run's `TextContent`
    relayouts (the property D2 buys); a reorder keeps named runs and respawns auto
    runs (TR-D); `set_tree` clears stale index entries and an orphaned id returns
    `None` (TR-A).
15. **Perf gate with criteria (TR-C, TR-L, TR-M).** Target < 16.7 ms/frame
    release; flag > 5% over a `main` baseline. Profile the per-frame-`set_text`
    cube-face examples (`input_keyboard`, `orthographic`, `pausing`) — not only the
    static `cascade`/`paper_sizes`/`world_text` panels, **and** add a resize pass
    on a complex-font panel (the known freeze path,
    `project_diegetic_panel_freeze.md`, which DTX-2's double-layout would amplify
    if the `ReconcileOwned` gate regressed). Criterion restated for DT1=(b)
    (TR-L): there is no OQ1(a) gather — the `TextContent → El.text` sync (step 8)
    must be **O(n_changed)** (driven off `Changed<TextContent>`, never a full
    `n_elements` walk) and must not re-invoke `MeasureTextFn` for an unchanged
    cached string; target < 0.5 ms on the 100-label panels. Regression fallback:
    the `unify_text.md` D1(c) lightweight single-element path.

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

- **DT3 — `PanelFieldId` representation (OQ3). (important, type-system, proposed)**
  Unanimous team rec: **enum `{ Named(String), Auto(u32) }`** — encodes the
  named-vs-auto invariant at the type level, `From<&str>` always yields `Named`,
  `Eq`/`Hash`/`Reflect` derive cleanly, editable-field call sites still compile.
  Alternative kept on the table because you said "PanelFieldId is fine for now":
  the `String` newtype with a reserved auto-form (no public type change, but a
  runtime escape hatch). Also decide whether text-run ids and editable-field ids
  share one panel-local namespace (and one duplicate check) or stay separate.
  **→ DECIDED: enum + shared namespace.** `PanelFieldId` becomes
  `enum { Named(String), Auto(u32) }`; `From<&str>`/`From<String>` always produce
  `Named`, so no `&str` can forge an `Auto`. Text-run ids and editable-field ids
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
  ergonomics, proposed)** `.id("title")` at authoring + `text_child(&PanelFieldId)`
  at runtime is a stringly reuse — a typo is a silent `None`. Options:
  **(a)** the `.id(...)` builder call returns the `PanelFieldId` for the caller to
  hold and reuse; **(b)** authoring returns an opaque `TextId` handle (cannot be
  forged); **(c)** keep stringly lookup but make `text_child` return a helpful
  error for an unknown id ("did you forget `.id()`?"). Mirrors the existing
  editable-field handle pattern (`set_field_display_text(&field_id, …)`).
  **→ DECIDED: look up by `PanelFieldId` (no new handle type).** A run is
  addressed by the same `PanelFieldId` from DT3 — `text_child(&id)` — built from a
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

- **TR-K — `Auto` variant must be unforgeable.** `PanelFieldId::Auto(u32)`
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
  means `build()`'s duplicate check must collect text-run ids **and**
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
> (`let id = PanelFieldId::named("title"); El::text(..).id(id.clone());
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

- **DTX-3 — does `.id("title")` return the `PanelFieldId`?
  (important, ergonomics/type-system, proposed)** DT5 keeps lookup stringly
  (`text_child(&PanelFieldId)`, no new handle type) — a typo at the lookup site is
  a silent `None` + debug `warn!`. Three lenses independently flagged that the
  caller must hand-rebuild the exact string at the lookup site. This does **not**
  reopen DT5 (no new type): the question is only how a caller avoids retyping the
  exact string at the lookup site. *Cycle 2 found a constraint:* `.id()` sits
  mid-builder-chain, so it must return `Self` to keep chaining — it **cannot** also
  return the `PanelFieldId`. So the realistic options are: **(a)** bind the id as a
  value first and pass it in — `let id = PanelFieldId::named("title");
  El::text(..).id(id.clone()); panel.text_child(&id)` — which mirrors how
  `editable_field(PanelFieldId::from("name"), …)` already takes the id as an arg
  (team-preferred — one pattern for both id families, no new surface); **(b)** keep
  `.id("title")` taking a `&str` and rebuild `PanelFieldId::from("title")` at the
  lookup site (status quo, accepts the typo cost, callers use a `const` if reused);
  **(c)** add an id-registry helper (`panel_ids! { TITLE = "title" }`) so the
  string is defined once and shared by both sites.
