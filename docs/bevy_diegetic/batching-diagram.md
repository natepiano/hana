# Batching Call Flow — Diagrams

> Companion to [`sdf-material-table-batching.md`](./sdf-material-table-batching.md).
> Mermaid views of the call flow the plan builds: the three render families
> (SDF fills/borders, text runs, panel shapes) converging on one shared frame
> material table, then splitting back into two batch stores and two render
> materials, both reading the same GPU table.

The plan's load-bearing idea: a batch key carries no scalar/vector PBR values.
Those live in a per-frame dense table (`FrameMaterialTable`) addressed by a
frame-local `MaterialSlotId`. Two records that differ only by table values share
a batch; they split only on pipeline/resource compatibility.

## 1. The whole flow at a glance

Three families, one append path, one table, two batch stores, two render
materials, one shared GPU table buffer.

```mermaid
flowchart TD
    sdfm["`SdfMaterial
    cascade source handle`"]
    txtm["`TextMaterial
    cascade source handle`"]
    shpm["`ShapeMaterial
    cascade source handle`"]

    sdfm --> sdfi["SdfMaterialSlotInput"]
    txtm --> txti["TextRunMaterialSlotInput"]
    shpm --> shpi["PanelShapeMaterialSlotInput"]

    sdfi --> cand
    txti --> cand
    shpi --> cand

    cand["`MaterialSlotCandidate
    values + pipeline_compat + resource_compat`"] --> builder["`FrameMaterialTableBuilder
    Phase 2 shared frame table, append order SDF, text, shape`"]
    builder --> rows["`FrameMaterialTable
    rows: Vec of MaterialSlotValues`"]

    builder -->|"MaterialSlotId"| sdfrec["`SdfRenderRecord
    fill_material, border_material`"]
    builder -->|"MaterialSlotId"| pathrec["`PathRenderRecord
    material slot (text + shapes)`"]

    sdfrec --> sdfkey["`SdfBatchKey
    to SdfBatchResources`"]
    pathrec --> pathkey["`PathBatchKey
    to PathBatchResources`"]

    sdfkey --> sdfmat["SdfExtendedMaterial"]
    pathkey --> pathmat["PathExtendedMaterial"]

    rows --> gpu["`MaterialSlotValues GPU table
    binding 106`"]
    sdfmat --> gpu
    pathmat --> gpu
    gpu --> shader["`pbr_input_from_material_table
    apply_pbr_lighting`"]
```

`StandardMaterial::depth_bias` is deliberately not part of this flow — diegetic
draw-order types own depth/OIT offsets (`DrawOrderProjection`, plus per-record
`oit_depth_offset` / `depth_nudge`).

## 2. Per-family vertical slices

Same backbone, different source identity and record type for each family. SDF
keeps its own key/material; text and panel shapes share the analytic path types.

```mermaid
flowchart TD
    sdfh["SDF fills / borders<br/>Phases 1, 3, 4"] --> s1["ResolvedSdfSurface"]
    s1 --> s2["`SdfMaterialSlotInput
    role: Fill or Border`"]
    s2 --> s3["`SdfPaintMaterial
    Authored(slot) or NotAuthored`"]
    s3 --> s4["`SdfRenderRecord
    via from_resolved()`"]
    s4 --> s5["SdfBatchKey to SdfBatchResources"]
    s5 --> s6["SdfExtendedMaterial"]

    txth["Text runs<br/>Phases 6, 8"] --> t1["PreparedPanelText / run entity"]
    t1 --> t2["TextRunMaterialSlotInput"]
    t2 --> t3["MaterialSlotId"]
    t3 --> t4["PathRenderRecord<br/>one per run"]
    t4 --> t5["PathBatchKey to PathBatchResources"]
    t5 --> t6["PathExtendedMaterial"]

    shph["Panel shapes<br/>Phases 6, 9"] --> p1["`PanelShape entity
    PanelShapeOf / PanelShapes`"]
    p1 --> p2["PanelShapeMaterialSlotInput"]
    p2 --> p3["MaterialSlotId"]
    p3 --> p4["PathRenderRecord<br/>one per primitive group"]
    p4 --> p5["PathBatchKey to PathBatchResources"]
    p5 --> p6["PathExtendedMaterial"]
```

Notes on the split lines:

- **SDF** has two material roles per surface (`Fill`, `Border`), so its record
  carries two slot fields. `SdfPaintMaterial::NotAuthored` becomes
  `INVALID_GPU_MATERIAL_SLOT` (`u32::MAX`); the shader skips the table read for
  that role.
- **Text and panel shapes share** `PathExtendedMaterial` / `PathExtension` and
  `PathBatchKey` / `PathBatchResources`. They differ only in source identity
  (`run` vs `shape`) and how many `PathRenderRecord`s one source emits
  (text: one per run; shape: one per merged primitive group).
- **Slot id is frame-local.** Every live record re-appends or refreshes its row
  and rewrites its slot id each frame, even with unchanged geometry.

## 3. Where scalar values split from compatibility

What goes in the table vs. what splits a batch — the rule that makes all three
families batch the same way.

```mermaid
flowchart TD
    src["`resolved StandardMaterial
    (behind cascade handle)`"] --> proj["`projection / classification site
    exhaustive destructure of StandardMaterialUniform`"]

    proj -->|"scalar / vector PBR values"| vals["`MaterialSlotValues (table row)
    base_color, emissive, metallic, roughness,
    reflectance, transmission, clearcoat,
    anisotropy, ior, uv_transform`"]
    proj -->|"pipeline facts"| pipe["`PipelineCompatibility
    alpha mode, double_sided, cull_mode,
    lighting/unlit, shader defs`"]
    proj -->|"resource facts"| res["`ResourceCompatibility
    texture handles, samplers,
    bind-group / UV-channel reqs`"]

    vals -->|"only changes the row"| samebatch["`records stay in one batch
    (read a different row value)`"]
    pipe -->|"any difference"| split["records move to a different batch"]
    res -->|"any difference"| split
```

## 4. Per-frame schedule order

The append window, the freeze, and the rebind-before-extract guarantee
(Phase 2, R2). Named sets and boundaries, not "freeze/commit" prose.

```mermaid
sequenceDiagram
    participant P as Producers (SDF, text, shape)
    participant B as FrameMaterialTableBuild
    participant X as Batch stores
    participant R as Rebind pass
    participant E as Extract
    participant G as Render world

    Note over B: cleared once<br/>start of append window
    P->>B: append_material_slot() rows (pre-Propagate)
    B-->>P: MaterialSlotId per row
    Note over P: TransformSystems::Propagate
    P->>P: world transforms, bounds, sort centers
    P->>X: BatchResourcesReady (create/grow/register/unregister)
    Note over B: MaterialTableUpdatedToCurrent<br/>table frozen, append-after-freeze panics
    X->>R: rebind_registered_material_table_buffers (last in PostUpdate)
    R-->>X: every batch material points at current table buffer
    Note over E: extract records + rows together (frame-atomic)
    E->>G: extracted MaterialSlotValues + records
    G->>G: prepare_material_table_buffer uploads before bind-group prep
```

Frame-atomic guarantee: records and table rows extract together each frame, so a
render-world record never indexes a different frame's table — no N-1-record /
N-row mix.

## 5. Shader read path (shared by all three families)

Both the SDF fill shader and the analytic path shader read the table through one
guarded helper. Direct `material_table[...]` reads outside the helper are
rejected by a shader source tripwire.

```mermaid
flowchart TD
    rec["record material id"] --> g1{"role-present bit set?"}
    g1 -->|"no"| collapse["`collapsed/transparent material
    (no table read)`"]
    g1 -->|"yes"| g2{"id != INVALID_GPU_MATERIAL_SLOT?"}
    g2 -->|"no"| collapse
    g2 -->|"yes"| g3{"id < arrayLength(material_table)?"}
    g3 -->|"no"| collapse
    g3 -->|"yes"| read["read material_table[id]"]
    read --> uv["`compute_material_sampled_uv(box_uv, uv_transform)
    final_uv = uv_transform * box_uv`"]
    uv --> sample["`sample texture channels
    multiply base_color before coverage/stroke`"]
    sample --> pbr["pbr_input_from_material_table to apply_pbr_lighting"]
```

## Type reference

| Concept | SDF fills/borders | Text runs | Panel shapes |
|---|---|---|---|
| Cascade source handle | `SdfMaterial` | `TextMaterial` | `ShapeMaterial` |
| Source identity key | `SdfMaterialSourceKey{panel, command_index, role}` | `TextRunMaterialSourceKey{run}` | `PanelShapeMaterialSourceKey{shape}` |
| Append-time input | `SdfMaterialSlotInput` | `TextRunMaterialSlotInput` | `PanelShapeMaterialSlotInput` |
| GPU record | `SdfRenderRecord` | `PathRenderRecord` | `PathRenderRecord` |
| Material slot field | `fill_material` / `border_material: GpuMaterialSlotId` | `material: MaterialSlotId` | `material: MaterialSlotId` |
| Batch key | `SdfBatchKey` | `PathBatchKey` | `PathBatchKey` |
| Batch resources | `SdfBatchResources` | `PathBatchResources` | `PathBatchResources` |
| Render material | `SdfExtendedMaterial` (`SdfExtension`) | `PathExtendedMaterial` (`PathExtension`) | `PathExtendedMaterial` (`PathExtension`) |

Shared by all three: `MaterialSlotCandidate`, `MaterialSlotValues`,
`PipelineCompatibility`, `ResourceCompatibility`, `FrameMaterialTable` /
`FrameMaterialTableBuilder`, `MaterialSlotId` / `GpuMaterialSlotId`, the
`MATERIAL_TABLE_BINDING = 106` GPU table, and the
`pbr_input_from_material_table` / `compute_material_sampled_uv` WGSL helpers.
