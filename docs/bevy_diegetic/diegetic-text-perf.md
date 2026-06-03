# Diegetic text performance — options

## Baseline

Conditions: `diegetic_text_stress`, 100 world labels each restrung every frame,
M2 Max, release, `with_perf_mode` (AutoNoVsync + `WinitSettings::continuous`).
Add a column after each phase lands.

Rows match the `diegetic_text_stress` overlay labels. The layout-area cells for
the instrumented columns (1a / 1b) read `now / 5s-max` to mirror the overlay's two
value columns; the `now` (moving mean) is the stable number, `5s-max` is peak-hold
and spiky (a single hitch spikes it). Where a column lacks a separately-captured
peak, only `now` is shown. The `After 3 ⊙` column is the geometry-stable skip — it
recovers the 1b regression and is the current state.

| Overlay row                  | Baseline (2026-06-02) | After A     | After C      | After 1a ⊗ (now) | After 1b ⊘ (now / 5s-max) | After 3 ⊙ (now) | After B |
| ---------------------------- | --------------------- | ----------- | ------------ | ---------------- | ------------------------- | --------------- | ------- |
| `ms` (frame) ‡               | ~25 ms                | ~24 ms      | ~18.5 ms ‡   | —                | —                         | —               |         |
| `fps` ‡                      | 40                    | 42          | 55 ‡         | —                | —                         | —               |         |
| `layout` †                   | 0 / 5.8 ms            | 0 / 5.8 ms  | 0 / 5.8 ms ⊕ | 0.14 ⊗           | 0.15 / 0.69 ⊘             | **~0.10** ⊙     |         |
| — `setup`                    | —                     | —           | ~3 ms ⊕      | ~0 ⊗             | ~0 / 0.001 ⊘              | ~0 ⊙            |         |
| — `scale`                    | —                     | —           | <0.2 ms ⊕    | 0.04 ⊗           | 0.04 / 0.32 ⊘            | ~0.03 ⊙         |         |
| — `solve`                    | —                     | —           | <0.8 ms ⊕    | 0.08 ⊗           | 0.08 / 0.24 ⊘            | **~0.02** ⊙     |         |
| — `commit`                   | —                     | —           | <0.1 ms ⊕    | 0.02 ⊗           | 0.02 / 0.10 ⊘            | **~0** ⊙        |         |
| `shaping`                    | ~0.6 ms               | ~0.31 ms    | ~0.33 ms     | 0.39             | 0.39 / 0.95 ⊘            | ~0.37 ⊙         |         |
| `mesh`                       | 1.8 ms                | **1.29 ms** | **0.12 ms**  | 0.12             | 0.12 / 0.37 ⊘            | ~0.12 ⊙         |         |
| `remainder` ‡                | —                     | ~18 ms ‡    | ~14.7 ms ‡   | —                | —                         | —               |         |
| Paused `fps` ‡ (not on HUD)  | 98                    | ~55         |              | —                | —                         | —               |         |

† Pre-1b the `layout` row reads `min / max` over alternate frames — the flip-flop
zeroed it every other frame, so the moving mean is halved. From 1b on, layout runs
every frame (steady 100 panels), so the `now` column is the true per-frame mean.

‡ The frame-time, FPS, paused, and render-floor rows are fill-rate-bound and scale
with window size, and the A and C columns were captured in separate sessions; the
absolute numbers are **not** comparable across columns. The window-independent
measure is the CPU `mesh_build_ms` row: 1.8 → 1.29 (A) → **0.12** (C).

⊕ Step-0 instrumentation (2026-06-03) split `compute_ms` into `setup` / `scale` /
`solve` / `commit`. `setup` (the per-frame `ShapedTextCache` clone) is the dominant
share of `compute_ms`, and it is **bounded**: `label_text` is
`format!("{index:02} {:03}", frame % 1000)`, so the distinct-string set caps at
100 indices × 1000 counter values = 100 000 entries. The cache fills over the first
~1000 frames, then every string is a hit and the clone plateaus — **steady ~2 ms,
5s-max ~3 ms** (observed uncontended in prior runs). A live BRP sample during this
session read `setup` climbing to ~13 ms, but that was concurrent-compile CPU
contention inflating the clone, not the cache growing — discard those magnitudes.
`scale` / `solve` / `commit` stay sub-millisecond (the cells below are contended-
sample upper bounds; a clean run will lower them). The split is the point: `setup`
is the largest single layout cost, so removing the clone (share the cache behind its
`Arc<Mutex>` instead of cloning) is the real win — paired with removing the flip-flop
for correctness. See Step 1a / Step 1b.

⊗ Step-1a result (2026-06-03, this session, uncontended). Moved `ShapedTextCache`'s
two maps behind one internal `Arc<Mutex<…>>`, so `cache.clone()` in
`compute_panel_layouts` is now a refcount bump (not a map copy) and the measure
closure's cache-miss inserts persist into the shared cache instead of being thrown
away. Sampled 73 active frames over ~5 s (flip-flop still alternating, so the other
half of frames skip layout and read 0). `setup` dropped from ~2 ms steady / ~3 ms
5s-max to **mean 0.0003 ms, max 0.0011 ms** — gone. `compute_ms` fell to **mean
0.14 ms, max 0.21 ms** (the whole `setup` share removed). `scale` / `solve` /
`commit` are unchanged — the layout work itself did not change; the values here are
clean means, below C's contended upper bounds. 259 crate tests pass; clippy
(nursery + pedantic) clean. The `Res<ShapedTextCache>` clones the handle into the
`'static` measure closure; the renderer and overlay paths now hold it as `Res` and
mutate through `&self`. Flip-flop still in place — Step 1b removes it next.

⊘ Step-1b result (2026-06-03, this session) — the deliberate regression Step 3
recovers. Deleted the flip-flop: removed `ReconcileOwned`, `sync_run_text_to_cache`,
and `clear_reconcile_owned`; routed the four writers to the authoritative tree
through a new `TextEdit` cursor (`PanelText` / `DiegeticTextMut::for_each_mut`),
making the tree the single source and `TextContent` pure derived output reconcile
rewrites. `compute_panels` went from alternating 100 / 0 to **steady 100 every
frame** (147 / 150 sampled frames at 100; the 3 at 101 are the FPS overlay panel),
so layout now runs every frame instead of every other.

Quiet-machine readout (overlay `now / 5s-max`, ms): `layout` **0.15 / 0.69**,
`setup` ~0 / 0.001, `scale` 0.04 / 0.32, `solve` **0.08 / 0.24**, `commit`
0.02 / 0.10. The clean per-frame layout cost is **essentially the same as 1a's**
(layout `now` 0.15 vs 0.14, `solve` 0.08 vs 0.08) — removing the flip-flop did not
make each pass heavier; the regression is **frequency**: layout now runs every
frame instead of every other, so per-frame-averaged layout CPU roughly doubled
(~0.07 → ~0.15 ms) while the per-active-frame `solve` is unchanged. (An earlier
sample this session read `solve` 0.31 / `layout` 0.38; that was concurrent-build
CPU contention inflating it — same effect as the original `setup` misread —
discard those magnitudes.) `setup`, `scale`, `commit`, and the text path
(`shaping` ~0.39, `mesh` ~0.12) are unchanged.

258 crate tests pass (the two `ReconcileOwned` / sync-back lifecycle tests were
retired; the "no-op set_text fires no measure" and "one edit = one relayout"
properties were rewritten to drive the public tree path and still hold); clippy
(nursery + pedantic) clean, full workspace builds. `TextEdit::set_text`
read-compares before the `&mut DiegeticPanel` borrow, so the no-op-no-relayout
guard the deleted sync held is preserved. Step 3 (the geometry-stable skip) routes
text-only edits whose measured size is unchanged to the cheap `VisualOnly` path —
every stress label is fixed-width `"NN MMM"`, so it should drop the full `solve` to
genuine-reflow only and pull `compute_panels` back down from steady 100.

⊙ Step-3 result (2026-06-03, this session, quiet machine, port 12000 release).
The text-edit path now records `VisualOnly` on the change-classification sibling
(via `TextEdit` → `DiegeticPanelChangeClassification::note_text_edit`), and
`compute_panel_layouts` gates the existing `VisualOnly` → `regenerate_commands`
branch on a new `LayoutResult::can_reuse_geometry` probe: it re-measures each text
leaf (cache-backed) and reuses the cached geometry only when structure, viewport,
and every leaf's measured width are bit-identical and no leaf is wrapped. Every
`"NN MMM"` label measures identical frame to frame, so all 100 take the cheap path.
Three live samples: `layout` **~0.10** (was 0.15), `solve` **~0.02** (was 0.08 —
the full `engine.compute` is replaced by `regenerate_commands`), `commit` **~0**
(was 0.02 — the cheap path `continue`s before `commit_layout_result`); `scale`
~0.03, `setup` ~0, text path unchanged (`shaping` ~0.37, `mesh` ~0.12). Layout is
now **below the flip-flop-era per-frame cost** with no marker, no every-other-frame
gating, and no reflow lag. One correction to the 1b prediction: `compute_panels`
**stays 100**, not lower — the regenerate path still counts each panel
(`panel_count += 1`); the win is in `solve` / `commit` per pass, not in the count.
A real reflow (a width-changing edit) still classifies as a full solve, so the skip
never renders stale geometry. `can_reuse_geometry`'s three guards (same-width swap,
newline rejection, wrapped-leaf rejection) are covered by engine unit tests; full
crate suite + clippy (nursery + pedantic) green.

The four sub-rows (`setup`+`scale`+`solve`+`commit` ≈ 0.05) do not sum to the
`layout` total (~0.10): the total is the whole system's wall-clock, the sub-rows
only their own timed spans. The ~0.05 remainder is un-instrumented per-panel loop
work (change-detection reads, two query `get`s) plus the new `can_reuse_geometry`
probe (100 cache-backed re-measures/frame), left outside `solve`. Folding the probe
into the `solve` span would close most of the gap — deferred as cosmetic.

## Finding — A is CPU-only; this stress test is render-bound

A landed correctly and all 255 tests pass: runs now overwrite their mesh and
three GPU buffers in place behind stable handles keyed by the label entity
(`RunStorageKey::from(entity)`), and the mesh child + material persist instead of
being despawned and re-added every frame. The measured effect is
`mesh_build_ms` 1.8 → 1.29 ms — the per-frame `meshes.add()` + 3×
`storage_buffers.add()` + mesh-entity respawn + `materials.add()` are gone.

But the moving frame time barely moved, and that corrects the earlier model. The
paused frame — text not changing, diegetic CPU ≈ 0 — is still ~18 ms, so that
entire floor is GPU render: OIT transparency + shadows + 3-light studio lighting
over ~600 glyph quads, which A does not touch. Diegetic CPU is only ~1.6–7 ms of
the frame (alternating with the layout toggle), **not** the ~7–10 ms the prior
note attributed to "asset churn." The per-frame text mutation costs ~6 ms
(moving − paused); A optimizes a ~0.5–1 ms slice of it, and the ~18 ms render
floor is the dominant, untouched cost.

Implication for ordering: to move *this* test's frame time, attack the render
(transparency / OIT / shadow / fill-rate) and per-frame upload volume
(instancing — **B**), not CPU churn. **D** still removes the alternating ~5.8 ms
layout CPU. A remains the right foundation — stable per-label identity is what a
later content-hash skip would build on — it just isn't where this test's wall
clock goes.

## Finding — C: shared glyph atlas (done 2026-06-02)

Each run formerly re-copied its glyphs' curves/bands into per-run `Vec`s and
uploaded three per-run `ShaderBuffer`s every frame (100 runs × the same digits
`0`–`9` = 100× duplication). C packs each glyph ONCE into one shared append-only
atlas (`GlyphOutlineCache` now holds `record_indices` / `curves` / `bands` /
`glyph_records` / `revision`); the run mesh stores the glyph's GLOBAL atlas index
in `UV_1.x`, and every material binds the same three shared buffers.
`RunRenderData` / `RunStorage` shrank to mesh-only; `commit_glyph_atlas` uploads
the shared buffers in place, only when `revision` grows → zero in steady state.
Shader + record formats unchanged.

Measured with the mesh sub-breakdown instrumentation (pre-C session), now / 5s-max:
- pack 0.85 → **0.04**, upload 0.93 → **0.06**, material 0.05 → **0.03**,
  `mesh_build_ms` 1.82 → **0.12 ms** (~15×).
- Bonus: dropping 300 per-frame buffer uploads cut render-world extract/prepare
  too — render remainder ~18.3 → **~14.7 ms**.
- 257 tests pass. Atlas is append-only (no eviction) — fine for the stress test.

## Options

**A. Reuse the geometry (in-place mesh/buffer update)** — DONE 2026-06-02
- Keep each label's existing mesh + buffers and overwrite their contents when the
  text changes, instead of allocating a new asset and dropping the old one.
- Removes the ~400-per-frame allocate/upload/discard churn.

**B. True instancing (one shared quad)**
- Stop storing a quad per glyph. Keep one unit quad and draw it N times, feeding a
  per-glyph table (position + glyph id) the GPU expands.
- No per-label meshes; moving text just updates a small table. Also removes the
  duplicate quads.
- Big change — needs a custom instanced render pipeline. The eventual destination.

**C. Share glyph outlines across labels** — DONE 2026-06-02
- Store each glyph's outline once; labels point at the shared copy. See Finding — C.

**D. Single source of truth + geometry-stable skip** — IN PROGRESS (current phase)
- Started as "skip full layout on a text-only change," but the right fix is to
  remove the model that forces the every-other-frame toggle. See the phase plan
  below.
- Step 0 (instrumentation) done 2026-06-03; it surfaced a second cost — the per-frame
  `ShapedTextCache` clone (`setup`, ~2 ms) — so the plan now also kills the clone
  (Step 1a) before removing the flip-flop (Step 1b).
- Saves the alternating layout-solve CPU, removes the per-frame cache clone, and
  removes a 1-frame reflow lag.

## Phase D (current) — tree as single source of truth, paired with a geometry-stable skip

### The problem

Text has two homes kept in bidirectional sync over one `Changed<TextContent>`
flag:
- tree → child: `reconcile_panel_text_children` writes the child's `TextContent`
  from the panel's layout tree.
- child → tree: `sync_run_text_to_cache` pulls a `TextContent` edit back into the
  tree cache + bumps `tree_revision` → relayout.

That loop is circular — reconcile's own tree→child write trips `Changed`, which
the child→tree sync would read as a user edit and re-fire forever. `ReconcileOwned`
is a one-frame marker whose only job is to hide reconcile's writes from the one
sync pass; `clear_reconcile_owned` strips it next frame. Delicate ordering holds
it together (`sync.before(ApplyTreeChanges)`, `clear.after(sync)`, reconcile in
PostUpdate). A second consumer, `shape_panel_text_children`, reads the same
`Changed<TextContent>` with NO gating.

Side effects of the marker:
- Layout is gated to every OTHER frame (`compute_panels` alternates 100 / 0) — a
  fragile accidental optimization, not a designed one.
- Glyphs do **not** lag (shaping reads `Changed<TextContent>` directly, ungated),
  but a layout-affecting (reflow / width) edit that lands on a marked frame is
  delayed one frame under continuous editing.

### Why it was built this way (the ergonomic reason)

`TextContent` was made a directly-mutable ECS component on the run child so
"retext my label" is a normal component mutation reached through a marker
(`access.rs:213-243`). The marker `M` sits on the panel and `TextContent` on the
child, so `DiegeticTextMut<M>` hops the panel→run relationship — a naive
`Query<&mut TextContent, With<M>>` matches nothing. That mutate-the-component
ergonomic forced the sync-back-to-tree, which forced `ReconcileOwned`.

### The de-risking asymmetry (verified in code)

Reads already go to the TREE: `PanelTextReader::text` / `sole_text` return
`tree().element_text(layout.element_idx)` (`access.rs:57,94`); the `resolve` doc
calls the tree "authoritative for valid ids at build time." The builder authors
text into the tree. ONLY the four writers detour through `&mut TextContent`
(`access.rs:172,186,258,276`). So the tree is already the single source for
everything except writes.

### The clone fix (Step 1a) — share the cache, stop discarding measurements

Independent of the tree-authoritative work and sequenced first, because removing the
flip-flop (1b) makes `setup` run every frame instead of every other — doubling the
clone's per-frame cost right as 1b lands. Fixing the clone first means 1b's
regression is measured against a `setup`-free layout, and the fix is measurable on
its own against today's flip-flop baseline.

Today `compute_panel_layouts` does `cache.clone()` of the whole `ShapedTextCache`
(`compute_layout.rs:61`), the measure-closure inserts cache misses into that *clone*,
and the clone is dropped at end of system — so every measurement computed during
layout is **thrown away** and the ~2 ms full-map copy repeats next frame. Move
`ShapedTextCache`'s maps behind an internal `Arc<Mutex<…>>` (interior mutability) so
that:
- `cache.clone()` (or a cheap `.handle()`) becomes a refcount bump, not a map copy →
  `setup` drops toward ~0;
- the measure-closure's inserts persist into the shared cache instead of being
  discarded → layout-side measurements are reused, not recomputed.

The closure already needs `'static + Send + Sync` to live inside `MeasureTextFn`; an
`Arc<Mutex>`-backed cache satisfies that without the per-frame clone. Expect `setup`
~2 ms → ~0; `compute_ms` drops by the `setup` share; behavior unchanged.

### The fix — route writes to the tree too (Step 1b)

`TextContent` becomes pure derived output (reconcile writes it tree→child; shaping
reads it). Delete `sync_run_text_to_cache`, `clear_reconcile_owned`,
`ReconcileOwned`, and every `Without<ReconcileOwned>` filter. Shaping / mesh /
cascades unchanged.

The four writers reroute from `content.set_text(text)` to
`panel.sync_run_text_cache(element_idx, &text)` (the same write the deleted sync
system used — updates `El.text`, bumps `tree_revision`). The reader's existing
`resolve` / `lone_run` find the element.

### The API (one visible change)

`DiegeticTextMut<M>` keeps the marker-query-then-mutate ergonomic; only the
mutated handle's type changes. New tree-edit cursor:

```rust
pub struct TextEdit<'a> {       // name TBD: TextEdit / TextCursor / PanelTextEdit
    panel:       &'a mut DiegeticPanel,
    element_idx: usize,
}
impl TextEdit<'_> {
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.panel.sync_run_text_cache(self.element_idx, &text.into());
    }
    #[must_use]
    pub fn text(&self) -> &str {
        self.panel.tree().element_text(self.element_idx).unwrap_or("")
    }
}
```

```rust
#[derive(SystemParam)]
pub struct DiegeticTextMut<'w, 's, M: Component> {
    markers: Query<'w, 's, (Entity, &'static M, &'static PanelTextRuns)>,
    layouts: Query<'w, 's, &'static PanelTextLayout>,
    panels:  Query<'w, 's, &'static mut DiegeticPanel>,   // was: Query<&mut TextContent>
}
```

`M`, `PanelTextRuns`, `DiegeticPanel` are disjoint components, so the marker read
and the tree write don't conflict on the same entity. Call site is unchanged
except the binding name: `for_each_mut(|label, edit| edit.set_text(...))`.
`set` / `set_text` / `set_sole_text` signatures are unaffected (string-only; pure
internal change). The single visible change is `for_each_mut`'s closure parameter
type: `&mut TextContent` → `&mut TextEdit`.

### The geometry-stable skip (Step 3, runtime decision)

Without the toggle, the layout solve would run every frame text changes (twice
today's count). The skip keeps the common case free: on a text edit, re-measure
that element (the cheap per-element measure via `ShapedTextCache`, NOT the
whole-tree solve); if the measured width/height equals the cached size, route to
the existing cheap `LayoutTreeChange::VisualOnly` regenerate-commands path
(`compute_layout.rs:121-127`) and skip the engine solve; else full solve. Stress
labels (`"NN MMM"`, fixed 6 chars) measure identical every frame → always
skippable. This is content-agnostic — works whether the width is fixed by
declaration or stable by content.

Keep the two "measurements" distinct: the `solve` ms row is human verification;
the per-element measured size is the runtime skip decision.

### Incremental plan (measure as you go)

- **Step 0 — Layout breakdown instrumentation.** DONE 2026-06-03. Added `setup` /
  `scale` / `solve` / `commit` sub-rows under `layout` in `DiegeticPerfStats` + the
  `diegetic_text_stress` overlay; removed the now-tiny `mesh` pack/upload/material
  sub-rows. 375 tests pass. Baseline recorded WITH the flip-flop still in place (see
  the ⊕ note on the table). Result: `setup` (the per-frame cache clone) is the
  dominant share of `compute_ms` — bounded (the 3-digit wrap caps distinct strings
  at ~100k), plateauing ~2 ms steady / ~3 ms 5s-max. Confirms the prime suspect:
  Step 1a removes the clone (`Arc<Mutex>`-share the cache); Step 1b removes the
  flip-flop for correctness.
  - `setup` = `Arc::new(Mutex::new(cache.clone()))` + measure-closure
    (`compute_layout.rs:61-77`); confirmed dominant cost — the `ShapedTextCache`
    clone runs every frame over a cache that fills to its bounded steady size (the
    3-digit wrap caps distinct strings) then plateaus.
  - `scale` = `scaled_tree_cache.get_or_update` (`:114-119`).
  - `solve` = `engine.compute` + the VisualOnly regen (`:121-135`).
  - `commit` = field collection + `set_result_with_fields` (`:137-151`).
- **Step 1a — Kill the clone** (`Arc<Mutex>`-share `ShapedTextCache`). DONE
  2026-06-03. Moved the cache's two maps behind one internal `Arc<Mutex<…>>`; the
  layout pass clones the handle (refcount bump) into the `'static` measure closure
  and its cache-miss inserts now persist; methods are `&self`, and the renderer +
  overlay paths hold the cache as `Res`. Measured (see ⊗ on the table): `setup`
  ~2 ms → ~0 (max 0.001 ms), `compute_ms` 0.14 ms mean / 0.21 ms max, flip-flop
  still in place. 259 crate tests pass; clippy (nursery + pedantic) clean. STOP for
  user review before Step 1b.
- **Step 1b — Remove the flip-flop** (tree-authoritative + the `TextEdit` API). DONE
  2026-06-03. Deleted `ReconcileOwned` + `sync_run_text_to_cache` +
  `clear_reconcile_owned`; added the `TextEdit` cursor (kept the public API — option A,
  not a return-based closure) and routed `PanelText` / `DiegeticTextMut` writes to the
  tree through it. `PanelText` dropped its embedded `PanelTextReader` to avoid a
  `&DiegeticPanel` / `&mut DiegeticPanel` conflict; run resolution moved to the free
  `resolve_run_entity`. The no-op-no-relayout guard moved into `TextEdit::set_text`
  (read-compare before the `&mut` borrow). 258 crate tests pass; clippy clean;
  workspace builds; lagrange examples compile unchanged (closure binds `&mut TextEdit`
  by inference).
- **Step 2 — Measure the regression.** DONE 2026-06-03 (see ⊘ on the table).
  `compute_panels` 100 / 0 → **steady 100 every frame**. Quiet-machine readout
  (overlay `now / 5s-max`): `layout` 0.15 / 0.69 ms, `solve` 0.08 / 0.24 ms. The
  per-frame cost is essentially unchanged from 1a (layout `now` 0.15 vs 0.14) — the
  regression is purely **frequency** (every frame vs every other), so per-frame-
  averaged layout CPU roughly doubled. An earlier same-session reading (`solve` 0.31,
  `layout` 0.38) was concurrent-build contention; discard it. This is what the Step 3
  skip must recover. STOP for user review before Step 3.
- **Step 3 — Geometry-stable skip.** DONE 2026-06-03 (see ⊙ on the table). The
  text-edit path records `VisualOnly` (`TextEdit` →
  `DiegeticPanelChangeClassification::note_text_edit`); `compute_panel_layouts`
  gates the `VisualOnly` → `regenerate_commands` branch on a new
  `LayoutResult::can_reuse_geometry` probe (structure + viewport + per-leaf
  measured width bit-identical, no wrapped leaf, no new newline). Restyle / resize
  stay full solves, so the skip never renders stale geometry. Engine unit tests
  cover the three guards; full crate suite + clippy (nursery + pedantic) green.
- **Step 4 — Measure the win.** DONE 2026-06-03 (see ⊙ on the table). `solve`
  0.08 → ~0.02, `commit` 0.02 → ~0, `layout` 0.15 → ~0.10 — below the flip-flop-era
  per-frame cost, with no reflow lag, no marker, and no per-frame cache clone.
  `compute_panels` stays 100 (the cheap path is still counted; the win is per-pass).

### Step 0 handoff — fresh-agent start guide (Step 0 is now DONE — kept as the record of what was edited)

Everything below is verified against the code at HEAD `bb51603` (option C / glyph
atlas, after `enh/showcase-example` merged into `update/0.19.0-rc.2`). A fresh
agent should re-confirm line numbers with `rg` before editing —
they drift. Options A and C are landed and committed; **Step 0 landed 2026-06-03**
(uncommitted working-tree edits at the time of writing) — the edit map below is the
record of those changes, and Step 1a is the next edit.

**Orientation.** This work is on `update/0.19.0-rc.2`. The `enh/showcase-example`
line — where options A and C landed (it diverged at `8d5f2d1`) — has been merged
back in, so there is no separate branch to track. Step 0 is pure instrumentation —
it adds and removes perf rows and changes no runtime behavior, so it is safe to
land and measure before the structural Step 1b.

**Run & measure.**
- Release is required (the Baseline conditions): `cargo run --release --example
  diegetic_text_stress -p bevy_diegetic`, or launch over BRP.
- The example is built on `fairy_dust::sprinkle_example().with_perf_mode()`
  (`examples/diegetic_text_stress.rs:208`), which uncaps vsync and the unfocused
  winit throttle so the frame time is the true per-frame cost. `with_brp_extras`
  pulls in `FrameTimeDiagnosticsPlugin` (the overlay's fps / ms source). The
  bottom-left overlay reads `DiegeticPerfStats` directly and shows a `now` column
  and a 5-second peak (`5s max`) column.
- `Space` pauses per-frame mutation → the idle floor (diegetic CPU ≈ 0, pure
  render).
- Read the layout rows (`setup` / `scale` / `solve` / `commit`) and `compute_ms`
  off the **`5s max`** column: the `ReconcileOwned` flip-flop still zeros them on
  alternate frames, so the moving average reads half the real cost. The `5s max`
  column is the with-flip-flop baseline you record.
- The fps / ms / `remainder` rows are fill-rate-bound and scale with window size
  (`with_save_window_position` restores the last window). Compare the
  window-independent CPU rows across runs, never the absolute frame time.

**Step 0 edit map (four files, verified line refs).**

1. `crates/bevy_diegetic/src/panel/perf.rs`
   - `DiegeticPerfStats` (`:29-37`): add `compute_setup_ms`, `compute_scale_ms`,
     `compute_solve_ms`, `compute_commit_ms: f32` alongside `compute_ms`.
   - `PanelTextPerfStats` (`:52-91`): remove `mesh_pack_ms` (`:81`),
     `mesh_upload_ms` (`:85`), `mesh_material_ms` (`:88`), and rewrite the
     `mesh_build_ms` doc comment that names them (`:73-76`).
   - `publish_perf_diagnostics` (`:124-142`) and the `DIAG_*` constants stay as
     they are. The overlay reads the resource directly, so the new sub-rows need
     no `DiagnosticPath`. The three removed fields were never published, so no
     `DIAG_*` reference breaks (confirm with `rg`).

2. `crates/bevy_diegetic/src/panel/compute_layout.rs` — instrument
   `compute_panel_layouts`. Four stages, one runs once and three run per panel:
   - `setup` = `cache.clone()` + the `cached_measure` closure build (`:61-77`),
     **once**, before the panel loop → a single `Instant`.
   - `scale` = `scaled_tree_cache.get_or_update` (`:114-119`).
   - `solve` = the `VisualOnly` regen branch **and** `engine.compute` (`:121-135`).
   - `commit` = `collect_panel_field_records` → `set_result_with_fields`
     (+ `set_content_size`) (`:137-151`).
   - Write all four into `perf` next to `compute_ms` / `compute_panels`
     (`:154-159`), each gated to `0.0` when `panel_count == 0`, exactly like
     `compute_ms`.

3. `crates/bevy_diegetic/src/render/panel_text/mesh_spawning.rs` — delete the
   mesh sub-timing that Step 0 retires:
   - the accumulators `pack_time` / `upload_time` / `material_time` (`:90-92`);
   - every per-iteration `Instant::now()` + `.elapsed()` add for them (`:100`,
     `:102`, `:113`, `:115`, `:128`, `:130`, `:133`, `:174`);
   - the three perf writes (`:188-190`).
   - Keep `mesh_build_ms` (`:186-187`) and `total_ms` (`:191`). `mesh_build_start`
     still uses `Instant`, so the `Instant` import stays; the `Duration` import
     likely goes unused after the cut — drop it if the compiler flags it.

4. `crates/bevy_diegetic/examples/diegetic_text_stress.rs` — the overlay:
   - `[MetricRow; 9]` → `[; 10]` (`:100`); `INITIAL_METRICS: [&str; 9]` → `[; 10]`
     (`:138`).
   - Replace the three `SubStage` rows `pack` / `upload` / `material` (`:121-132`)
     with four `SubStage` rows `setup` / `scale` / `solve` / `commit`, and move
     them to sit directly under the `layout` row (`:109-112`). The `mesh` row
     (`:117-120`) becomes a plain `TopLevel` row with no children.
   - `PerfSnapshot` (`:165-189`): swap `pack_ms` / `upload_ms` / `material_ms` for
     `setup_ms` / `scale_ms` / `solve_ms` / `commit_ms`; keep `mesh_ms` /
     `remainder_ms`. Update `ZERO` (`:185-189`).
   - Snapshot read (`:489-504`): read the four off `diegetic_perf.compute_setup_ms`
     etc.; delete the three `mesh_*` reads (`:501-503`).
   - mean (`:535-539`, `:587-591`), peak (`:546-550`, `:603-607`), and the window
     accumulate (`:575-579`): swap the three fields for four.
   - Retarget the doc comments that say sub-stages sit under `mesh` to `layout`:
     `RowIndent` (`:80-86`), `MetricRow` (`:88-94`), `METRIC_ROWS` (`:96-99`), and
     `SUB_ROW_INDENT` (`:71-72`). (The module-level doc comment was rewritten in the
     merge and no longer describes mesh sub-stages, so it needs no retarget.)
     `remainder` (8 chars) stays the longest label, so `LABEL_COLUMN_WIDTH` (`:70`)
     needs no change.

**The timing detail that will trip you.** `setup` is measured once with a single
`Instant`, but `scale` / `solve` / `commit` run per panel inside the loop, so each
needs a `Duration` accumulator summed across panels — the same pattern being
deleted from `mesh_spawning.rs`, moved over to layout. Watch the two early
`continue`s: the `Identical` skip (`:108-112`) returns before `scale`, and the
`VisualOnly` branch (`:121-127`) does `scale` then regenerates and `continue`s, so
its elapsed time must be added to the `solve` accumulator **before** the `continue`
or skip-heavy frames undercount.

**Verify.**
- Shut down the BRP app before building (build-dir lock).
- `rg -n "mesh_pack_ms|mesh_upload_ms|mesh_material_ms" crates/` returns nothing.
- `cargo build && cargo +nightly fmt`.
- `cargo nextest run` — 257 tests are green at HEAD; none reference the removed
  fields, so expect 257 still green.

**Record & stop.** Add four sub-rows under `layout` (`setup` / `scale` / `solve` /
`commit`) to the Baseline table above and fill the current column from the
`5s max` overlay values. Because Step 0 changes only instrumentation, that column
is the After-C runtime state with the flip-flop still in place — the baseline the
rest of Phase D measures against. Then STOP for user review before Step 1a.
(Done 2026-06-03: baseline recorded, plan split into 1a/1b, reviewed and approved.)

**Open question carried into Step 1b (the API change, not Step 0 or 1a).** The
tree-edit handle name is still unsettled — `TextEdit` / `TextCursor` /
`PanelTextEdit`. The user picks before Step 1b lands. It is a new public type, so
this is the user's editor-global rename to make. (Step 1a touches no public type and
needs no name decision.)

## Suggested order

A (done) → C (done) → D (current phase: Step 0 done; next 1a clone fix → 1b
flip-flop removal → 2 measure → 3 skip → 4 measure) → B.

Detail also in the memory notes `project_diegetic_text_single_source`,
`project_diegetic_text_perf_targets`, and `project_perf_mode_measurement`.
