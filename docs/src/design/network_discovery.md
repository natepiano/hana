# Hana Network Discovery Protocol

To maximize compute and gpu - there shouldn't have to be a hana app running on a machine but the machine could just be running visualization(s). After all the hana app is a bevy app making heavy use of the local GPU - which may be fine but it would be nice to have the option for the visualization to assume full control of the GPU - especially when driving multiple screens/projectors.

Additionally anyone that wants to control visualizations on the mesh needs to run a Hana App, connect to the network and start controlling visualizations (or even change the overall hana network configuration)

To accomplish this, we propose a UDP discovery protocol for the mesh network.

# Design
{{#include ../ai.md}}

**And...**
that said, I spent some serious time getting this design in place so I think it's pretty solid and close to what hana actually needs.

## Overview
### Motivation
1. **Simplicity**: One discovery mechanism to implement and maintain
2. **Flexibility**: Works in ad-hoc environments without installation
3. **Immediacy**: Visualizations are usable as soon as they start
4. **Portability**: Works across different platforms consistently

### Approach
Every Hana instance (management app or visualization) participates in discovery:
- UDP broadcast/multicast for discovery
- Direct TCP connections for control
- No central coordination point

**Pros:**
- No single point of failure
- Works in ad-hoc environments
- Simple to bootstrap

**Cons:**
- More complex to maintain consistency
- Each node needs discovery logic
- Harder to scale to very large network

### Discovery Protocol
- UDP multicast/broadcast announcements containing:
  - Service type (management app, visualization, daemon)
  - Connection information (IP, port)
  - Capabilities (GPU, special hardware)
  - Resource availability

- Periodic heartbeats to maintain "alive" status
- Query mechanism to request specific services
- Same connection pattern whether local or remote
- Minimal special casing for different scenarios

### Connection Management

- Direct TCP connections for reliable remote control
- Unix Sockets for reliable local control
- (maybe) Connection pooling for efficient communication
- (maybe) WebRTC-inspired NAT traversal techniques for difficult network scenarios

### Use Cases

1. **Local hana/visualization communication**:
   - Use Unix domain sockets/Named Pipes when on same machine
2. **Remote hana/visualization communication**:
   - Discover visualizations via UDP protocol
   - Connect directly via TCP, or
3. **Remote hana/hana communication**:
   - Use the same discovery protocol
   - Establish mesh network of management instances
   - Use a leadership election protocol to prevent conflicts

## Implementation Plan

1. Implement a `DiscoveryService` that:
   - Broadcasts presence periodically
   - Listens for other services
   - Maintains a registry of available nodes

2. Extend your current `TransportConnector` to leverage discovery:
   - Add remote connection capabilities
   - Abstract connection source (local vs remote)

3. Develop a simple protocol for:
   - Service advertisement
   - Capability queries
   - Status updates

This approach gives you a usable distributed system with minimal architectural complexity. If you later determine that a daemon adds value (perhaps for resource management or persistent configuration), you can add it as an enhancement rather than a core requirement.

### Package Structure

```
hana/crates/hana_discovery/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── node.rs         # Node types and traits
│   ├── protocol.rs     # Discovery protocol messages
│   ├── service.rs      # Discovery service implementation
```

### Node Model Using Existing Roles

We should reuse the `hana_network::role` to maintain consistency. Here's how the node system could work:

```rust
use hana_network::role::{Role, HanaRole, VisualizationRole};

/// Represents a node in the Hana network
pub struct Node<R: Role> {
    id: NodeId,
    role: PhantomData<R>,
    address: SocketAddr,
    capabilities: Capabilities,
    last_seen: Instant,
}

/// Type aliases for convenience
pub type HanaNode = Node<HanaRole>;
pub type VisualizationNode = Node<VisualizationRole>;

/// A registry of discovered nodes
pub struct NodeRegistry {
    hana_nodes: HashMap<NodeId, HanaNode>,
    visualization_nodes: HashMap<NodeId, VisualizationNode>,
    // ...
}
```

### Discovery Service API

```rust
pub struct DiscoveryService {
    registry: NodeRegistry,
    // UDP socket for discovery
    // Periodic announcement task
    // Discovery listener task
}

impl DiscoveryService {
    /// Start the discovery service
    pub async fn start() -> Result<Self>;

    /// Register a callback for when nodes are discovered/lost
    pub fn on_node_change<F>(&mut self, callback: F)
    where F: Fn(NodeEvent) + Send + 'static;

    /// Get all currently known Hana nodes
    pub fn hana_nodes(&self) -> Vec<HanaNode>;

    /// Get all currently known visualization nodes
    pub fn visualization_nodes(&self) -> Vec<VisualizationNode>;

    /// Connect to a specific node
    pub async fn connect_to_node<R: Role>(&self, node_id: NodeId) -> Result<Endpoint<R>>;
}
```

### Integration with Existing Code

This discovery service would plug into your current architecture:

1. When a Hana app starts, it initializes the discovery service
2. It can immediately discover local visualizations via socket pairs
3. The discovery service finds remote nodes via UDP
4. Your existing connection code can be used with the discovered addresses

## Benefits of This Approach

1. **Clean Separation**: Discovery logic contained in one package
2. **Role Consistency**: Reusing your existing role system
3. **Extensibility**: Easy to add new node types later
4. **Simplicity**: Single discovery mechanism for all scenarios

## Integration Testing for Hana Discovery Scenarios

Creating effective integration tests for distributed network scenarios requires careful consideration of the testing environment.

### Testing Technology Stack

1. **Tokio's Multi-Threaded Runtime**
   - Perfect for testing asynchronous Rust code
   - Allows simulating multiple independent processes
   - Built-in utilities for timing and coordination

2. **Docker for Cross-Machine Testing**
   - Allows creating isolated network environments
   - Simulates true multi-machine scenarios
   - Consistent test environment across development machines


### Integration Test Implementation

#### In-Process Multi-Node Tests

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_local_discovery() {
    // Create temporary socket directories
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("hana-test.sock");

    // Configure environment to use test paths
    std::env::set_var("HANA_SOCKET_PATH", socket_path.to_str().unwrap());

    // Start a visualization node in a separate task
    let visualization_handle = tokio::spawn(async {
        let mut app = TestVisualizationApp::new();
        app.start().await;
        // Keep running for test duration
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    // Brief delay to ensure visualization starts
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Start a Hana node that should discover the visualization
    let hana_handle = tokio::spawn(async {
        let mut app = TestHanaApp::new();
        app.start().await;

        // Assert that discovery happened
        assert_eq!(app.discovery_service.visualization_nodes().len(), 1);

        // Test control operations
        let result = app.send_ping_to_all().await;
        assert!(result.is_ok());
    });

    // Wait for both tasks to complete
    let _ = tokio::join!(visualization_handle, hana_handle);
}
```

### Basic Test Scenarios

#### 1. Hana Local, Visualization Local
- **Configuration**: Single machine running both Hana app and visualization
- **Expected Behavior**:
  - Hana discovers and connects to local visualization
  - Full control of visualization through local interface
  - Socket pair communication for efficiency
- **Test Focus**:
  - Discovery of local processes
  - Performance with direct local connections
  - Resource sharing on single machine

#### 2. Hana Local, Visualization Remote
- **Configuration**: Hana app on one machine, visualization on another
- **Expected Behavior**:
  - Hana discovers remote visualization over network
  - Control messages transmitted over TCP
  - Possible latency in control response
- **Test Focus**:
  - Network discovery mechanisms
  - Connection establishment across machines
  - Handling of network interruptions

### Advanced Scenarios

#### 3. Hana Local, Visualization Local, Hana Remote
- **Configuration**: Primary Hana on machine A with visualization, secondary Hana on machine B
- **Expected Behavior**:
  - Both Hana instances discover same visualization
  - Control can happen on both Hana instances
  - Changes made by one Hana reflected in the other
- **Test Focus**:
  - Coordination between Hana instances
  - Control synchronization
  - Resource contention handling

#### 4. Multiple Hana Instances, Multiple Visualizations (Mixed Local/Remote)
- **Configuration**: Network of 3+ machines with Hana and visualization instances distributed
- **Expected Behavior**:
  - All Hana instances discover all visualizations
  - Control can be coordinated across the network
  - System handles dynamic joining/leaving of nodes
- **Test Focus**:
  - Network mesh formation
  - Scalability of discovery protocol
  - Control coordination across multiple machines
  - Handling partial network failures

### Special Test Cases

#### 5. Dynamic Network Topology Changes
- **Configuration**: Start with stable network, then add/remove machines
- **Expected Behavior**:
  - System detects new Hana/visualization instances as they appear
  - Gracefully handles disconnection of controlled visualizations
  - Maintains consistent state across available nodes
- **Test Focus**:
  - Discovery resilience
  - Connection recovery
  - State consistency

#### 6. Input Device Routing
- **Configuration**: Hana with special input devices controlling remote visualization
- **Expected Behavior**:
  - Input from MIDI controllers, sensors routed properly to remote visualizations
  - Real-time responsiveness of visualizations to inputs
- **Test Focus**:
  - Input latency
  - Device discovery and mapping
  - Real-time performance

#### 7. High-Load Collaborative Control
- **Configuration**: Multiple Hana instances controlling shared visualizations
- **Expected Behavior**:
  - Control inputs from multiple sources properly merged
  - Visualization state remains consistent across observers
  - System handles concurrent modification requests
- **Test Focus**:
  - Conflict resolution
  - Update propagation
  - Control priority management

####   8. Cross-Platform Operation
- **Configuration**: Mix of operating systems running Hana and visualizations
- **Expected Behavior**:
  - Consistent discovery and control across platforms
  - Adaptable transport mechanisms (Unix sockets vs Windows pipes locally)
- **Test Focus**:
  - Platform-specific behaviors
  - Transport abstraction effectiveness

#### Docker-Based Multi-Machine Tests

Create a test harness that:

1. Builds Docker containers with your Hana binaries
2. Creates a Docker network for isolation
3. Runs containers with different roles
4. Executes test scenarios
5. Verifies results

```rust
// In tests/docker_integration.rs
#[test]
#[ignore] // Run explicitly with cargo test -- --ignored
fn test_remote_discovery() {
    // Requires Docker CLI to be available
    let docker_available = std::process::Command::new("docker")
        .arg("--version")
        .output()
        .is_ok();

    if !docker_available {
        eprintln!("Docker not available, skipping multi-machine test");
        return;
    }

    // Create Docker test harness
    let mut harness = DockerTestHarness::new();

    // Add containers
    harness.add_container("hana1", ContainerConfig::hana());
    harness.add_container("vis1", ContainerConfig::visualization());
    harness.add_container("hana2", ContainerConfig::hana());

    // Run the test
    let results = harness.run_test_scenario(Scenario::RemoteDiscovery);

    // Verify expected outcomes
    assert!(results.all_hana_discovered_visualization);
    assert!(results.control_operations_succeeded);
}
```

### Custom Test Utilities

#### `TestNetworkSimulator`

Create a utility that simulates network conditions:

```rust
struct TestNetworkSimulator {
    // Simulates network latency, packet loss, etc.
}

impl TestNetworkSimulator {
    fn introduce_partition(&mut self, node_a: &str, node_b: &str) {
        // Prevent communication between nodes
    }

    fn introduce_latency(&mut self, node: &str, latency_ms: u64) {
        // Add delay to all packets from node
    }

    fn heal_network(&mut self) {
        // Restore normal conditions
    }
}
```

#### `TestMetricsCollector`

```rust
struct TestMetricsCollector {
    discovery_times: Vec<Duration>,
    message_latencies: Vec<Duration>,
    packet_counts: HashMap<PacketType, usize>,
}
```

## Test Implementation Plan

1. **Start with in-process tests**
   - Test basic discovery and communication
   - Mock network interfaces where needed

2. **Develop Docker-based harness**
   - Create reusable test infrastructure
   - Implement scripts to build and run containers

3. **Implement cross-machine scenarios**
   - Add scenario implementations for each test case
   - Include metrics collection

4. **Add network condition simulation**
   - Test behavior under suboptimal network conditions
   - Verify recovery capabilities

## Continuous Integration Integration

Configure your CI pipeline to:

1. Run the in-process tests on every commit
2. Run Docker-based tests on scheduled intervals or pre-release
3. Store test metrics for performance tracking

Would you like me to elaborate on any specific part of this testing approach or provide a more detailed implementation of any component?

## future directions
if we ever need to scale beyond a local network's capacity, we can introduce a central discovery service connecting hana subnets together. That might be necessary at a football game when there are hundreds of hana nodes and tens of thousands of users connecting - ha!
