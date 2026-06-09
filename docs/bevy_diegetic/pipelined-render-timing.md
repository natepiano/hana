# Pipelined render timing — how the frames overlap

Three things process every frame, each on its own timeline:

- **Main thread** — game/sim logic (spawns, transforms, your text mutations).
- **Render thread** — turns the world into GPU commands (the `Render` schedule).
- **GPU / display** — executes those commands and shows the result at vsync.

They are **pipelined**: like a factory line, each stage is always one frame
behind the stage before it. The main thread never waits on the render thread for
the *same* frame, because by the time the main thread is on frame N+1, the render
thread is still finishing frame N. That offset is the overlap.

> These charts use **time-slot tables**, not mermaid gantt. A gantt would not
> place the bars and the vsync markers at the times I gave it, which is exactly
> what made the earlier version unreadable. In a table the columns line up by
> construction, so "this ends when that happens" is true on screen.

## 1. The pipeline at a glance

Each column is one **period** (one frame's worth of time). Read **down a
column** = everything happening at that instant. The starred cells are frame
**N** moving through the three stages.

| lane              | period 1     | period 2      | period 3     |
|-------------------|--------------|---------------|--------------|
| **Main thread**   | sim ✦N       | sim N+1       | sim N+2      |
| **Render thread** | render N-1   | render ✦N     | render N+1   |
| **GPU / display** | gpu N-2      | gpu N-1       | gpu ✦N       |

- Follow the **✦** cells: frame N is **sim**ulated in period 1, **render**ed in
  period 2, run by the **GPU** in period 3. It steps diagonally down-right, one
  period per stage.
- Read any single **column** and three *different* frames are in flight at once.
  In period 2 the main thread is already on N+1 while the render thread is still
  on N and the GPU is still showing N-1. Nobody is blocked on the same frame —
  that is the overlap, and why the render thread's time runs *alongside* the
  main-thread frame instead of adding to it.
- The handoff from main → render each period is **Extract**: a brief copy of the
  main world into the render world, after which the render thread takes over.

## 2. Inside one render frame — where the GPU stall is

Now zoom into a single `render N` cell from the table above and put the GPU
underneath it, sharing the **same time columns** (t0…t7, each a slice of one
render frame):

| lane                 | t0      | t1      | t2      | t3      | t4      | t5             | t6     | t7    |
|----------------------|---------|---------|---------|---------|---------|----------------|--------|-------|
| **Render — frame N** | prep    | prep    | WAIT    | WAIT    | WAIT    | queue / bind   | submit | —     |
| **GPU / display**    | run N-1 | run N-1 | run N-1 | run N-1 | run N-1 | ◆ present N-1  | —      | run N |

Read it across:

1. **prep** (t0–t1) — extract-apply, prepare meshes/views, specialize. CPU work.
2. **WAIT** (t2–t4) — the render thread asks for a swapchain image to draw into
   and **blocks**. Look straight **down** from where WAIT ends (the t4 → t5
   boundary): that is column **t5**, where the GPU finishes frame N-1 and
   **◆ presents it at vsync**. Presenting frees an image. *That* is what the wait
   was waiting on — the **previous** frame reaching the display, not frame N's
   own GPU work. The longer the GPU takes on N-1, the more WAIT columns appear.
3. **queue / bind** (t5) — the instant an image is free, the render thread
   unblocks and resumes CPU work.
4. **submit** (t6) — record the draws into that image and hand them to the GPU.
   The GPU then **runs frame N** (t7), and will present *it* at the next vsync.

The single idea: the stall sits **between** two chunks of CPU work (after prep,
before submit), and it is gated by an **earlier** frame hitting vsync.

## 3. Why your tail-wait model was off

You had: *finish all CPU work → wait → submit.* Actual order:

```
prep → [WAIT for an image] → queue/bind → submit → (GPU runs async) → present @ vsync
```

The wait is in the **middle**. Under vsync the CPU can race ahead of the display
by at most the swapchain depth (a couple of frames); once every image is in
flight, the next image-acquire stalls until the display releases one at the next
refresh.

That stall is exactly the `gpu wait` row in the overlay (the WAIT columns above):
small when the GPU keeps up, large when a fragment-heavy frame — the transparent
text pass — can't finish inside one refresh interval, so N-1 presents late and
frame N's acquire waits longer.

## 4. The live overlay — the timeline we want

The bottom panel draws three bars, one per lane: **Main world**, **Render
world**, **GPU**. Today all three start at the left edge, so you read *how long*
each took but never *how they line up*. The goal: slide each bar to its real
place on one shared clock, so reading **straight down** at any x shows what is
waiting for what.

```
            0ms          4ms          8ms          12ms        16ms
            |------------|------------|------------|------------|
Main world  [ work ........................ ][ wait ........... ]
Render world             [ prep ...][ WAIT .......][ graph ]
GPU                  [ draw N-1 ......... ]◆ present
                                          └── same x: GPU present ─┐
                                              releases Render WAIT ┘
```

The single line that makes the whole thing legible: the **GPU's present** and
the end of the **Render WAIT** sit at the **same x**. That is the wait paying
off — the previous frame reaching the display is what frees the image the render
thread was blocked on.

### Three changes from what's there now

1. **Draw the real GPU bar.** The GPU lane currently shows a full-width
   placeholder equal to frame time. Replace it with the true GPU draw time the
   timer already measures (`GpuFrameMs`) — the time the GPU spent on the world's
   3D pass.

2. **Line the bars up.** Give each lane a left offset so vertical = same instant:
   - **Main** starts at x = 0.
   - **Render** starts at the offset where main handed off to it. We already have
     the marks to measure this: main-world frame start and the render thread's
     start are both plain `Instant`s on the same process clock, so the offset is
     a subtraction, not an assumption.
   - **GPU** is placed so its **present marker ends at the Render WAIT's right
     edge** (the vsync instant), with the bar's width = the measured GPU draw
     time, extending left from there. No GPU-clock-to-CPU-clock conversion needed
     — we borrow the present instant from the CPU side (where WAIT ends) and take
     only the *duration* from the GPU timer.

3. **Slow, readable updates.** Stop the continuous 5Hz slide. Instead:
   - **Sample once per second** — average each lane's segments over that second.
   - On the new sample, **lerp the bars from the old picture to the new over
     ~200ms**, then **hold for ~800ms**. You watch a smooth morph, then a frozen
     second you can actually study, then the next morph.

### What each bar's segments are

| lane         | segments (left → right)                         | source                          |
|--------------|-------------------------------------------------|---------------------------------|
| Main world   | `work` · `wait`                                 | main-thread span, frame time    |
| Render world | `prep` (assets+prep) · `WAIT` · `graph`         | render-thread `Instant` marks   |
| GPU          | `draw` ending at the present marker              | `GpuFrameMs` (real GPU timer)   |

`work` = the main schedules (layout, reconcile, shaping, mesh, the rest).
`wait` = frame time past `work` — the main thread blocked handing off + extract.
`WAIT` = the swapchain acquire stall (`PrepareViews`), the one gated by vsync.
