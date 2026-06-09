# GPU meter — Main / Render / GPU on one shared clock

Status: **partly built**. The recv-block instrumentation, shared-clock alignment,
per-lane segments, cleanup/return split, and segment labels exist in
`diegetic_text_stress`. The left stats table uses the same phase labels and is
positioned from the title bar's `PanelDimensionsChanged` screen-space bottom edge.
The GPU full-frame duration is still inferred. Supersedes the layout sketch in
`pipelined-render-timing.md` §4.

## Goal

The bottom overlay draws three bars — **Main world**, **Render world**, **GPU** —
for one frame on a single shared clock. Reading straight **down** at any x shows
what is waiting on what, with each bar placed at its true offset on one clock.

## Pipeline model

Three stages run concurrently, each one frame behind the one before it:

| lane          | frame | does                                            |
|---------------|-------|-------------------------------------------------|
| Main thread   | N+2   | sim + layout + shaping + mesh (the `Main` schedule) |
| Render thread | N+1   | `Render` schedule: main world → GPU commands    |
| GPU           | N     | executes those commands, presents at the display |

Main is two updates ahead of what is on screen. Read any vertical slice and three
different frames are in flight at once — that overlap is why the render thread's
time runs *alongside* the main frame instead of adding to it.

The handoff each period (from `bevy_render::pipelined_rendering`):

1. `app.update()` runs the **Main schedule** (First..Last) on the main thread.
2. `renderer_extract` (still the main thread) calls `recv().await` — **Main blocks
   here** until the render thread returns the previous frame's render app, which
   it cannot do until its whole `Render` schedule (render graph and cleanup
   included) finishes.
3. Main runs **extract** (copy main world → render world) and `send_blocking`s the
   app back; the render thread starts the next frame, applying the extracted data
   first in `ExtractCommands`.

## Measured vs inferred

Measured — `Instant` marks, all on the same process-monotonic clock:

- Main: frame start (`First`), schedule end (`Last`), extract begin (first system
  in `ExtractSchedule`, on the main thread).
- Render: schedule start/end, before/after `PrepareAssets`, before/after
  `PrepareViews` (the swapchain-acquire stall), before/after `Render` (render
  graph).
- `recv` block = `extract_begin − main_schedule_end` — the real Main wait.
- `return` gap = `extract_begin − render_schedule_end` — schedule closeout,
  app-return handoff, and main `recv` unblock overhead.
- Left stats table `render` total = `assets + prep + wait for present +
  render graph + cleanup + return`, matching the meter vocabulary. The first
  five are inside the `Render` schedule; `return` is the measured post-schedule
  handoff gap.

Inferred — not measured:

- GPU `current` **left edge** and the `next` tail. We measure *when* the GPU
  presents (the acquire releases) but not *how long* it ran. Only the present
  instant (the `current` right edge) is real.

The GPU `TIMESTAMP_QUERY` path measures only the `OrbitCam` opaque `MainPass`
(~0.3 ms sliver) — it excludes the prepass, the OIT resolve, and every overlay
camera — so it does not drive the GPU lane.

## Lanes and segments

### Main world

| pos    | label            | meaning                                                        |
|--------|------------------|----------------------------------------------------------------|
| left   | `work`           | the `Main` schedule: sim, layout, shaping, mesh                |
| center | `wait on render` | blocked in `recv` until the render thread returns last frame's app |
| right  | `extract`        | copy main world → render world (then send back)                |

### Render world

| pos    | label              | meaning                                                                 |
|--------|--------------------|-------------------------------------------------------------------------|
| left   | `prep`             | `ExtractCommands` + prepare assets/meshes/views/specialize/queue/bind   |
| center | `wait for present` | swapchain-acquire stall (`PrepareViews`): blocked until the GPU presents the *previous* frame and frees an image |
| center | `render graph`     | render-graph run: encode passes + submit/present                        |
| center | `cleanup`          | render cleanup and schedule closeout after the graph                    |
| center | `return`           | render app return + main `recv` unblock gap before extract begins       |
| right  | `wait for extract` | render thread done; parked on `recv`, waiting for Main to run `extract`. No render-thread work here — on the shared clock this overlaps Main's `extract` exactly |

### GPU

| pos    | label            | meaning                                                                  |
|--------|------------------|--------------------------------------------------------------------------|
| left   | `current`        | GPU finishing the frame now reaching the screen. Right edge (present) is measured — it releases Render's acquire. Left edge inferred |
| center | `idle`           | GPU has nothing to do while the render thread encodes + submits the next render graph. Not extract, not readback |
| right  | `next (inferred)`| GPU starting the next frame's commands. Inferred                         |

## Shared-clock layout

```
            0ms        2ms        4ms        6ms        8ms
            |----------|----------|----------|----------|----
Main world  [ work .. ][ wait on render ............ ][extract]
Render world[ prep ..][ wait for present ][ render graph ][cleanup][return][ wait for extract ]
GPU         [ current ............ ]  idle              [next.....]
```

- One shared **epoch** `Instant` cloned into both worlds; every mark stored as
  `(instant − epoch)` ms — one absolute timeline both threads write to.
- Anchor the displayed window at the render frame's `start` mark; place each
  lane's segments at their absolute offsets.
- Two real-time anchors hold by construction (vertical lines):
  - Main `wait on render` end == Render `return` end → Main starts extract.
  - Render `wait for extract` == Main `extract` → same span; render parked while
    Main copies and sends the render app back.
- **Do not draw vertical anchor lines into the GPU lane** for now — its left edge
  and tail are inferred, so a line claiming a GPU instant would overstate what we
  measured. Draw the two inferred GPU edges dim/dashed to mark them as inferred.

## Cadence

Sample once per second (mean each lane's segments over that second). On the new
sample, smoothstep-morph the bars old → new over ~200 ms, then hold ~800 ms.
Watch a smooth slide, study a still second, repeat.

## Older measured example (M2 Max, perf mode, this scene)

This sample predates the cleanup/return split; the current overlay reports those
spans separately.

| segment                 | ms   |
|-------------------------|------|
| frame                   | 9.1  |
| Main `work`             | 2.94 |
| Main `wait on render` (`recv`) | ~5.6 |
| Render total            | 8.5  |
| ├ `prep` (assets+prep)  | 1.73 |
| ├ `wait for present`    | 4.48 |
| └ `render graph`        | 2.33 |

Identity that confirms the recv measurement: `recv ≈ render_total − main_work`
(`8.5 − 2.94 = 5.56`). The render thread runs concurrently with Main's 2.94 ms of
work; rendering needs 8.5 ms; so Main blocks the remaining ~5.6 ms. The frame
length is set by the render thread (dominated by the 4.48 ms acquire stall), not
by when Main finishes — which is why the period outlasts Render's `render graph`.

## Open / next

- GPU full-frame duration unmeasured; the `current` left edge and `next` tail stay
  inferred until a full-frame GPU timer exists.
