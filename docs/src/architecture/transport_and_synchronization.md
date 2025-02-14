# Transport and Synchronization
## Purpose
Outline the transport control mechanism and timing synchronization for the hana system.

By transport we mean play/pause/stop/seek/loop/rewind/etc. By synchronization we mean keeping the timing of the visualizations in sync across all displays.

## Transport Requirements (options)
- Transport controls for play/pause/stop/restart
  - Looping
  - Seek?
  - Scrubbing?
- External Transport control
  - MIDI Machine Control (MMC)
  - OSC
  - VCV Rack - audio outputs containing transport signals

## Timing Requirements
- Timing synchronization
  - Sync to external clock
  - Sync to internal clock (shared in the mesh)
  - Sync to other software?
  - Timecode (SMPTE, MIDI, etc.)?

## Visualization Timing
We'll need some mechanism to ensure that if the same visualization is running on multiple screens - or one visualization is running across multiple screens - that they are all in sync - at the frame level. There may be a piece of this that relies on a common timing sync mechanism especially if we're trying to sync to music.

If we're not syncing to the clock then we'll still want to sync to the same frame - not sure how we'll do this in bevy yet but probably at least we'll need to use the fixed update system.

### Ideas
1. Use Ableton Link for timing synchronization
