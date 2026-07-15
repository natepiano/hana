# DiegeticText: panel text, element ids, and where the string lives

Panel text is a first-class in-world feature: a text run rendered as PBR-lit
world-space (or screen-space overlay) geometry inside a `DiegeticPanel`. This
doc describes the shipped model — one `DiegeticText` facade, stable
`PanelElementId`s on runs, the panel→run relationship, and the get/set API — for
a developer modifying this code today.

## What it is

- **`DiegeticText`** (`fluent.rs:68`) — authoring sugar for a standalone label: a
  one-element `DiegeticPanel`. `DiegeticText::world(text)` / `screen(text)` return
  a `DiegeticTextBuilder`; `.build() -> impl Bundle` / `.spawn(&mut Commands)`
  materialize it. `DiegeticText` is also a marker component left on the spawned
  panel-root entity, so a single label is queryable via `With<DiegeticText>`.
- **`El::text` / `Text`** (`layout/builder.rs`) — a text leaf inside a
  hand-built multi-element panel tree. `builder.text(impl Into<Text>)` takes
  `("string", TextStyle)` or `Text::new(text, style)`.
- Every text leaf, standalone or in a tree, becomes one or more **run child
  entities** carrying `TextContent`, `PanelTextLayout`, and `TextRunOf(panel)`.
  Glyphs shape into meshes from those children.

## How it works

### Where the string lives (authoritative tree, derived children)

The panel's layout tree is the single source of truth for run text. Each text
`Element` stores its string in `El.text`; the layout engine reads it directly to
measure and word-wrap (the engine is a pure `Fn(&str, &TextMeasure)` over the
tree). Reconcile derives everything downstream from the tree.

- `TextContent` (`render/world_text/mod.rs:89`, `#[require(TextStyle, Transform,
  Visibility)]`) sits on each run child and holds the string reconcile copied out
  of the tree. It is **derived output**, not a source — reconcile rewrites it each
  pass (`reconcile.rs:360`, `:436`).
- A **wrapped** run materializes as one run child per visual line, each holding a
  per-line slice. The authoritative full-run string is always `El.text`
  (`tree().element_text(idx)`), never any single child slice.

Reads therefore come from `El.text`; writes go into `El.text` (which re-wraps and
re-derives the children), never into a child `TextContent` directly.

### Element ids

`PanelElementId` (`ime/ids.rs:85`) is an enum, not a string newtype:

```rust
pub enum PanelElementId {
    Named(String),          // author-assigned, publicly addressable
    Auto(AutoElementId),    // builder-minted, positional, not addressable
}
```

- `From<&str>` / `From<String>` / `PanelElementId::named(..)` always yield
  `Named`. `Auto` is minted only by `PanelElementId::auto(u32)` (`pub(crate)`),
  wrapping a private `AutoElementId(u32)` — no external code can forge an `Auto`,
  so named and auto ids share one panel-local namespace and cannot collide.
- **Authoring is the `.id(...)` chain**: `Text::new(text, style).id("title")`
  (`builder.rs:154`) or `El::id("title")` (`builder.rs:421`), both
  `impl Into<PanelElementId>`. There is no `text_id(...)` builder method.
- Unnamed text gets an `Auto(u32)` from a per-build order counter
  (`take_auto_id`, `builder.rs:874`; `next_auto_id` reset to 0 each build). Named
  runs use their name; `.id()` does not consume the auto counter.
- Element ids and editable-field ids share one namespace and one duplicate check.
  A repeated explicit id is a build error on `build() -> Result<_, PanelBuildError>`.

Stability gradient: **named ids are content-stable and addressable; auto ids are
positional and respawn on reorder.** Name a run to keep its identity across tree
edits.

### The panel↔run relationship

`render/panel_text/relationship.rs` defines a typed Bevy relationship over the
text-run subset of a panel's children:

```rust
#[relationship(relationship_target = PanelTextRuns)]
pub struct TextRunOf(#[entities] pub Entity);   // on each run child → panel

#[relationship_target(relationship = TextRunOf)]
pub struct PanelTextRuns(Vec<Entity>);          // on the panel → its runs
```

- Runs keep `ChildOf(panel)` for transform propagation and despawn; `TextRunOf`
  is an additive traversal index so a query reaches a panel's runs without
  filtering `Children` by `PanelTextLayout`.
- `PanelTextRuns` has **no `linked_spawn`** — `ChildOf`'s `linked_spawn` is the
  sole despawn path; a second would double-despawn. The relationship's on-remove
  hook still drops a despawned run from the set.
- `PanelTextRuns::sole() -> Option<Entity>` returns the lone run of a
  single-line label. It is count-based, so it returns `None` for a wrapped label
  (>1 entity); the access layer's `line_index == 0` filter (`lone_run`,
  `access.rs:261`) resolves those instead.
- `PanelTextRuns` derefs to `[Entity]` (`len()`, indexing, `iter()` from the
  derive yields `Entity` by value).
- A panel with no text run carries **no `PanelTextRuns` component at all** (the
  target only materializes when a `TextRunOf` source points at it).

### Reconcile

`reconcile_panel_text_children` (`reconcile.rs:185`, `PostUpdate`, gated on
`Changed<ComputedDiegeticPanel>`) walks the computed tree and spawns/updates one
run child per text render command:

- Reuse key is the content-stable **`(PanelElementId, line_index)`** carried on
  the child as `PanelTextLayout { id, line_index, element_idx }`
  (`panel_text/layout.rs`). A named run survives sibling reorders; an auto run,
  whose id shifts with build order, respawns.
- `spawn_panel_text_child` inserts `TextContent`, `PanelTextLayout`, and
  `TextRunOf(panel)` on newly spawned runs. The reuse branch must **not**
  re-insert `TextRunOf` (that would fire the relationship hook and mutate
  `PanelTextRuns` on a no-op).
- Reconcile rebuilds the panel's `id → Entity` index each pass, mapping each
  named run's `line_index == 0` child (`text_index`, `diegetic_panel.rs:223`),
  written via `bypass_change_detection()` so it does not re-dirty the panel.

### Access and mutation (`render/panel_text/access.rs`)

Two ways to address a run divide the public surface:

- **By user marker — `DiegeticTextMut<M>`** — the ergonomic path for "retext my
  labels". Bundles the marker query, the layout query, and the panel write query;
  a caller names only its marker.
  - `set(text) -> usize` writes one string to every `M`-marked label.
  - `for_each_mut(|&M, &mut TextEdit|) -> usize` yields each marker and a
    `TextEdit` handle for per-label strings (it is `for_each_mut`, not
    `iter_mut`: a Bevy mutable many-entity query is a lending iterator).
  - `for_each_style_mut(|&mut TextStyle|)` restyles through the tree's
    authoritative `El.config` (mutating the run's derived style alone would render
    a new font while measuring the old).
  - Resolves each label's lone run by the `line_index == 0` filter, so a wrapped
    label is editable too.

  ```rust
  fn rename(mut labels: DiegeticTextMut<CubeFaceLabel>) { labels.set("hi"); }
  ```

- **By `PanelElementId` — `PanelText` / `PanelTextReader`** — for a named run on a
  multi-run panel. `PanelText` is the read-write `SystemParam`
  (`set_text(panel, &id, s)`, `set_sole_text(panel, s)`); `PanelTextReader` is the
  read-only variant (`text`, `entity`, `sole_text`) so reader systems don't
  serialize on the `&mut DiegeticPanel` write claim.
  - Resolution: `DiegeticPanel::text_child(&id) -> Option<Entity>`
    (`diegetic_panel.rs:372`) is an **unchecked** `&self` index read; it may return
    a dead entity. The SystemParam validates liveness with its `PanelTextLayout`
    query (`resolve_run_entity`, `access.rs:238`), so a despawned run reads back
    `None`. On a genuine miss (id absent from `tree().contains_text_id(id)`) a
    `#[cfg(debug_assertions)]` `warn!` fires; the not-yet-materialized window
    stays quiet.

Both paths funnel writes through **`TextEdit`** (`access.rs:295`):
`set_text` compares against the current `El.text`, and only on a real change calls
`DiegeticPanel::sync_run_text_cache(idx, text)` (writes `El.text`, bumps the tree
revision) and records `VisualOnly` on the panel's
`DiegeticPanelChangeClassification` (`note_text_edit`). That lets
`compute_panel_layouts` re-measure only the edited leaf and take the
geometry-stable skip when the box did not move.

## Invariants

- **The tree (`El.text`) is authoritative; child `TextContent` is derived.**
  Reconcile rewrites `TextContent` from the tree every pass. Never treat a child's
  `TextContent` as a source of truth.
- **Auto ids are per-build and positional.** The `u32` counter resets to 0 on
  every build/`set_tree`. Never persist or cross-panel-compare an auto id.
- **Named ids are unique per panel, checked at build.** Duplicates error on
  `build()`; the check spans element ids and editable-field ids together.
- **`ChildOf` owns despawn; `TextRunOf` is a traversal index only.** No
  `linked_spawn` on `PanelTextRuns`.
- **`PanelTextRuns` mutates only on run spawn/despawn**, never on a layout pass —
  the reuse branch skips re-inserting `TextRunOf`. Two consecutive no-op reconcile
  passes leave it `Changed`-false.
- **A `set_text` edit fires exactly one relayout pass.** The write lands once in
  the tree, reconcile re-derives the child, and there is no child→tree sync-back to
  oscillate.
- **An unchanged `set_text` drives no relayout and no re-measure** — `TextEdit`
  bails on equality before taking the `&mut` path.

## Gotchas

- **Do not mutate a run child's `TextContent` directly.** There is no
  `Changed<TextContent>` reactor that syncs a child edit back into the tree (the
  old `sync_run_text_to_cache` is gone). A direct write re-shapes glyphs
  (`shaping.rs` reads `Changed<TextContent>`) but leaves the tree's measured size
  stale — new font/size renders, old size measures. Always go through `TextEdit` /
  `PanelText` / `DiegeticTextMut`.
- **`PanelTextRuns::sole()` returns `None` for a wrapped label** (one entity per
  visual line). Use the access-layer `line_index == 0` resolution (`sole_text`,
  `DiegeticTextMut`), which the two-query `runs.sole() → get_mut` pattern does not
  cover — that pattern is correct only for a single-line label.
- **`DiegeticTextMut<M>` is type-keyed.** Two standalone labels that must be
  retext independently need distinct marker types; one shared marker's `set`
  rewrites both.
- **Same-frame after `set_tree`:** `set_tree` empties `text_index` immediately but
  reconcile only repopulates it next pass, so a `text_child`/`set_text` call in the
  same frame returns `None`/no-ops until the rebuild.
- **`set_text` can neither add nor remove a run.** A panel whose structure changes
  (a row appears/disappears) must rebuild via `set_tree`.
- **`sole_text` / `set_sole_text` are the one-run names** (not `text`/`set_text`):
  Rust has no overloading, and `text`/`set_text` are the id-addressed methods.

## Why

- **One string, one home.** The tree already owns the string the engine measures;
  keeping `El.text` authoritative and `TextContent` derived means a text edit and
  a relayout cannot disagree. Editing the derived child instead was the failure
  mode a single source removes.
- **Ids do double duty.** A `PanelElementId` is both the reconcile reuse key
  (content-stable identity across reorders) and the public lookup handle, mirroring
  Clay/egui/Flutter keys — one user-assigned key for addressing and identity.
- **Enum, not string newtype.** Making `Auto` unforgeable (`pub(crate)`
  constructor, private field) lets named and auto ids share one namespace with no
  runtime collision check.
- **Relationship + id map, both entity-only.** `PanelTextRuns` gives O(1)
  liveness/traversal and single-run findability for a marker on the panel entity
  (whose `TextContent` sits on the child); the `text_index` map gives O(1) named
  lookup a `Vec` cannot. Neither stores text — the string stays single-homed.
- **`DiegeticText` is build-time sugar.** The marker holds no state (space lives
  on the panel's `CoordinateSpace`, string on the run), so there is nothing on it
  to drift from the panel it produced.
