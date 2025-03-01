# Comprehensive Migration Plan for Hana Network Redesign

## High-Level Design

### Core Components

1. **Transport Layer**
   - `Transport` trait: Abstracts network communication
   - Platform-specific implementations: TCP, Unix pipes, Windows named pipes
   - Factory functions for creating appropriate transports

2. **Role System**
   - `Role` trait: Defines capabilities and behavior
   - `Controller` role: Controls visualizations
   - `Visualization` role: Receives and responds to control commands

3. **Endpoint API**
   - Generic over transport and role
   - Type-safe message sending/receiving
   - Connection establishment

4. **Message System**
   - Serializable message types
   - Role-specific message handling

### Module Structure

```
hana_network/
├── src/
│   ├── lib.rs                 # Public API and re-exports
│   ├── error.rs               # Error types
│   ├── message.rs             # Message trait and implementations
│   ├── endpoint.rs            # Endpoint implementation
│   ├── transport/
│   │   ├── mod.rs             # Transport trait and factories
│   │   ├── tcp.rs             # TCP transport implementation
│   │   ├── local.rs           # Platform-agnostic local API
│   │   ├── unix.rs            # Unix-specific implementation
│   │   └── windows.rs         # Windows-specific implementation
│   └── role/
│       ├── mod.rs             # Role trait definition
│       ├── controller.rs      # Controller implementation
│       └── visualization.rs   # Visualization implementation
```

## Detailed Migration Plan

### Phase 1: Transport Layer Foundation

#### Step 1.1: Create Transport Trait
#### Step 1.2: Implement TCP Transport
```rust
// in transport/tcp.rs
pub struct TcpTransport(pub(crate) TcpStream);

```

#### Step 1.3: Create Factory Functions
```rust
// in transport/mod.rs
pub async fn connect_remote(address: &str) -> Result<TcpTransport> {
    let stream = TcpStream::connect(address).await?;
    Ok(TcpTransport(stream))
}
```
### Phase 2: skip this
### Phase 3: Refactor Endpoint for Transport Abstraction

#### Step 3.1: Update Endpoint Structure
```rust
// in endpoint.rs
pub struct Endpoint<R: Role, T: Transport> {
    role: PhantomData<R>,
    transport: T,
}

impl<R: Role, T: Transport> Endpoint<R, T> {
    pub fn new(transport: T) -> Self {
        Self {
            role: PhantomData,
            transport,
        }
    }
}
```

#### Step 3.2: Implement Message Sending/Receiving
```rust
impl<R: Role, T: Transport> Endpoint<R, T> {
    pub async fn send<M>(&mut self, message: &M) -> Result<()>
    where
        M: HanaMessage + Debug,
        R: Sender<M>,
    {
        // Serialize and send message using transport
    }

    pub async fn receive<M: HanaMessage>(&mut self) -> Result<Option<M>>
    where
        R: Receiver<M>,
    {
        // Receive and deserialize message using transport
    }
}
```

### Phase 4: Local Transport Implementation

#### Step 4.1: Unix Transport (conditionally compiled)
```rust
// in transport/unix.rs
#[cfg(unix)]
pub struct UnixTransport(pub(crate) UnixStream);

#[cfg(unix)]
pub async fn connect_local(path: &str) -> Result<UnixTransport> {
    let stream = UnixStream::connect(path).await?;
    Ok(UnixTransport(stream))
}
```

#### Step 4.2: Windows Transport (conditionally compiled)
```rust
// in transport/windows.rs
#[cfg(windows)]
pub struct NamedPipeTransport(pub(crate) NamedPipe);

#[cfg(windows)]
pub async fn connect_local(path: &str) -> Result<NamedPipeTransport> {
    // Windows-specific implementation
}
```

#### Step 4.3: Platform-Agnostic Local API
```rust
// in transport/local.rs
pub async fn connect_local(name: &str) -> Result<impl Transport> {
    #[cfg(unix)]
    return unix::connect_local(&format!("/tmp/hana_{}.sock", name)).await;

    #[cfg(windows)]
    return windows::connect_local(&format!("\\\\.\\pipe\\hana_{}", name)).await;

    #[cfg(not(any(unix, windows)))]
    return Err(Error::UnsupportedPlatform);
}
```

### Phase 5: Enhanced Public API

#### Step 5.1: High-Level Connection Functions
```rust
// in lib.rs
pub async fn connect_to_local_visualization() -> Result<Endpoint<Controller, impl Transport>> {
    let transport = transport::connect_local("visualization").await?;
    Ok(Endpoint::new(transport))
}

pub async fn connect_to_remote_visualization(address: &str) -> Result<Endpoint<Controller, impl Transport>> {
    let transport = transport::connect_remote(address).await?;
    Ok(Endpoint::new(transport))
}

pub async fn listen_for_controller(name: &str) -> Result<Endpoint<Visualization, impl Transport>> {
    let transport = transport::listen_local(name).await?;
    Ok(Endpoint::new(transport))
}
```

### Phase 6: Integration with Existing Code

#### Step 6.1: Backward Compatibility Wrappers
```rust
// in lib.rs - temporary compatibility layer
#[deprecated(note = "Use connect_to_local_visualization instead")]
pub async fn connect() -> Result<Endpoint<Controller, impl Transport>> {
    connect_to_local_visualization().await
}
```

## Testing Plan for Each Phase

1. **Unit Tests**
   - Transport implementations
   - Role functionality
   - Message serialization/deserialization

2. **Integration Tests**
   - Local communication (Controller ↔ Visualization)
   - Remote communication over TCP
   - Cross-platform compatibility

3. **Benchmark Tests**
   - Message throughput
   - Connection establishment time
   - Memory usage

## Implementation Strategy

### For Each Phase:

1. Create new files without modifying existing code
2. Implement and test new functionality in isolation
3. Create bridge code to maintain compatibility
4. Gradually migrate callers to new API
5. Remove deprecated functionality when no longer needed

### Cross-Platform Considerations:

1. Use conditional compilation (`#[cfg(unix)]`, `#[cfg(windows)]`)
2. Provide platform-agnostic interfaces
3. Test on all target platforms regularly
4. Handle platform-specific errors gracefully

This plan provides a comprehensive roadmap for migrating to the new architecture while maintaining compatibility with existing code. Each phase can be implemented and tested independently, reducing risk and allowing for incremental progress.
