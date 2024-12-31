# Hana Application
## Purpose
The hana application serves as the central hub for configuring 
and controlling all aspects of the hana system. It provides 
a 3D interface for managing the [Display Environment](./display.md) and integrates with all core systems:
- [Display Environment](./display.md) configuration
- Network configuration and monitoring
- Input controller setup (OSC, midi, keyboards, gamepads, sensors)
- Parameter and control system management
- Modulation interface management
- Plugin library management and updates
- State saving and restoration
- Performance monitoring
- Diagnostic tools
## 3D Interface
- Built as a 3D bevy application with orbit and zoom camera controls
- Visualizes the complete [display environment](display.md) hierarchy:
    - Global 3D environment space
    - Display groups with relative positioning
    - Individual displays with physical properties
    - Windows containing visualizations
- Features:
    - Live miniature previews of running visualizations
    - Status indicators for network connectivity
    - Visual feedback for spatial relationships
    - Grid and measurement tools for positioning
## [Display Environment](display.md)
### Environment Control
- Configure global 3D space properties
- Set measurement units and scale
- Define coordinate system orientation
### Display Group Operations
- Create and modify display groups
- Position groups in 3D space
- Apply transformations to entire groups
### Display Management
- Add and configure physical displays
- Set resolution and dimensions
- Position displays within groups
- Monitor network status and connectivity
### Window Management
- Create and manage visualization windows
- Assign plugins to windows
- Configure window bounds and properties
- Support spanning across multiple displays
- Allow for the same plugin on different windows
- Allow for setting whether it is full screen or not
### Interaction Features
- Drag-and-drop placement of windows
- Visual feedback for alignment
- Free-form and snap-to-grid movement
- Multi-select for group operations
- Click-to-configure individual elements
## Integration with Other Systems
### [Input and Control](./input)
- configuration of OSC, midi, keyboards, gamepads, other devices
- visualization of inputs
### [Modulation System](./modulation.md)
- Configure parameters globally or per visualization
- Assign inputs (OSC, midi, etc.) to specific parameters
- Configure and route modulation signals
- Monitor real-time modulation flows
### [Network Architecture](./network.md)
- Discover and connect to remote machines
- Monitor display network status
- Manage synchronization
### [Plugin System](./plugins.md)
- Manage visualization plugins
- Handle version compatibility
- Update management
## [State Management](./state.md)
- Save and restore complete environment state:
    - [Display Environment](./display.md) configuration
    - Parameter values
    - Modulation routing
    - Input mappings
    - Network settings
- Synchronize saved states across network
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

## Doc Links
- [Architecture](README.md) - High level system design
- [Developer](../developer/README.md) - Coding guidelines for hana contributors
- [Overview](../../README.md) - Hana overview
- [Plugin Development](../plugins/README.md) - Guidelines for plugin development
- [User](../developer/README.md) - Hana user documentation
