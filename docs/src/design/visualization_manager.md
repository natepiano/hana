# Visualization Manager
{{#include ../ai.md}}
## Purpose
The Visualization Manager allows for instantiation and runtime management of visualizations. It handles loading, initialization, and lifecycle management of visualizations, including resource allocation and cleanup. Visualizations are rendered within windows in the [Display Environment](./environment_editor).
## Architecture
### Core Components
1. **Visualization Runtime**
    - Loads and initializes visualizations
    - Manages visualization lifecycle and state
    - Handles [resource](resource.md) allocation and cleanup
    - Implements security sandboxing (needs investigation)
    - Monitors performance and [resource](resource.md) usage

2. Integration with **Visualization SDK**
    - the [SDK](visualization_sdk.md) provides the development kit for creating visualizations
    - Provides the ability to communicate via SDK API for:
        - Visualization creation
        - Modulation parameter management
        - Streaming of input and modulation data
        - State persistence
        - Error handling

3. Integration with **Visualization Library**
    - Communicate via Library API for:
        - Visualization discovery
        - Installation and updates
    - Assign visualizations to windows
    - Version management and compatibility tracking
