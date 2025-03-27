# Modulation System
{{#include ../misc/ai.md}}

## Purpose
The modulation system provides the foundation for controlling visualization behaviors and enables dynamic connections between parameters through modulation. It offers a standardized interface for input/output integration and real-time control.

### Modulation Example
We want to build on the analogy of VCV Rack modules but instead of controlling audio signals, we will be controlling visualization parameters. Imagine a module that implements a low frequency oscillator emitting a triangle wave every 2 seconds. The output of this module can be connected to the input of a visualization parameter - for example the zoom level. As the triangle wave oscillates, the zoom level of the visualization steadily increase until the peak of the wave and then decrease back to the start as the wave completes its cycle - taking a full 2 seconds to complete.

If the module was synchronized to a clock, the zoom level could be made to oscillate in time with music.

### Experience TBD
We have not yet decided whether to create a modulation / module system exactly as in VCV Rack - with mono and polyphonic cables, a DSP, etc.  We are still exploring this area.

Other models could be the node based systems such as seen in Blender. These modules are uniform in their look and feel - while they all have different behaviors associated. This is probably more simple to implement than the VCV Rack approach.

## Modulation System
### Purpose
Enables dynamic connections between parameters for real-time control. For example, an audio input level could control the zoom level of a visualization.
### Components
- **Sources**: Parameters or inputs that provide control values
- **Routes**: Connections between sources and destinations with scaling
- **Destinations**: Parameters that receive modulated values
### Operation Flow
1. Sources emit value changes in -1.0 to 1.0 range - inputs from external devices are normalized to this range
2. Routes connect sources and destinations - potentially we could add scaling, offset, etc. but these can also be modules themselves so we can keep it simple
3. Destinations receive and apply updated values
## Management Interface
- Visual node-based routing interface (maybe like Blender geometry nodes, maybe like VCV Rack)
- Real-time connection state visualization
- Simple scaling controls per route
- Active connection highlighting

## [State Management](./state.md)
- Parameter values and modulation routes saved with system state
- Network synchronization via [State Management](./state.md) system
- Persistence of routing configurations
