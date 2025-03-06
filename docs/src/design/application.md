# Hana Application
{{#include ../ai.md}}

## Purpose
The hana application serves as the central hub for configuring
and controlling all aspects of the hana system. It provides
a 3D game-like interface for managing the [Display Environment](display.md) and integrates with all core systems:
- Management app [Configuration](configuration.md)
- [Display Environment](display.md) management
- [Network](network.md) configuration and monitoring
- [Input](input.md) controller setup (OSC, midi, keyboards, gamepads, sensors)
- [Modulation](modulation.md) interface management
- Integrating with the [Visualization Library](visualization_library.md), the [SDK](visualization_sdk.md) and [managing visualizations](visualization_manager.md) from the application.
- [State](state.md) saving and restoration
- [Deployment](deployment.md) and update management
- Performance & Diagnostic tools
## 3D Interface
- Built as a 3D bevy application
- Visualizes the complete [display environment](display.md) - a representation of the actual "stage" or "space" that the displays are arranged in.:
    - Global 3D environment space
    - Display or projected screen with relative positioning
    - Individual displays with physical properties
- Features:
  - Integration with Visualization Library
  - Live miniature previews of running visualizations
  - Status indicators for network connectivity
  - Visual feedback for spatial relationships
  - Grid and measurement tools for positioning
  - Drag-and-drop interface for moving displays
  - Multi-select for group operations
  - Contextual menus for configuration
  - Real-time updates for changes in the environment
  - Undo/redo functionality
  - Save and restore complete environment state
  - Synchronize saved states across network
  - Monitor system metrics and performance
  - Modulation system interface
  - Input controller setup
## Display Environment
### Environment Control
- Configure global 3D space properties
- Set measurement units and scale
- Define coordinate system orientation
### Display Group Operations
- Select groups of displays
- Position groups in 3D space
- Apply transformations to entire groups
### Display Management
- Add and configure physical displays
- Set resolution and dimensions
- Monitor network status and connectivity
### Window Management
- Create and manage OS windows for visualizations
- Assign visualizations to windows
- Configure window bounds and properties
- Provide window and camera context to visualizations
- Support spanning across multiple displays
- Allow for the same visualization on different windows
- Allow for setting whether it is full screen or not
### Interaction Features
- Drag-and-drop placement of windows
- Visual feedback for alignment
- Free-form and snap-to-grid movement
- Multi-select for group operations
- Click-to-configure individual elements
## Integration with Other Systems
### [Input and Control](input.md)
- configuration of OSC, midi, keyboards, gamepads, other devices
- visualization of inputs
### [Modulation System](modulation.md)
- Configure parameters globally or per visualization
- Assign inputs (OSC, midi, etc.) to specific parameters
- Configure and route modulation signals
- Monitor real-time modulation flows
### [Network](network.md)
- Discover and connect to remote machines
- Monitor display network status
- Manage synchronization
### [Visualization System](visualization.md)
- Integrate with [Visualization Library](visualization_library.md)
- Communicate with visualizations via the SDK
## [State Management](state.md)
- Save and restore complete user state:
    - Display Environment
    - Parameter values
    - Modulation routing
    - Input mappings
    - Network settings
- Synchronize saved states across network so any controller would be able to load the same state
## Performance Monitoring
- Monitor system metrics:
    - Network latency
    - Resource usage
    - Visualization performance
    - Display sync status
## Diagnostic Tools
- Network diagnostics
- Display synchronization monitoring
- Error logging and recovery options
## [Deployment System](deployment.md)
- Update local application version
- Manage updates across network
