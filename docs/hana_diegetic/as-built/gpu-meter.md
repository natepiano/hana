# GPU meter - Main / Render / GPU on one shared clock

Status: **implemented and archived**. This note records the timing model now
implemented in `crates/hana_diegetic/examples/diegetic_text_stress.rs`.

The original goal was to draw the Main world, Render world, and GPU lanes on one
shared clock so a vertical slice shows what is waiting on what. The current
example does that for the CPU-observable timeline:

- Main and Render timestamps use one shared `Instant` epoch.
- The waterfall panel draws lane segments at render-relative offsets.
- The perf table uses the same labels as the waterfall.
- The title-bar dimensions event positions the stats panel below the title bar.
- GPU full-frame duration remains inferred because the existing GPU timestamp
  query measures only the `OrbitCam` opaque `MainPass`.

## Pipeline Model

Three stages run concurrently, each one frame behind the one before it:

| lane          | frame | does                                             |
|---------------|-------|--------------------------------------------------|
| Main thread   | N+2   | sim, layout, shaping, mesh, and extract handoff  |
| Render thread | N+1   | render-world prep and GPU command submission     |
| GPU           | N     | executes commands and presents to the display    |

The handoff each period is:

1. `app.update()` runs the Main schedule on the main thread.
2. `renderer_extract` blocks on `recv` until the render thread returns the
   previous frame's render app.
3. Main extracts into the render world and sends the render app back.
4. The render thread starts the next `Render` schedule.

## Measured Values

Measured on the shared process-monotonic clock:

- Main start and end, from `First` and `Last`.
- Main extract begin, from the first system in `ExtractSchedule`.
- Render start and end.
- Render `PrepareAssets`.
- Render `PrepareViews`, where swapchain acquire waits for the GPU.
- Render graph execution.
- Render cleanup after graph execution.
- Render app return and main `recv` unblock gap.
- Render parked time while Main extracts and sends the render app back.

Inferred values:

- GPU `current` left edge.
- GPU `next` right edge.

The measured GPU timestamp query is not used to size the GPU lane because it
only covers the `OrbitCam` opaque `MainPass`, excluding prepass, OIT resolve,
and overlay cameras.

## Perf Table Rows

The left stats panel reports the same vocabulary as the meter.

### Main World

| row               | meaning                                      |
|-------------------|----------------------------------------------|
| `ms/frame`        | full app frame delta                         |
| `layout`          | diegetic layout compute                      |
| `reconcile`       | panel text reconcile                         |
| `shaping`         | text shaping                                 |
| `mesh`            | panel text mesh build                        |
| `other`           | remaining measured Main schedule work        |
| `wait for render` | main blocked in render-app `recv`            |
| `extract`         | measured main-to-render handoff              |
| `frame slack`     | residual app-frame time not covered above    |

### Render World

| row               | meaning                                      |
|-------------------|----------------------------------------------|
| `render cycle`    | one render-start to next-render-start period |
| `assets`          | `PrepareAssets`                              |
| `prep`            | render prep outside named rows               |
| `wait for GPU`    | `PrepareViews` swapchain acquire stall       |
| `render graph`    | render graph execution                       |
| `cleanup`         | post-graph render cleanup                    |
| `return`          | render app return and main unblock gap       |
| `extract handoff` | render thread parked while Main extracts     |

## Waterfall Lanes

### Main World

| segment           | meaning                                      |
|-------------------|----------------------------------------------|
| `work N+1`        | measured Main schedule span                  |
| `wait for render` | measured main `recv` block                   |
| `extract`         | measured main-to-render handoff              |

### Render World

| segment        | meaning                                           |
|----------------|---------------------------------------------------|
| `work N`       | assets plus render prep before GPU wait           |
| `wait for GPU` | swapchain acquire waiting for a presentable image |
| `render graph` | render graph execution                            |
| unlabeled tail | cleanup, return, and extract-handoff blocks       |

The short tail blocks are colored to match `cleanup`, `return`, and
`extract handoff` in the perf table.

### GPU

| segment   | meaning                                      |
|-----------|----------------------------------------------|
| `current` | inferred GPU-busy block ending at present    |
| `idle`    | measured CPU-clock gap while render graph runs |
| `next`    | inferred next GPU-busy block                 |

The `current` right edge aligns with the render lane's `wait for GPU` ->
`render graph` boundary. That is the point where the GPU has presented enough
for swapchain acquire to release.

## Cadence

The waterfall samples one-second means into `WaterfallBars`, then smoothstep
morphs the displayed bars toward the new sample over `WATERFALL_MORPH_DURATION`.
During the hold period, the tree is left unchanged.

## Final State

The active work from this plan is complete:

- Shared-clock main/render alignment is implemented.
- Render graph, cleanup, return, and extract handoff are split.
- Waterfall labels and perf-table labels match the current code.
- The stats panel is positioned from `PanelDimensionsChanged`.
- The remaining GPU limitation is documented as inference, not active work.
