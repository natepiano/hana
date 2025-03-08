# Hana Protocol Architecture Plan

## Motivation

The current Hana architecture duplicates command concepts across multiple layers (user input, network messages, events, async runtime), making it difficult to add new commands consistently. A protocol-driven architecture would:

1. Create a single source of truth for all commands
2. Ensure consistent handling across all layers
3. Use the type system to enforce completeness
4. Reduce the chance of inconsistencies when adding new commands
5. Provide clear documentation of which commands apply to which layers

## Layers to Handle

1. **User Input Layer**: Handling keyboard/input device actions (`Action` enum)
2. **Event Layer**: Managing Bevy events for visualization control (`StartVisualization`, etc.)
3. **Network Protocol Layer**: Defining network messages between components (`Instruction` enum)
4. **Async Runtime Layer**: Assigning tasks to the async runtime (`RuntimeTask` enum)
5. **Runtime Outcome Layer**: Processing results from async operations (`RuntimeOutcomeMessage`)

## Recommended Approach: Protocol Trait Architecture

We recommend a trait-based approach where:

1. A central `Protocol` enum defines all available commands
2. Layer-specific traits define how a protocol is handled at each layer
3. Individual implementations map protocols to layer-specific actions
4. Registration helpers ensure all applicable protocols are properly registered
5. Verification functions ensure complete coverage

## Crate Structure

```
hana_protocol/
├── src/
│   ├── lib.rs            # Protocol enum and core traits
│   ├── layers/
│   │   ├── mod.rs        # Layer trait exports
│   │   ├── user_input.rs # User input layer trait
│   │   ├── event.rs      # Event layer trait
│   │   ├── network.rs    # Network layer trait
│   │   ├── runtime.rs    # Runtime layer trait
│   ├── mapping/
│   │   ├── mod.rs        # Mapping function exports
│   │   ├── action.rs     # Protocol to Action mappings
│   │   ├── instruction.rs # Protocol to Instruction mappings
│   │   ├── runtime_task.rs # Protocol to RuntimeTask mappings
│   ├── verification.rs   # Verification functions
```

## Implementation Details

### 1. Core Protocol Definition

```rust
// In hana_protocol/src/lib.rs
use strum::{EnumIter, IntoEnumIterator};

#[derive(EnumIter, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Ping,
    Start,
    Shutdown,
    // Future commands here
}

// Define layer applicability
impl Protocol {
    pub fn applies_to_user_input(&self) -> bool {
        match self {
            Protocol::Ping => true,
            Protocol::Start => true,
            Protocol::Shutdown => true,
        }
    }
    
    pub fn applies_to_network(&self) -> bool {
        match self {
            Protocol::Ping => true,
            Protocol::Shutdown => true,
            Protocol::Start => false, // Start is not sent over network
        }
    }
    
    // Similar methods for other layers
}
```

### 2. Layer Traits

```rust
// In hana_protocol/src/layers/user_input.rs
use bevy::prelude::*;
use crate::Protocol;

pub trait UserInputHandler {
    // What Action (if any) this protocol maps to
    fn to_action(&self) -> Option<crate::mapping::Action>;
    
    // System to handle this protocol input (if applicable)
    fn get_input_system(&self) -> Option<SystemConfigs>;
    
    // Key mapping for this protocol (if applicable)
    fn get_key_mapping(&self) -> Option<(crate::mapping::Action, KeyCode)>;
}

// Similar traits for other layers
```

### 3. Implementation Example - User Input Layer

```rust
// In hana_protocol/src/layers/user_input.rs
impl UserInputHandler for Protocol {
    fn to_action(&self) -> Option<crate::mapping::Action> {
        if !self.applies_to_user_input() {
            return None;
        }
        
        match self {
            Protocol::Ping => Some(crate::mapping::Action::Ping),
            Protocol::Start => Some(crate::mapping::Action::Start),
            Protocol::Shutdown => Some(crate::mapping::Action::Shutdown),
            // Compiler error if we miss a variant!
        }
    }
    
    fn get_input_system(&self) -> Option<SystemConfigs> {
        if !self.applies_to_user_input() {
            return None;
        }
        
        match self {
            Protocol::Ping => Some(ping_system.run_if(
                just_pressed(self.to_action().unwrap())
            )),
            Protocol::Start => Some(start_system.run_if(
                just_pressed(self.to_action().unwrap())
            )),
            Protocol::Shutdown => Some(shutdown_system.run_if(
                just_pressed(self.to_action().unwrap())
            )),
            // Compiler error if we miss a variant!
        }
    }
    
    fn get_key_mapping(&self) -> Option<(crate::mapping::Action, KeyCode)> {
        if !self.applies_to_user_input() {
            return None;
        }
        
        match self {
            Protocol::Ping => Some((crate::mapping::Action::Ping, KeyCode::KeyP)),
            Protocol::Start => Some((crate::mapping::Action::Start, KeyCode::F1)),
            Protocol::Shutdown => Some((crate::mapping::Action::Shutdown, KeyCode::F2)),
            // Compiler error if we miss a variant!
        }
    }
}
```

### 4. Implementation Example - Network Layer

```rust
// In hana_protocol/src/layers/network.rs
use crate::Protocol;
use hana_network::Instruction;

pub trait NetworkInstructionHandler {
    fn to_instruction(&self) -> Option<Instruction>;
}

impl NetworkInstructionHandler for Protocol {
    fn to_instruction(&self) -> Option<Instruction> {
        if !self.applies_to_network() {
            return None;
        }
        
        match self {
            Protocol::Ping => Some(Instruction::Ping),
            Protocol::Shutdown => Some(Instruction::Shutdown),
            Protocol::Start => None, // Explicitly not a network instruction
            // Compiler error if we miss a variant!
        }
    }
}
```

### 5. Registration Helpers

```rust
// In hana_protocol/src/lib.rs
use bevy::prelude::*;

pub trait ProtocolPluginExt {
    fn register_protocol_handlers(&mut self) -> &mut Self;
}

impl ProtocolPluginExt for App {
    fn register_protocol_handlers(&mut self) -> &mut Self {
        // Register all user input systems
        for protocol in Protocol::iter() {
            if let Some(system) = protocol.get_input_system() {
                self.add_systems(Update, system);
            }
        }
        
        // Create input map from all protocols
        let input_map = Protocol::create_input_map();
        self.insert_resource(input_map);
        
        // Register event handlers for all protocols
        // ... similar pattern for other layers
        
        self
    }
}

impl Protocol {
    pub fn create_input_map() -> InputMap<crate::mapping::Action> {
        Protocol::iter()
            .filter_map(|p| p.get_key_mapping())
            .fold(InputMap::default(), |map, (action, key)| {
                map.with(action, key)
            })
    }
}
```

## Verification Approaches

### 1. Compile-Time Verification (Preferred)

```rust
// Will cause compiler errors if a protocol variant isn't handled
impl UserInputHandler for Protocol {
    fn to_action(&self) -> Option<crate::mapping::Action> {
        // Must handle all variants or compiler will complain
        match self {
            Protocol::Ping => Some(crate::mapping::Action::Ping),
            Protocol::Start => Some(crate::mapping::Action::Start),
            Protocol::Shutdown => Some(crate::mapping::Action::Shutdown),
            // Compiler error if we miss a variant!
        }
    }
}
```

### 2. Runtime Verification (For Layer Applicability)

```rust
// In hana_protocol/src/verification.rs
pub fn verify_protocol_coverage() {
    for protocol in Protocol::iter() {
        // If a protocol applies to a layer, it must have a handler
        if protocol.applies_to_user_input() {
            assert!(protocol.to_action().is_some(),
                "Protocol {:?} applies to user input but has no Action mapping", 
                protocol);
            assert!(protocol.get_input_system().is_some(), 
                "Protocol {:?} applies to user input but has no input system", 
                protocol);
            assert!(protocol.get_key_mapping().is_some(),
                "Protocol {:?} applies to user input but has no key mapping",
                protocol);
        }
        
        // Similar checks for other layers
        
        // Ensure every protocol has at least one layer
        assert!(
            protocol.applies_to_user_input() || 
            protocol.applies_to_network() || 
            protocol.applies_to_runtime(),
            "Protocol {:?} doesn't apply to any layer", protocol
        );
    }
}

// Run this in tests and/or app startup
#[cfg(test)]
mod tests {
    #[test]
    fn test_protocol_coverage() {
        super::verify_protocol_coverage();
    }
}
```

### 3. Unit Tests for Specific Behaviors

```rust
#[test]
fn test_network_protocol_consistency() {
    // Ensure all network protocols map to valid instructions
    for protocol in Protocol::iter() {
        if protocol.applies_to_network() {
            assert!(protocol.to_instruction().is_some(),
                "Protocol {:?} applies to network but has no instruction mapping",
                protocol);
        } else {
            assert!(protocol.to_instruction().is_none(),
                "Protocol {:?} doesn't apply to network but has an instruction mapping",
                protocol);
        }
    }
}
```

## Implementation Strategy

1. **Start Small**: Begin with the Protocol enum and one layer (e.g., user input)
2. **Add Layer by Layer**: Implement each handler trait one at a time
3. **Add Verification**: Add compile-time verification as you go
4. **Refactor Existing Code**: Gradually migrate existing code to use the protocol architecture
5. **Documentation**: Document the protocol approach thoroughly for future developers

This architecture creates a strong, type-safe foundation that scales well as Hana grows, while ensuring consistency across all layers of the application.

