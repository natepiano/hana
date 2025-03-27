# State Management
There are a couple of areas to cover in state. One is saving the contents of the node editor/environment editor. Another is propagating state in the networking. I suppose there are other states - such as account, node library. Anyway - right now this is a page that is in serious need of disambiguation and splitting apart.

# save hana
first off - what is a save going to be called?
## research
- Bevy_serialization_extras - but more likely - bevy_save which seems to be pretty full-featured for game saves

# Below is from an AI dialogue and is just kept around for historical reasons
## Purpose
Manages the synchronization, versioning, and persistence of state across the hana system. Provides a robust foundation for maintaining consistency and handling state recovery across the distributed system.

This document is really just some scratch notes about some things we know we'll need (persisting the management app configuration) and the fact that if we have a mesh architecture we will need some way to synchronize system state across all nodes in the mesh.

Everything in this doc is subject to change - and speculative at this point.

## Core Responsibilities
- State versioning and change tracking
- State synchronization logic
- Conflict detection and resolution
- State persistence and recovery
- Update ordering and consistency
## Runtime State Components
### [Management App](application.md) State
- Global environment properties
- Display group configurations
- Individual display states
- Window assignments and properties
- Any value that the user sets
### Visualization State
- Modulation state
- Input mappings
- Modulation routing
- Input mappings (OSC, midi)
### System State
- Current controller assignment
- Connected peer information
- Global system settings
- Resource allocation state
## State Synchronization

### Version Management
- Each state change assigned unique version
- Version tracking per state component
- Change history maintenance
- Component-level version vectors
- Obsolete version cleanup
### Change Propagation
- State change detection
- Change record creation
- Version assignment
- Propagation via [Network Architecture](./network.md)
- Propagation status tracking
- Retry handling for failed propagation
### Conflict Resolution
- Single-controller model for writes
- Last-write-wins for simultaneous changes
- Component-level conflict detection
- User notification for critical conflicts
- Automatic resolution for non-critical conflicts
## Network Integration
### State Propagation (made up example)
```rust
struct StateChange {
    component: StateComponent,
    version: Version,
    data: Vec<u8>,
    timestamp: u64,
}

impl StateManager {
    fn propagate_state_change(&self, change: StateChange) {
        // Create versioned change record
        let versioned_change = self.create_versioned_change(change);

        // Request propagation via Network Architecture
        self.network.propagate_message(versioned_change);

        // Track propagation status
        self.track_change_propagation(versioned_change);
    }
}
```
### Network Event Handling
- Peer disconnect state preservation
- Reconnection state reconciliation
- Network partition recovery
- Partial update handling
- State request processing
## State Persistence
### Save Operations (made up example)
```rust
struct SavedState {
    environment: EnvironmentState,
    display_groups: Vec<DisplayGroupState>,
    displays: Vec<DisplayState>,
    windows: Vec<WindowState>,
    plugin_states: Vec<PluginState>,
    parameters: ParameterState,
    modulation: ModulationState,
    input_mappings: InputMappingState,
    version: Version,
    timestamp: u64,
}

impl StateManager {
    fn save_state(&self) -> Result<SavedState, StateError> {
        // Capture current state atomically
        // Return serializable state structure
    }
}
```
### Restore Operations
```rust
impl StateManager {
    fn restore_state(&mut self, state: SavedState) -> Result<(), StateError> {
        // Validate state version
        // Restore components in dependency order
        // Verify restoration success
        // Propagate new state if controller
    }
}
```
## Recovery Mechanisms
### State History
- Rolling change history buffer
- Component-level change tracking
- Recovery points for critical operations
- Change log persistence
### Fallback Options
- Revert to previous known good state
- Reset to default values
- Manual conflict resolution interface
- Partial state restoration
### Recovery Process
1. Detect inconsistency or failure
2. Identify affected state components
3. Select appropriate recovery strategy
4. Execute recovery operations
5. Verify state consistency
6. Resume normal operation
## [Error Handling](error_handling.md)
- Invalid state detection
- Version mismatch handling
- Corruption detection
- Recovery trigger conditions
- Error reporting interface

## Security
- Authorized state modification checks
- State update validation
- Secure state storage
- Access control integration
- Audit logging
## Example Workflows

### Runtime Synchronization
1. Component state change detected
2. Version number assigned
3. Change propagated via [Network](network.md)
4. Peers validate and apply update
5. Confirmation of synchronization
### State Save/Restore
1. User initiates save
2. Current state captured atomically
3. State serialized and stored
4. Optional network synchronization
5. Available for future restoration

## Integration Points
### [Network](network.md)
- Message transport for state updates
- Peer status notifications
- Network health monitoring
- Connection event handling
### [Visualization Manager](visualization_manager.md)
- Visualization state capture
- State restoration to plugins
- Version compatibility checks
- Resource state tracking
### [Management App](application.md)
- State visualization
- Manual intervention controls
- Recovery operation triggers
- Configuration interface
### [Undo](undo.md)
- what happens when undo passes a save or load operation?
