# Display Environment Architecture

## Purpose
The display environment system manages the spatial arrangement and 
control of all display devices in the hana network, 
enabling visualization across arbitrary 3D configurations of screens 
and projectors.

The Display Environment is managed within the [hana application](application.md).

## Requirements
- Support single-screen and multi-monitor configurations
- Expose standardized parameter ranges for visualizations
- Enable multiscale rendering and split-rendering capability
- Provide virtualized preview of all available displays in the [application](application.md)
## Hierarchy
1. **Environment**: The complete 3D space where displays exist
    - Contains one or more Display Groups
    - Defines the global coordinate system
    - Manages global properties (e.g., ambient lighting, scale)
    - Can be assigned a name

2. **Display Group**: A logical collection of displays with spatial relationship
    - Examples: "North Wall", "Stage Right", "Ceiling Array"
    - Defines relative positioning of member displays
    - Can be manipulated as a unit (move, rotate, etc.)
    - Can be assigned a name

3. **Display**: A physical output device (monitor, projector)
    - Physical properties (resolution, dimensions)
    - Position and orientation in 3D space
    - Network location and status
    - Contains one or more windows
    - Can be assigned a name

4. **Window**: A visualization container
    - Bounded area where plugins render
    - Can span multiple displays
    - Maintains plugin instance and state
    - Handles plugin-specific parameters

## Spatial Management
- Environment uses right-handed 3D coordinate system
- Display Groups define local coordinate spaces
- Displays track physical dimensions and positions
- Windows handle viewport calculations and plugin rendering

## Example Configuration
```rust
struct Environment {
    coordinate_system: CoordinateSystem,
    display_groups: Vec<DisplayGroup>,
    global_properties: GlobalProperties,
}

struct DisplayGroup {
    name: String,
    position: Vector3,
    rotation: Quaternion,
    displays: Vec<Display>,
}

struct Display {
    physical_dimensions: Dimensions,
    resolution: Resolution,
    position: Vector3,
    rotation: Quaternion,
    network_location: NetworkPeer,
    windows: Vec<Window>,
}

struct Window {
    bounds: Rect,
    plugin_instance: Option<Box<dyn Plugin>>,
    parameters: ParameterSet,
}
```
## Doc Links
- [Architecture](../architecture/README.md) - High level system design
- [Developer](../developer/README.md) - Coding guidelines for hana contributors
- [Overview](../../README.md) - Hana overview
- [Plugin Development](../plugins/README.md) - Guidelines for plugin development
- [User](../user/README.md) - Hana user documentation
