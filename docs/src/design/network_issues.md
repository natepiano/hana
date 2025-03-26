# Network

## Issues
All of these are basically issues to implement - we can eventually migrate this  list to github when you're working with other devs

- We need a policy and a UX to handle local process failure - whether the process is shutdown manually or just becomes unresponsive. There's a lot of choices we could make and a lot of things we could do to make it so users can recover from these situations with the least amount of fuss.
- Buffered comms
- Lanes as in [aeronet](https://github.com/aecsocket/aeronet/blob/main/crates/aeronet_transport/src/lane.rs) for bevy - supporting unordered reliable, unordered unreliable, ordered reliable and unordered reliable. Each have use cases
- handle network errors in the hana_plugin when network failures occur
- decide whether we want to have a hanad (or hana_d or hana_daemon) whose sole responsibility is to provide network discovery for every machine it's running on - and then allow for starting remote visualizations without necessarily needing to run a local [hana app](./application.md). And if a hana app come online, it can connect via hanad rather than have hana itself be responsible for the discovery process. Advantages include
  - No cpu/gpu on remote machines required to run a hana app
  - lightweight and can easily just be started up and remain listening on any machine in the mesh
- decide which or the capabilities from the network discussion below need to be implemented
- support multiple simultaneous connections -
  - mesh control for starting/stopping processes and heartbeat/health - one message at a time, must be reliable but doesn't have to be superfast
  - modulation control - probably more like a streaming protocol
  - timing and play/pause control - provided by ableton link (tbd)
  - communication between hana apps on the mesh
  - connection of a particular hana app instance to a particular visualization to update its modulation / play it in real time
  - decide whether messages (and roles?) need to come out of hana_network app as these are higher level constructs
  - decide whether a Visualization can implement their own message types (how would they define the hana app side?) possible use cases could be visualizations talking to each other on multiple machines in the mesh?
