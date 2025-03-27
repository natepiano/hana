# Display Environment
{{#include ../misc/ai.md}}

## Purpose
The display environment system manages the spatial arrangement and
configuration of all display devices in the hana network,
enabling visualization across arbitrary 3D configurations of screens
and projectors. Or lasers. Or Lights. Or AR devices. Or VR headsets. Or whatever.

## Requirements
- Place virtualized screens and projectors in a 3D space mimicking the physical environment such as a stage or installation
- Display and manage the physical properties of each display including information about the machine to which they're attached in the mesh network.
- Provide virtualized preview of visualizations on available displays in the [application](application.md)
- Allow [state](state.md) to be saved and loaded
## Hierarchy
1. **Environment**: The complete 3D space where displays exist
    - Contains one or more Groups (fixtures? walls?) that define the physical space and on which Displays can be located
    - Contains one or more Displays
    - Defines the global coordinate system
    - Manages global properties (e.g., ambient lighting, scale)
    - Can be assigned a name - such as "Stage" or "Installation" or "Favorite Customer"
2. **Display Groups**: A logical collection of displays with spatial relationship
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
4. **Window**: A standard operating system window
   - Bounded area where plugins render
   - Managed by the operating system
   - Contains a visualizations
   - Handles visualization-specific parameters
   - It's important to know that a visualization can run on multiple windows on multiple Displays. It could be duplicated, it could be given information to coordinate running different parts of the visualization across different displays - e.g., the camera angle and viewport that this particular monitor is handling for a much larger visualization.

## Spatial Management
- Environment uses right-handed 3D coordinate system
- Display Groups define local coordinate spaces - probably via parent / child relationships in bevy
- Displays track physical dimensions and positions
- Windows handle viewport calculations and plugin rendering

## Example Configuration
think of this as pseudocode for example purposes only - names and fields will almost certainly change

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
