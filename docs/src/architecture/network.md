# Network Architecture

## Current State
networking is implemented in hana_network crate. Currently we support TCP for remote comms for local, IPC over Unix Sockets on Mac/Linux and Named Pipes over windows.

We want to make clients agnostic to the underlying transport and have networking just be easy to use.

In the current model the [hana app](./application.md) will start a process (using hana_process) and then connect to it over IPC to send commands. Pretty basic.

We currently support only primitive messages like `shutdown` and `ping`. But the types are in place so that we enforce that a Sender and a Receiver are role based - i.e., the HanaApp can send messages to the Visualization process but (right now) the visualization process can't send any message back.

## Issues
eventually migrate this issue list to github when you're working with other devs

- We need a policy and a UX to handle local process failure - whether the process is shutdown manually or just becomes unresponsive. There's a lot of choices we could make and a lot of things we could do to make it so users can recover from these situations with the least amount of fuss.
- Buffered comms
- Lanes as in [aeronet](https://github.com/aecsocket/aeronet/blob/main/crates/aeronet_transport/src/lane.rs) for bevy - supporting unordered reliable, unordered unreliable, ordered reliable and unordered reliable. Each have use cases
- decide whether we want to have a hanad (or hana_d or hana_daemon) whose sole responsibility is to provide network discovery for every machine it's running on - and then allow for starting remote visualizations without necessarily needing to run a local [hana app](./application.md). And if a hana app come online, it can connect via hanad rather than have hana itself be responsible for the discovery process. Advantages include
  - No cpu/gpu on remote machines required to run a hana app
  - lightweight and can easily just be started up and remain listening on any machine in the mesh
- decide which or the capabilities from the network discussion below need to be implemented
- support multiple simultaneous connections -
  - mesh control for starting/stopping processes and heartbeat/health - one message at a time, must be reliable but doesn't have to be super fast
  - modulation control - probably more like a streaming protocol
  - timing and play/pause control - provided by ableton link (tbd)
  - communication between hana apps on the mesh
  - connection of a particular hana app instance to a particular visualization to update it's modulation / play it in real time
  - decidee whether messages (and roles?) need to come out of hana_network app as these are higher level constructs
  - decide whether a Visualization can implement their own message types (how would they define the hana app side?) possible use cases could be visualizations talking to each other on multiple machines in the mesh?

# Network discussion with claude 3.5
Everything below just came from an claude-assisted dialog - not necessarily everything will be as is described below.
## Purpose
Enables reliable peer-to-peer communication and display coordination across the hana network. Focused on network transport, topology management, and connection health.
## Core Responsibilities
- Peer discovery and connection management
- Reliable message transport (UDP/TCP)
- Connection health monitoring
- Network topology maintenance
- Basic failure detection
- Network diagnostics and metrics
## Network Transport Layer
### Message Delivery
- Reliable message delivery API for state propagation
- Message prioritization and ordering
- Delivery confirmation and retry logic
- Bandwidth management
- Latency optimization
### Connection Management
- Peer discovery and connection establishment
- Connection health monitoring
- Basic failure detection
- Reconnection handling
- Network topology updates
### Network Status
- Connection state tracking
- Peer availability monitoring
- Network health metrics
- Performance diagnostics
- Bandwidth utilization tracking
## Interfaces
### [State Management](./state.md) Interface
- Reliable message delivery API for state updates
- Peer status change notifications
- Network health status updates
- Connection event notifications
### Display Management Interface
- Display availability updates
- Display network status
- Connection quality metrics
## Failure Handling
### Connection Issues
- Connection loss detection
- Reconnection attempts
- Peer status updates
- Network topology updates
### Performance Issues
- Bandwidth throttling
- Message prioritization
- Congestion detection
- Performance metrics
## Network Configuration
### Topology
- Peer-to-peer mesh within local network
- All peers on same subnet
- Up to 100 peer limit
- Dynamic role assignment
### Monitoring
- Real-time connection status
- Display synchronization metrics
- Network performance statistics
