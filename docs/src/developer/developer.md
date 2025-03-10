# Developer Guidelines
What you need to know to contribute to hana.
## Development Approach
- Favor rust-only dependencies to minimize complexity
- Re-use actively-maintained existing crates where possible
- Robust [error handling](../design/error_handling.md) from the start
- Secure [network communication](../design/network.md) because of well, people
- Use workspace crates for organization as well as to improve compile time
- Aim to make crates single responsibility except for the [management app](../design/application.md) itself
- Prefer turning off defaults and explicitly specifying feature dependencies - to improve compile time and limit binary size
