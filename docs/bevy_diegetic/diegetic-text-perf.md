# Diegetic text performance — options

## Baseline

Conditions: `diegetic_text_stress`, 100 world labels each restrung every frame,
M2 Max, release, `with_perf_mode` (AutoNoVsync + `WinitSettings::continuous`).
Add a column after each phase lands.

| Metric (moving unless noted)       | Baseline (2026-06-02) | After A     | After C      | After D | After B |
| ---------------------------------- | --------------------- | ----------- | ------------ | ------- | ------- |
| Frame time ‡                       | ~25 ms                | ~24 ms      | ~18.5 ms ‡   |         |         |
| FPS ‡                              | 40                    | 42          | 55 ‡         |         |         |
| Layout `compute_ms` (alt. frames)  | 0 / 5.8 ms            | 0 / 5.8 ms  | 0 / 5.8 ms   |         |         |
| Text `panel_text.total_ms`         | 2.4 ms                | 1.6 ms      | ~0.45 ms     |         |         |
| — of which `mesh_build_ms`         | 1.8 ms                | **1.29 ms** | **0.12 ms**  |         |         |
| Render floor (remainder, moving)   | —                     | ~18 ms ‡    | ~14.7 ms ‡   |         |         |
| Paused FPS ‡                       | 98                    | ~55         |              |         |         |

‡ The frame-time, FPS, paused, and render-floor rows are fill-rate-bound and scale
with window size, and the A and C columns were captured in separate sessions; the
absolute numbers are **not** comparable across columns. The window-independent
measure is the CPU `mesh_build_ms` row: 1.8 → 1.29 (A) → **0.12** (C).

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
- Saves the alternating ~5.8 ms layout CPU and removes a 1-frame reflow lag.

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

### The fix — route writes to the tree too

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

### The geometry-stable skip (runtime decision)

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

- **Step 0 — Layout breakdown instrumentation.** Add `setup` / `scale` / `solve`
  / `commit` sub-rows under `layout` in `DiegeticPerfStats` + the
  `diegetic_text_stress` overlay; remove the now-tiny `mesh` pack/upload/material
  sub-rows. This is the measuring instrument for the rest. Baseline WITH the
  flip-flop still in place. (Decisions settled: remove mesh sub-instrumentation
  entirely; zero the layout sub-rows on skipped frames like `compute_ms` — read
  `setup` off the `5s max` column.)
  - `setup` = `Arc::new(Mutex::new(cache.clone()))` + measure-closure
    (`compute_layout.rs:61-77`); prime suspect — the `ShapedTextCache` deep-clone
    runs every frame and the maps grow unbounded under the per-frame counter.
  - `scale` = `scaled_tree_cache.get_or_update` (`:114-119`).
  - `solve` = `engine.compute` + the VisualOnly regen (`:121-135`).
  - `commit` = field collection + `set_result_with_fields` (`:137-151`).
- **Step 1 — Remove the flip-flop** (tree-authoritative + the `TextEdit` API).
  Expect `compute_panels` 100 / 0 → steady 100; `solve` every frame.
- **Step 2 — Measure the regression.** Confirm `solve` runs every frame (layout
  CPU ~2×). Quantifies what the skip must recover.
- **Step 3 — Geometry-stable skip.** Route text-only edits with unchanged
  measured size to the cheap path. Measure: `solve` should drop to firing only on
  genuine reflow.
- **Step 4 — Measure the win.** Layout CPU at or below the flip-flop baseline,
  now with no reflow lag and no marker.

### Step 0 handoff — fresh-agent start guide

Everything below is verified against the code at HEAD `bb51603` (option C / glyph
atlas, after `enh/showcase-example` merged into `update/0.19.0-rc.2`). A fresh
agent should re-confirm line numbers with `rg` before editing —
they drift. No Phase-D code has been written yet; only this doc and the two memory
notes exist. Options A and C are landed and committed; Step 0 is the next edit.

**Orientation.** This work is on `update/0.19.0-rc.2`. The `enh/showcase-example`
line — where options A and C landed (it diverged at `8d5f2d1`) — has been merged
back in, so there is no separate branch to track. Step 0 is pure instrumentation —
it adds and removes perf rows and changes no runtime behavior, so it is safe to
land and measure before the structural Step 1.

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
rest of Phase D measures against. Then STOP for user review before Step 1.

**Open question carried into Step 1 (not Step 0).** The tree-edit handle name is
still unsettled — `TextEdit` / `TextCursor` / `PanelTextEdit`. The user picks
before Step 1 lands. It is a new public type, so this is the user's editor-global
rename to make.

## Suggested order

A (done) → C (done) → D (current phase, steps above) → B.

Detail also in the memory notes `project_diegetic_text_single_source`,
`project_diegetic_text_perf_targets`, and `project_perf_mode_measurement`.
