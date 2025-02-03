# Modulation System

## Overview
The modulation system provides the foundation for controlling visualization behaviors and enables dynamic connections between parameters through modulation. It offers a standardized interface for input/output integration and real-time control.
## Parameter Types
### Core Types
- **Param**: Standard adjustable parameter with configurable range
- **Input**: External control source (OSC, midi, etc.)
- **Output**: Value output for feedback or chaining
- **Light**: Visual indicator for status/activity
### Parameter Attributes
- `name`: Display name
- `group`: Organizational category
- `default`: Initial value
- `smooth`: Enable value smoothing
- `easing`: Default easing function
- `osc`: OSC address binding
## Modulation System
### Purpose
Enables dynamic connections between parameters for real-time control. For example, an audio input level could control the zoom level of a visualization.
### Components
- **Sources**: Parameters or inputs that provide control values
- **Routes**: Connections between sources and destinations with scaling
- **Destinations**: Parameters that receive modulated values
### Operation Flow
1. Sources emit value changes in -1.0 to 1.0 range
2. Routes apply scaling transformations
3. Destinations receive and apply updated values
## Management Interface
- Visual node-based routing interface
- Real-time connection state visualization
- Simple scaling controls per route
- Active connection highlighting
##  Example Parameter Declaration
```rust
#[derive(Visualization)]
struct MyVisualization {
    #[param(
        name = "Zoom",
        group = "Camera",
        default = 0.0,
        smooth = true,
        easing = "exponential",
        osc = "/camera/zoom"
    )]
    zoom: Param,

    #[input(
        name = "Rotation CV",
        group = "Camera",
        osc = "/camera/rotation"
    )]
    rotation_cv: Input,

    #[output(
        name = "Activity",
        group = "Feedback",
        osc = "/feedback/activity"
    )]
    activity_out: Output,

    #[light(
        name = "Active",
        color = "blue",
        group = "Status"
    )]
    active_light: Light,
}
```
## [State Management](./state.md)
- Parameter values and modulation routes saved with system state
- Network synchronization via [State Management](./state.md) system
- Persistence of routing configurations
