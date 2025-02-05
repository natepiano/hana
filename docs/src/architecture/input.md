# Input and Control Integration
## Unified Input System
- Supports OSC, midi, and other external inputs for parameter control.
- Automatic address registration and mapping for parameters.
## OSC Integration
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
## Additional Input Support
- Keyboard, mouse, gamepad, and other sensors for parameter mapping.
## State management
- input configuration saved automatically
- parameters saved automatically
- a configuration can be explicitly saved
- user input mappings can be explicitly saved
