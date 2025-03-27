# Workspace Organization
{{#include ../misc/ai.md}}
## Purpose
We're using a workspace to save on compile times and to keep the code organized. The workspace is defined in the root `Cargo.toml` file.

## Structure
```
hana/
├── Cargo.toml            # Workspace manifest
├── crates/
│   ├── hana              # management app
│   ├── hana-display/     # Display management
│   ├── hana-network/     # Network functionality
│   ├── hana-plugin/      # Plugin system
│   ├── hana-config/      # Configuration
│   ├── hana-input/       # Input handling
│   └── hana-state/       # State management
├── docs/
│ ├── src                  # mdbook docs root
│ │ ├── design             # design docs
│ │ └── developer          # dev docs
│ ├── user                 # hana management app docs
│ └── visualization        # visualization sdk docs
├── examples/
│ └── basic_visualization  # example visualization
└── target/                # Single shared target directory
```

## Guidelines

### Crate Organization
- Each major system component gets its own crate
- Keep dependencies minimal and explicit per crate
- Avoid duplicating heavy dependencies across crates
- Use feature flags to control optional functionality

### Version Management
- Maintain consistent version numbers across workspace
- Use workspace-level dependency definitions
- Document breaking changes between crates

## Review Checklist
- [ ] Crate boundaries follow system components
- [ ] Dependencies minimized per crate
- [ ] Breaking changes documented
- [ ] Workspace dependencies consistent
