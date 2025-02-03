# Configuration Management

## Overview
The configuration management system handles core system settings that determine how hana operates. It focuses on application configuration rather than user-created content or runtime state, which are managed by the [State Management](./state.md) system.
## Core Principles
- **Hierarchical Configuration**: System-wide defaults with local overrides
- **Simplicity**: Plain TOML files for easy editing and reading
- **Minimal Scope**: Only system configuration, not user content or state
- **Idiomatic**: Following rust ecosystem conventions
## Configuration Categories
### System Configuration
- Global logging settings
- Default resource allocation limits
- Temporary storage paths
### Local Instance Configuration
- Instance-specific network settings
- Local filesystem paths
### Network Configuration
- Network interface settings
- Default ports
- Local network discovery settings
- Connection timeouts
### [Visualization System](./visualization.md) Configuration
- Visualization load paths
- Default resource limits for visualizations
## Storage Format
- TOML format for consistency with rust ecosystem
- Simple hierarchy matching system components
- Comments explaining each setting
- Example configurations provided
## Configuration Operations
### Loading Sequence
1. Load system defaults
2. Apply local instance overrides
3. Initialize system with combined settings
### Validation
- Basic schema validation for configuration files
- Type checking and bounds validation
### Updates
- Direct file edits for changes
- Application restart to apply changes
- Simple network propagation where needed
## Implementation Guidelines
### File Structure
```
config/
  ├── defaults.toml    # System-wide defaults
  ├── local.toml      # Local instance overrides
  └── plugins.toml    # Plugin system settings
```
### Example Configuration
```toml
# System-wide defaults
[logging]
level = "info"
file = "logs/hana.log"

[resources]
max_memory = 2048
max_cpu_percent = 80

[network]
discovery_port = 45678
connection_timeout = 5000  # ms

[plugins]
load_path = "plugins/"
max_instances = 50
```
