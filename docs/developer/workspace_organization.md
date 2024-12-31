# Workspace Organization

## Structure
```
hana/
├── Cargo.toml           # Workspace manifest
├── apps/
│   └── hana/           # Main application
│       ├── Cargo.toml  
│       └── src/
├── crates/
│   ├── hana-display/   # Display management
│   ├── hana-network/   # Network functionality 
│   ├── hana-plugin/    # Plugin system
│   ├── hana-config/    # Configuration
│   ├── hana-input/     # Input handling
│   └── hana-state/     # State management
├── docs/               
├── examples/
└── target/             # Single shared target directory
```

## Guidelines

### Crate Organization
- Each major system component gets its own crate
- Keep dependencies minimal and explicit per crate
- Avoid duplicating heavy dependencies across crates
- Use feature flags to control optional functionality

### Dependencies
```toml
# Root Cargo.toml
[workspace]
members = [
    "apps/hana",
    "crates/hana-display",
    "crates/hana-network",
    "crates/hana-plugin",
]

[workspace.dependencies]
bevy = { version = "0.15", default-features = false }
```

### Version Management
- Maintain consistent version numbers across workspace
- Use workspace-level dependency definitions
- Document breaking changes between crates

## Review Checklist
- [ ] Crate boundaries follow system components
- [ ] Dependencies minimized per crate
- [ ] Breaking changes documented
- [ ] Workspace dependencies consistent

## Doc Links
- [Architecture](../architecture/README.md)
- [Developer](../developer/README.md)
- [Overview](../../README.md)
- [Plugin Development](../plugins/README.md)
- [User](../user/README.md)
