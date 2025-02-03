# Developer Guidelines
What you need to know to contribute to hana.
## Development Approach
- Favor rust-only dependencies to minimize complexity
- Re-use existing crates wherever possible
- Robust [error handling](../architecture/error_handling.md) from the start
- Secure network communication because of well, people
- Use workspace crates for organization as well as to improve compile time
- Try to keep workspaces single purpose to avoid needing to repeat large dependencies (such as[bevy](https://bevyengine.org/)) across them
- Prefer turning off defaults and explicitly specifying feature dependencies - to improve compile time and limit binary size
## TOC
- [Contributing ](./contributing.md)
- [Code Organization](./code_organization.md)
- [Documentation Guidelines](documentation.md)
- [Error Handling](../architecture/error_handling.md)
- [Testing Approach](testing.md)
- [Performance Testing](performance_testing.md)
- [Versioning](versioning.md)
- [Workspace Organization](workspace_organization.md)
