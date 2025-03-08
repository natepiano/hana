# Command Palette

We need a command system  for Hana that supports both internal key bindings and external control, while keeping it type-safe and extensible.

With this, we need a Command Palette with fuzzy searching as is found in many apps (obsidian, raycast, zed, jetbrains, vscode, etc.)

** note ** - recently used commands should be at the top of the list

Seems to me that the command palette functionality could be it's own crate - i would use leafwing input manager for the commands. There can be a json keybindings file that maps all commands to keys - both default and user-defined which just overrides the default keybindings. Make it work like zed as much possible as there system is easy and intuitive.

Commands will have to be bound to actions that can be executed - there's a fair bit of code in this in zed to facilitate creation of actions - which ties directly to code in the app.  In zed a lot of it is tied to GPUI but i think not all of it. Given we're going to bind many things to bevy UI and even to visualizations, we'll need to implement our own action system on the hana side.

Ideally this would be something built up front as a command system for "operating hana" will be useful for development, testing and debugging from jump.

Here's a high-level approach for just the commands
{{#include ../ai.md}}

```rust
/// Each subsystem defines their commands as a strongly typed enum
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayCommand {
    CreateWindow { name: String, size: (u32, u32) },
    CloseWindow { id: WindowId },
    SetFullscreen { id: WindowId, enabled: bool },
    // etc
}

#[derive(Debug, Clone, PartialEq)]
pub enum VisualizationCommand {
    Load { name: String },
    Unload { id: VisualizationId },
    SetParameter { id: VisualizationId, param: Parameter },
    // etc
}

/// Commands are namespaced via a top-level enum
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Display(DisplayCommand),
    Visualization(VisualizationCommand),
    Transport(TransportCommand),
    // etc
}

/// Commands can be validated and converted from strings using a path format
#[derive(Debug, Clone)]
pub struct CommandPath(String);

impl CommandPath {
    pub fn parse(path: &str) -> Result<Command, CommandError> {
        // Parse strings like "display:create_window" into Command enum
        let parts: Vec<_> = path.split(':').collect();
        match parts.as_slice() {
            ["display", "create_window"] => {
                // Parse additional args and construct DisplayCommand
                Ok(Command::Display(DisplayCommand::CreateWindow {
                    name: "default".into(),
                    size: (800, 600),
                }))
            }
            // etc
            _ => Err(CommandError::InvalidPath(path.to_string()))
        }
    }
}

/// Commands can be registered with metadata for discovery
#[derive(Debug)]
pub struct CommandRegistration {
    path: CommandPath,
    description: String,
    parameters: Vec<CommandParameter>,
}

/// A registry manages available commands and their metadata
pub struct CommandRegistry {
    commands: HashMap<CommandPath, CommandRegistration>,
}

impl CommandRegistry {
    pub fn register(&mut self, registration: CommandRegistration) {
        self.commands.insert(registration.path.clone(), registration);
    }

    pub fn execute(&self, path: &str) -> Result<(), CommandError> {
        let cmd = CommandPath::parse(path)?;
        // Dispatch command to appropriate handler
        Ok(())
    }
}
```

Key aspects of this design:

1. **Type Safety**: Commands are strongly typed enums, making invalid commands impossible to construct

2. **Namespacing**: Commands are naturally grouped by subsystem through the enum variants

3. **Discoverability**: The registry provides metadata about available commands

4. **String Interface**: Commands can be constructed from strings for external control

5. **Validation**: The parsing layer ensures only valid commands are created

This could be used in several ways:

```rust
// Internal key binding
app.bind_key(
    KeyCode::N,
    Command::Display(DisplayCommand::CreateWindow {
        name: "main".into(),
        size: (1920, 1080)
    })
);

// External control via string interface
app.execute_command("display:create_window --name main --size 1920,1080")?;

// Timeline sequencing
timeline.add_keyframe(
    TimeCode::from_secs(10.0),
    Command::Transport(TransportCommand::Play)
);
```

The benefits of this approach:

1. **Extensible**: New command namespaces can be added by adding enum variants

2. **Type-safe**: The compiler ensures commands are valid

3. **Discoverable**: Commands and their parameters can be introspected

4. **Flexible**: Supports both typed and string-based interfaces

5. **Testable**: Commands can be easily tested in isolation
