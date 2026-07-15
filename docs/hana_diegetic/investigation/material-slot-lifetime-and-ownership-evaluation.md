# Material Slot Lifetime And Ownership Evaluation

> **Status: EVALUATION COMPLETE.** Use this decision before continuing the SDF
> material table batching plan. The result decides the material-slot ABI,
> ownership model, scheduler constraints, and producer migration shape.

## Goal

Prove the simplest correct material-slot model for shared SDF, text, and
panel-shape batching.

This evaluation answers four questions:

1. Can GPU records safely store a bare `MaterialSlotId`, or do they need a
   generational `MaterialSlotRef`?
2. Can proper Bevy system ordering prevent stale material-slot reads without
   broad producer serialization?
3. Should ownership cleanup be table-owned key maps, Bevy relationships, or
   relationship-backed slot entities?
4. What exact lifetime, ownership, and scheduler rules should the batching plan
   use afterward?

## Current Code Facts

- Text runs already use Bevy relationships:
  - `crates/hana_diegetic/src/render/panel_text/relationship.rs`
  - `TextRunOf` is the source component on each text run.
  - `PanelTextRuns` is Bevy-maintained on the panel.
  - `ChildOf(panel)` remains responsible for transform propagation and despawn;
    the relationship is a typed traversal index.
- Panel shapes are currently store-owned:
  - `crates/hana_diegetic/src/render/panel_shapes/batching.rs`
  - `ShapeBatchStore` owns `panel_members: HashMap<Entity, Vec<_>>`.
  - `remove_panel(panel)` removes retained shape records from batches.
  - `PanelShapeRenderKey` is `(panel, PanelShapePrimitiveKey)`.
- SDF panel surfaces are currently entity-backed only through the old
  per-surface quad path:
  - `PanelSdfMesh` / `PanelSdfSurface` are old-route entities/components.
  - The future batched SDF route should not depend on keeping those entities.
- Current analytic material identity is value-interned:
  - `VisualMaterialInterner` assigns `BaseMaterialId` from material values.
  - Shared material slots must be source identity, not material-value identity.

## Non-Goals

- Do not implement SDF batching here.
- Do not migrate text or panel-shape producers here.
- Do not update `as-built/material-table-batching.md` until this evaluation has a
  result.
- Do not keep a throwaway workspace member after the evaluation unless its tests
  are renamed into durable regression tests.

## Implementation Plan

### Step 1: Build A Minimal Probe

Preferred location:

- `crates/hana_diegetic/src/render/material_slot_lifetime_probe.rs`
- Gate it with `#[cfg(test)]`.

Use a temporary workspace member only if an in-crate test module cannot model
the required Bevy relationship behavior cleanly.

The probe should model only the hard parts:

- a main-world source set that creates, updates, hides, and removes logical
  render sources;
- retained CPU records that store a material-slot reference;
- a shared material table with allocation, free, retire, reuse, and capacity
  growth;
- record-buffer growth/rewrite;
- material-table buffer growth/rebind timing;
- extraction/prepare-style staged snapshots;
- multiple independent producer stores that can run in parallel except at
  explicitly named boundaries.

Keep the probe direct: concrete structs, explicit systems, and named test
scenarios. Avoid building a generic framework whose complexity becomes the
answer.

### Step 2: Define Probe Types

Use names close to the intended production names:

- `MaterialSlotId`
- `MaterialSlotGeneration`
- `MaterialSlotRef`
- `MaterialSlotKey`
- `SdfSurfaceMaterialKey`
- `TextRunMaterialKey`
- `PanelShapeMaterialKey`
- `MaterialSlotValues`
- `RetainedRecord`
- `ProducerStore`
- `RegisteredBatchMaterial`

`MaterialSlotKey` should have variants for:

- `SdfSurface(SdfSurfaceMaterialKey)`
- `TextRun(TextRunMaterialKey)`
- `PanelShape(PanelShapeMaterialKey)`

The key represents source ownership. The slot id/ref represents the table row
that a retained GPU record reads.

### Step 3: Test Two Lifetime Strategies

Run the same scenarios against both strategies.

#### Simple Strategy

GPU records store:

```rust
MaterialSlotId
```

Freed slots enter a retirement queue and are not reused until the proven safe
boundary. Correctness depends on named scheduler ordering and bounded delayed
reuse.

#### Generation Strategy

GPU records store:

```rust
MaterialSlotRef {
    id: MaterialSlotId,
    generation: MaterialSlotGeneration,
}
```

Each table row stores its current generation. Reusing a slot increments the
generation. A stale record is rejected by the shader/helper equivalent if the
record generation does not match the row generation.

### Step 4: Test Ownership Strategies

Evaluate ownership separately from lifetime.

#### Table-Owned Key Map

`SharedMaterialTable` owns:

- `HashMap<MaterialSlotKey, SlotRefOrId>`
- owner-to-keys index for cleanup, for example panel to keys or producer to keys
- explicit `finish_scope(owner, live_keys)` cleanup

This is the baseline. It is simple and source-agnostic.

#### Relationships For Existing Entity-Backed Sources

Use Bevy relationships where the source already is an entity.

Text runs are the reference case because the code already has
`TextRunOf` / `PanelTextRuns`.

This variant should answer:

- Can relationship removal clean text-run slots without a broad scan?
- Does the relationship replace a real cleanup map, or only duplicate
  `MaterialSlotKey` lookup?
- Does it keep `ChildOf` as the transform/despawn owner, like the current text
  relationship does?

#### Relationship-Backed Slot Entities

Create material-slot entities only as an explicit experiment.

This variant is rejected by default unless it clearly simplifies cleanup and
does not create unacceptable churn. It must be tested against SDF surfaces and
panel-shape primitives, because those are not currently durable slot entities.

## Required Scenarios

Every lifetime strategy must pass these scenarios:

- remove source A, create source B, and pressure slot reuse in the same app
  update;
- remove A, grow table capacity, and grow/rewrite record buffers in the same
  frame;
- material-only edit plus material-table capacity growth plus registered
  batch-material rebind;
- two independent producer stores where one store can lag while another
  allocates, frees, or reuses slots;
- stale retained record from A attempts to read a row reused by B;
- hidden, despawned, clipped, and removed sources free or park slots without
  changing unrelated layout;
- source material changes refresh table rows even when geometry/layout did not
  change.

Ownership variants must also pass these scenarios:

- text run entity despawn removes or retires its slots;
- text run reuse does not mutate relationship targets unnecessarily;
- panel removal removes all panel-shape slots;
- panel-shape primitive removal removes only the affected slots;
- SDF surface removal does not require retaining the old per-surface quad
  entity model;
- optional slot entities, if tested, do not create per-frame entity churn for
  ordinary material edits.

## Scheduler Evidence To Record

For each passing strategy, record the exact system ordering it needs:

- system names;
- system sets;
- `.before(...)`, `.after(...)`, or `.chain()` edges;
- which systems can run in parallel;
- which systems must be serialized;
- whether serialization is producer-local or global;
- where extraction observes table records and registered material handles;
- where table-buffer growth and material rebind happen.

The evaluation must make the parallelism cost visible. A simple strategy that
requires all producers to serialize behind one global rebuild step is not
automatically simpler.

## Decision Criteria

### Lifetime Decision

Choose bare `MaterialSlotId` only if all are true:

- all required stale-record and same-frame reuse scenarios pass;
- stale reads are prevented by narrow, understandable scheduler ordering;
- freed ids have a bounded retirement rule;
- producers are not broadly serialized together;
- shader code does not need a generation branch/check;
- the implementation has fewer moving parts than generations.

Choose generational `MaterialSlotRef` only if any are true:

- bare `MaterialSlotId` fails a required scenario;
- bare `MaterialSlotId` requires broad producer serialization;
- bare `MaterialSlotId` requires fragile ordering across extraction, buffer
  growth, and material rebind;
- the generation check is simpler than the scheduler coupling needed by bare
  ids.

If both pass, choose the simpler and faster model. In practice that means bare
`MaterialSlotId` unless the scheduler proof is brittle.

### Ownership Decision

Choose table-owned key maps if:

- they pass all cleanup scenarios;
- they keep non-entity sources simple;
- relationships would duplicate, not replace, the real lookup/cleanup state.

Choose relationships for existing entity-backed sources if:

- they remove a real cleanup map or scan;
- they follow the current `TextRunOf` / `PanelTextRuns` pattern;
- they do not take over `ChildOf`'s transform/despawn responsibility;
- they do not force SDF surfaces or panel-shape primitives to become entities.

Choose relationship-backed slot entities only if:

- tests show they materially reduce cleanup complexity;
- entity churn is bounded and not per material-value edit;
- they work for SDF, text, and panel shapes without special-case cleanup paths;
- they are simpler than table-owned key maps plus existing relationships.

Otherwise reject slot entities.

### Scheduler Decision

Prefer the strategy with:

- local producer reconciliation in parallel;
- one narrow shared table allocation/rebind boundary if needed;
- explicit ordering before extraction;
- no frame where material assets point at stale table buffers;
- no dependence on undocumented Bevy scheduling behavior.

Reject any strategy that is correct only because all producers run in one long
serialized chain.

## Expected Output

At the end of this evaluation, update this document with:

- the chosen lifetime strategy;
- the chosen ownership strategy;
- the exact scheduler ordering;
- the test names that prove the decision;
- rejected alternatives and why they were rejected;
- the required updates to `as-built/material-table-batching.md`.

Only after this document has that result should the batching implementation
plan be reviewed or regenerated.

## Decision

Use bare `MaterialSlotId` with two-frame delayed reuse.

Do not add `MaterialSlotGeneration` or `MaterialSlotRef` to the first
production implementation.

This is the chosen lifetime rule:

- GPU records store `MaterialSlotId`.
- Freed slot ids enter a retirement queue.
- A freed id is not reusable until two subsequent material-table apply frames
  have passed.
- Reuse is controlled only by the shared table allocator.
- Production stats must expose live slots, retired slots, and table capacity.

This is the chosen ownership rule:

- `MaterialSlotKey` is source identity.
- `MaterialSlotId` is table-row identity.
- `SharedMaterialTable` owns:
  - `MaterialSlotKey -> MaterialSlotId`;
  - `SourceOwner -> MaterialSlotKey` membership;
  - slot values;
  - the retirement queue.
- Producers submit material requests locally.
- Producers do not borrow `SharedMaterialTable` directly.
- One shared apply step allocates, updates, frees, and retires slots.

Use Bevy relationships only as source traversal aids where source entities
already exist. Text runs are the current example: `TextRunOf` / `PanelTextRuns`
can identify which text-run entities are still live, but the material table
still owns material slot lookup and cleanup.

Rejected for the first implementation:

- Generational slot refs. They solve stale records and reduce churn capacity,
  but add ABI and shader complexity. The current evidence does not justify that
  cost.
- Material-slot entities. They add entity churn and still need table-style
  key/slot state unless they reimplement allocator semantics in ECS.
- Per-SDF-surface or per-panel-shape-primitive entities. Source keys handle the
  required cleanup without preserving the old SDF quad entity model or adding
  new primitive entities.

Required scheduler ordering:

- Producer request collection runs before the shared apply step.
- The shared apply step runs after all producer request collection.
- Registered material-table rebind runs after the shared apply step.
- Extraction runs after registered material-table rebind.
- Render-world material-table upload/prepare reads an extracted state whose
  registered materials already point at the current table buffer handle.

Required batching-plan updates:

- Replace any `MaterialSlotRef`/generation requirement with bare
  `MaterialSlotId` plus two-frame delayed reuse.
- Specify producer-local request collection instead of direct producer mutation
  of `SharedMaterialTable`.
- Specify table-owned owner maps and `finish_scope(owner, live_keys)` cleanup.
- Specify `free_owner(owner)` for panel/source removal cleanup.
- Keep Bevy relationships limited to existing entity-backed source traversal.
- Add production stats and stress coverage for live slots, retired slots,
  capacity, and churn.

Decision evidence:

- `immediate_reuse_with_bare_slot_id_exposes_reused_row_to_stale_record`
  proves immediate bare-id reuse is unsafe.
- `two_frame_retirement_blocks_previous_extraction_snapshot_from_reused_row`
  proves the selected retirement boundary in the modeled lag scenario.
- `scheduled_rebind_after_allocation_keeps_extraction_consistent_on_growth`
  proves the required apply/rebind/extract ordering.
- `scheduled_extraction_before_rebind_observes_stale_material_binding_on_growth`
  proves the inverse ordering is unsafe.
- `delayed_reuse_capacity_stabilizes_under_full_slot_churn` and
  `material_only_stress_does_not_grow_delayed_reuse_table` prove the capacity
  bound and show material edits do not create table growth.
- `slot_entity_model_creates_topology_churn_entities` rejects material-slot
  entities as the baseline ownership model.

## Findings

### Iteration 1 — Minimal Lifetime And Ownership Probe

**Implemented probe:** `crates/hana_diegetic/src/render/material_slot_lifetime_probe.rs`

**Verification:**

- `cargo nextest run -p hana_diegetic material_slot_lifetime_probe` passed 6
  tests.
- `cargo clippy -p hana_diegetic --all-targets` passed.
- `cargo +nightly fmt --all -- --check` was attempted, but the workspace is
  not currently fmt-clean in unrelated layout/panel files. Those unrelated
  rustfmt changes were not kept in this evaluation patch.

Tests added:

- `immediate_reuse_with_bare_slot_id_exposes_reused_row_to_stale_record`
- `delayed_reuse_keeps_bare_slot_id_from_reading_new_source`
- `generation_ref_rejects_stale_record_after_same_frame_reuse`
- `delayed_reuse_handles_independent_producer_stores_without_record_rewrite`
- `table_owned_scope_frees_removed_owner_keys_without_touching_other_owners`
- `bevy_relationship_tracks_entity_backed_runs_without_slot_entities`

Findings so far:

- The stale-record scenario is real in the minimal model. If a bare
  `MaterialSlotId` is freed and reused in the same frame, a retained stale GPU
  record can read the new source's `MaterialSlotValues`.
- A bare `MaterialSlotId` can avoid that stale read if freed ids enter a
  retirement queue and cannot be reused until a proven safe boundary.
- A generational `MaterialSlotRef` rejects the same stale record even when the
  physical row is reused immediately.
- Delayed bare-id reuse can protect a lagging producer store from reading
  another producer's new row, as long as all producers allocate through the
  same retirement rule.
- Table-owned key maps plus `finish_scope(owner, live_keys)` can free removed
  panel-shape slots without touching SDF or text slots.
- Bevy relationships correctly maintain an entity-backed source set without
  creating material-slot entities. This supports using relationships for
  sources that are already entities, like text runs.

What this does not prove yet:

- It does not prove the exact production scheduler edges around extraction,
  render-world prepare, table-buffer growth, and registered material rebind.
- It does not prove the needed retirement frame count for production.
- It does not justify relationship-backed material-slot entities.
- It does not prove that SDF surfaces or panel-shape primitives should become
  entities.

Current decision pressure:

- Do not choose generations yet. The first probe shows generations work, but it
  also shows bare ids are viable if the retirement boundary is narrow and
  explicit.
- Keep table-owned key maps as the baseline ownership model.
- Use relationships only where source entities already exist, unless a later
  probe demonstrates that slot entities reduce complexity more than they add.

### Iteration 2 — Scheduled Producer/Rebind/Extraction Probe

**Implemented probe extension:** `crates/hana_diegetic/src/render/material_slot_lifetime_probe.rs`

**Verification:**

- `cargo nextest run -p hana_diegetic material_slot_lifetime_probe` passed 9
  tests.
- `cargo clippy -p hana_diegetic --all-targets` passed.

Tests added:

- `scheduled_rebind_after_allocation_keeps_extraction_consistent_on_growth`
- `scheduled_extraction_before_rebind_observes_stale_material_binding_on_growth`
- `scheduled_material_edit_updates_row_without_rebind_or_pointer_change`

Scheduled model tested:

- producer systems collect SDF, text, and panel-shape material requests into
  producer-local request resources;
- one shared `apply_material_requests` boundary owns `SharedMaterialTable`
  mutation and owner-scope cleanup;
- `rebind_registered_materials` runs after allocation and cleanup;
- `extract_probe_snapshot` runs after rebind in the correct schedule;
- `prepare_probe_snapshot` verifies the extracted material binding capacity and
  live record reads.

Findings so far:

- Producer request collection can avoid borrowing `SharedMaterialTable`, so
  SDF/text/panel-shape producer systems do not need to serialize on the table
  itself.
- The shared serialized boundary can be narrow: apply material requests, update
  owner scopes, then rebind registered materials if table capacity changed.
- Rebind-before-extraction is a real ordering requirement. The intentionally
  wrong schedule extracts the old bound table capacity after table growth, even
  though live records still read valid rows.
- Material-only edits to an existing source update `MaterialSlotValues` without
  changing the record pointer or rebinding registered materials when capacity is
  unchanged.

Current decision pressure:

- Bare `MaterialSlotId` with delayed reuse still looks viable. The added
  scheduler evidence shows the table mutation/rebind requirement can be a
  narrow boundary, not a full producer-chain serialization.
- The production plan should separate producer-local request collection from
  shared table allocation. Having each producer directly take
  `ResMut<SharedMaterialTable>` would unnecessarily serialize them.
- Rebind-before-extraction must become a named invariant in the implementation
  plan and tests.

Still unproven:

- The exact retirement boundary remains unproven. The probe uses a two-frame
  delay as a conservative placeholder, not a final production number.
- Render-world prepare is still simulated in main-world test systems; a later
  pass should model Bevy extraction/prepare boundaries more closely if the
  simple-id decision depends on a specific frame count.
- Slot entities are still unjustified.

### Iteration 3 — Retirement Boundary Probe

**Implemented probe extension:** `crates/hana_diegetic/src/render/material_slot_lifetime_probe.rs`

**Verification:**

- `cargo nextest run -p hana_diegetic material_slot_lifetime_probe` passed 12
  tests.
- `cargo clippy -p hana_diegetic --all-targets` passed.

Tests added:

- `one_frame_retirement_allows_previous_extraction_snapshot_to_read_reused_row`
- `two_frame_retirement_blocks_previous_extraction_snapshot_from_reused_row`
- `generation_ref_survives_record_buffer_lag_without_retirement`

Scenario modeled:

- Frame 1 produces source A and extracts its retained record.
- Main-world reconcile frees A and creates source B before the old extracted
  record is assumed fully gone.
- The old extracted record is then read against the later material table.

Findings:

- A one-frame retirement delay is not sufficient under a one-frame extracted
  record lag model. The old extracted record can read source B's row after the
  row is reused.
- A two-frame retirement delay protects the old extracted record in the same
  model. The old record reads no live row, and the retired id becomes reusable
  after the modeled lag boundary.
- A generational `MaterialSlotRef` also protects the old extracted record while
  allowing immediate physical row reuse.

Current decision pressure:

- Bare `MaterialSlotId` is still viable, but it now has a concrete cost: freed
  ids need a named two-frame retirement rule unless a later Bevy-specific proof
  shows a shorter safe boundary.
- The two-frame rule is simple and isolated in the allocator, so it may still
  be less complex than adding a generation field to every record and shader
  table row.
- Generations remain the stronger safety mechanism if production needs
  immediate reuse or if real render-world lag can exceed the modeled two-frame
  boundary.

Next proof needed:

- Model actual Bevy extraction/render-world retention more closely, or inspect
  the relevant Bevy asset/extract timing, before turning "two frames" into a
  production rule.
- Decide whether the memory cost of delaying id reuse is acceptable for
  material slots. If it is, bare ids plus delayed reuse are trending toward the
  simpler answer.

### Iteration 4 — Bevy Lag Inspection And Churn Stress

**Implemented probe extension:** `crates/hana_diegetic/src/render/material_slot_lifetime_probe.rs`

**Bevy source inspected:**

- `bevy_render-0.19.0-rc.3/src/extract_plugin.rs`
- `bevy_render-0.19.0-rc.3/src/render_asset.rs`
- `bevy_render-0.19.0-rc.3/src/pipelined_rendering.rs`
- `bevy_render-0.19.0-rc.3/src/lib.rs`

Relevant Bevy facts:

- `ExtractSchedule` copies data from the main world into the render world.
- Extract commands are applied during the render schedule, not during
  extraction.
- `RenderAsset` extraction happens in `ExtractSchedule`; GPU preparation
  happens afterward in `RenderSystems::PrepareAssets`.
- `prepare_assets` can carry extracted assets forward in
  `PrepareNextFrameAssets` when preparation is deferred.
- Pipelined rendering moves `RenderApp` to a render thread; main-world update
  and render-world work can overlap by design.

Interpretation:

- A previous extracted/prepared record must be treated as able to outlive the
  main-world frame that freed its source.
- The two-frame retirement model is conservative but justified by Bevy's
  extraction/prepare split and pipelined rendering.
- A one-frame retirement rule is too optimistic unless production code proves
  stronger local constraints than Bevy's general render pipeline provides.

**Verification:**

- `cargo nextest run -p hana_diegetic material_slot_lifetime_probe` passed 15
  tests.
- `cargo clippy -p hana_diegetic --all-targets` passed.

Tests added:

- `delayed_reuse_capacity_stabilizes_under_full_slot_churn`
- `generation_capacity_stays_at_live_count_under_full_slot_churn`
- `material_only_stress_does_not_grow_delayed_reuse_table`

Stress findings:

- With two-frame delayed reuse, full topology churn stabilizes at:

  ```text
  live slots * (retirement_frames + 1)
  ```

- In the current stress test that is `128 * 3 = 384` table rows for 128 live
  slots under continuous full churn.
- With generational refs, the same full-churn test stabilizes at the live slot
  count because physical rows can be reused immediately.
- Material-only edits do not grow the delayed-reuse table. They update existing
  slot values in place.

Decision pressure:

- Bare `MaterialSlotId` plus two-frame delayed reuse is now the preferred
  strategy unless a later implementation detail makes the `3x` worst-case churn
  capacity unacceptable.
- The `3x` bound applies to extreme full topology churn, not material
  animation.
- Generations buy immediate row reuse and a smaller table under churn, but they
  add a generation field to CPU records, GPU records, table rows, and shader
  validation.
- Given the stated priority on simplicity, the current evidence favors bare
  `MaterialSlotId` with:
  - producer-local request collection;
  - one shared apply/rebind boundary;
  - rebind before extraction;
  - two-frame slot retirement;
  - stress stats proving live, retired, and capacity counts.

Remaining condition before finalizing:

- The production implementation plan should require material-table stats and a
  stress example or test that exposes live slots, retired slots, capacity, and
  churn behavior. That keeps the delayed-reuse memory cost visible.

### Iteration 5 — Ownership Stress

**Implemented probe extension:** `crates/hana_diegetic/src/render/material_slot_lifetime_probe.rs`

**Verification:**

- `cargo nextest run -p hana_diegetic material_slot_lifetime_probe` passed 21
  tests.
- `cargo clippy -p hana_diegetic --all-targets` passed.

Tests added:

- `relationship_run_removal_guides_text_slot_cleanup_without_slot_entities`
- `text_run_reuse_updates_values_without_relationship_or_slot_churn`
- `panel_shape_scope_cleanup_frees_only_removed_primitives`
- `panel_owner_cleanup_frees_shape_and_sdf_slots_without_text_slots`
- `sdf_scope_cleanup_does_not_need_surface_entities`
- `slot_entity_model_creates_topology_churn_entities`

Findings:

- Table-owned owner maps plus `finish_scope(owner, live_keys)` clean up
  panel-shape primitive removal without touching unrelated shape slots.
- Table-owned `free_owner(owner)` cleanly handles panel-wide cleanup for SDF
  and panel-shape slots.
- SDF cleanup does not require keeping or replacing the old per-surface
  `PanelSdfSurface` entity model. Source keys are enough.
- Bevy relationships help with entity-backed source traversal. The text-run
  relationship identifies which run entities remain after despawn.
- Relationships do not replace the material table's key map. The table still
  needs key-to-slot and owner-to-key state to free/update material slots.
- Text run reuse updates `MaterialSlotValues` in place without mutating the
  relationship set or allocating a new slot.
- Relationship-backed slot entities create entity churn under topology churn
  unless they reimplement table-style reuse. That makes them a worse baseline
  than table-owned keys.

Ownership decision:

- Use table-owned `MaterialSlotKey` maps as the baseline ownership model.
- Use Bevy relationships only for source traversal where source entities
  already exist, such as text runs.
- Do not create material-slot entities.
- Do not create per-SDF-surface or per-panel-shape-primitive entities for this
  material-table design.

Lifetime decision:

- Start with bare `MaterialSlotId` plus two-frame delayed reuse.
- Keep material-table stats and stress tests so live, retired, and capacity
  behavior are visible.
- Do not add generational `MaterialSlotRef` now. If production evidence later
  shows delayed-reuse churn is too high or the retirement boundary is
  insufficient, optimize then with a generation scheme.

Implementation implications for the batching plan:

- Producers collect material requests locally and do not borrow
  `SharedMaterialTable`.
- A shared apply step allocates/frees slots, updates `MaterialSlotValues`, and
  maintains owner maps.
- Registered material rebind runs after the shared apply step and before
  extraction.
- `MaterialSlotKey` remains source identity; `MaterialSlotId` remains the row
  stored in GPU records.
