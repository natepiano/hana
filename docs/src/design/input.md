# Input and Control Integration

{{#include ../ai.md}}

## Purpose
The input and control system provides a standardized interface for integrating various input sources into the hana system. This allows for real-time control of visualizations and other parameters through external devices and sensors.

## Requirements
- Supports OSC, midi, and other external inputs for parameter control.
- Visualize input sources and their mappings to [modulation](modulation.md) parameters.
- Map visualization's modulation parameters to input sources such as keys on a keyboard, midi notes, midi cc, OSC addresses,computer keys, mouse, gamepad, etc.
- It should be easy to hide/show input sources when necessary so as not to clutter the UI

## Thoughts about integration with external sources
Another form of control could be to internally route output audio channels from VCV Rack to be parsed as input to hana. Audio signals from VCV Rack can still contain low frequency control voltages which can be used for modulation (audio rate signals can also be used). This would allow for the creation of complex modulation chains in VCV Rack that can be used to control hana visualizations.

There's a lot more on this topic as to the choices to integrate VCV Rack but it could provide a very powerful input source for hana.

## OSC Integration (example)
- **Address Structure**:
  ```
  /hana                   # Root namespace
    /{viz_id}             # Visualization instance
      /{group}            # Parameter group
        /{param}          # Parameter name
  ```
  Example addresses:
  ```
  /hana/viz1/camera/zoom   # Specific visualization parameter
  /hana/global/tempo      # Global control parameter
  ```
- **Features**:
    - Real-time parameter control from OSC sources.
    - Bidirectional communication for feedback.
    - OSC value mapping and bundle support.
## Midi Integration
- **Features**:
    - Control mapping with velocity sensitivity.
    - Support for multiple midi devices.
    - MPE (Midi Polyphonic Expression) support.
## Additional Input Support
- Keyboard, mouse, gamepad, and other sensors for parameter mapping.
## State management
- input configuration saved automatically
- parameters mappings saved automatically
- a configuration can be explicitly saved
- user input mappings can be explicitly saved
