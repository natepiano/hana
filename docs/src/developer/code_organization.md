# Code Organization

## File Structure Within Crates

```
crate/
├── src/
│   ├── lib.rs           # Library root, public exports
│   ├── display/         # Major subsystem = folder
│   │   ├── mod.rs       # Public exports and shared types
│   │   ├── window.rs    # Window management
│   │   └── types.rs     # Shared display types
│   ├── config.rs        # Simple system = single file
│   └── network/         # Will grow = folder
│       └── mod.rs       # Start simple, split later
├── Cargo.toml
└── README.md
```

## When to Split Files

Start with a single file until you hit these thresholds (use common sense - these are just guidelines)
- File exceeds ~300 lines
- Multiple developers frequently working on different sections
- Clear separation of concerns emerges
- Changes to one part shouldn't force recompilation of others

## File Organization

### Simple File (< ~300 lines)
```rust
// 1. Imports
use std::sync::Arc;
use bevy::prelude::*;

// 2. Constants
const MAX_WINDOWS: u32 = 64;

// 3. Types and their implementations
pub struct Window {
    id: WindowId,
    size: (u32, u32),
}

impl Window {
    pub fn new(id: WindowId) -> Self {
        Self {
            id,
            size: (800, 600),
        }
    }
}

// 4. Additional traits
impl Default for Window {
    fn default() -> Self {
        Self::new(WindowId(0))
    }
}

// 5. Tests at the end
#[cfg(test)]
mod tests {
    use super::*;
    // tests...
}
```

### Complex File (Consider Splitting)
```rust
// Types grouped at top for API clarity
pub struct Window { /* ... */ }
pub struct WindowManager { /* ... */ }
pub enum WindowState { /* ... */ }

// Implementations follow their types
impl Window { /* ... */ }
impl WindowManager { /* ... */ }

// Trait definitions with their implementations
pub trait WindowControl { /* ... */ }
impl WindowControl for Window { /* ... */ }

// Tests last
#[cfg(test)]
mod tests { /* ... */ }
```

## When to Use Folders vs Files

### Use a Single File When:
- Functionality is cohesive
- Under ~300 lines
- Minimal subcomponents
- Changes are usually all-or-nothing

### Use a Folder (with mod.rs) When:
- Multiple related but distinct components
- Need to break up code for clarity
- Expect future growth
- Have private internal modules

## Type Organization Tradeoffs

### Keep Types Together When:
- Many small, related types (enums, small structs)
- Types form a cohesive API
- Documentation focuses on type relationships
- Implementations are simple

### Keep Types with Implementations When:
- Complex implementations
- Implementation details crucial to understanding type
- Frequent changes to specific type/implementation pairs
- Heavy testing requirements for specific types

## Module Guidelines

### Module Size
- Start with a single file
- Split at ~300 lines if clear divisions exist
- Consider splitting sooner if multiple developers work on different parts
- Err on the side of fewer, larger files early in development

### File Naming
- Use noun-based names for type-focused modules (`window.rs`)
- Use verb-based names for action-focused modules (`render.rs`)
- Prefer clear, full words over abbreviations
- Use snake_case

### Visibility
- Keep module-private items truly private
- Export only what other modules need
- Use pub(crate) for truly internal APIs

## Testing Organization

### Small Module Unit Tests
```rust
// At the bottom of the file
#[cfg(test)]
mod tests {
    use super::*;
    // tests...
}
```

### Large Module Unit Tests
```
module/
├── mod.rs
├── implementation.rs
└── tests/
    ├── mod.rs
    └── integration.rs
```
### Integration Tests
use ./tests folder in the crate root for integration tests as is typical
for rust projects

## Review Checklist

Before committing new code organization:
- [ ] Files have clear, single responsibilities
- [ ] Public API is clearly documented
- [ ] Tests are logically organized
- [ ] Related code stays together
- [ ] Module structure matches project needs
- [ ] Changes minimize compilation impact
