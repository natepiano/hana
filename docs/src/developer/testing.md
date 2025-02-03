# Testing Approach

## Overview
Testing strategy emphasizing integration tests and following bevy's testing
patterns, with a focus on non-UI components and leveraging rust's built-in testing capabilities.
## Core Testing Principles
- Favor integration tests over unit tests
- Follow bevy's testing patterns where applicable
- Leverage rust's built-in testing framework
- Use shared test utilities across workspaces
- Comprehensive CI via GitHub Actions
## Test Organization
### Workspace Structure
- Dedicated test-utils crate for shared testing code
- Integration tests in each workspace crate's `/tests` directory
- Minimal in-file unit tests following bevy's patterns
### Test Categories
1. **Core System Tests**
    - Network protocol validation
    - State synchronization
    - [Visualization System](../architecture/visualization.md) integration
    - [Configuration Management](../architecture/configuration.md)
2. **Display Tests**
    - Basic display coordination
    - Window management
    - Plugin rendering (non-UI)
    - [Resource Management](../architecture/resource.md)
3. **Management App Tests**
    - Basic state verification
    - System initialization
    - Limited frame execution tests
    - _Future consideration: Headless UI testing_
## CI/CD Integration
### GitHub Actions
- Multi-platform testing (Linux, macOS, Windows)
- Automated test running on pull requests
- Integration with code coverage tools
- Dependency caching for faster builds
### Test Environment
- Simulated network environment
- Mock display configurations
- Virtual plugin instances
- Controlled state scenarios
## Logging and Diagnostics
- Test-specific logging configurations
- Assertion tracking in parallel tests
- Performance metrics collection
- Failure diagnostics
## Future Considerations
- Headless UI testing implementation
- Performance benchmark suite
- Extended management app testing
- Network simulation framework

Note: GitHub Actions does support testing on all three major platforms
(Linux, macOS, and Windows). This capability is available through GitHub-hosted
runners, which can be specified in workflow configurations using `runs-on: [ubuntu-latest, macos-latest, windows-latest]`.

# To Investigate
Read about these in rust for rustaceans - fuzz testing is about trying random inputs to try to make your api crash. Proptest is somewhat like fuzz testing but more structured. Loom is for testing concurrent code - such as making sure thread ordering doesn't actually matter to your sync points.  Criterion is for benchmark testing.
- Cargo-fuzz and arbitrary - also proptest- and loom - all for testing
- Criterion for benchmark tests
