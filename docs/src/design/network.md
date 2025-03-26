# Network & Security
Overview of networking and security in hana. Our intent is to be able to create a mesh network of hana apps controlling visualizations whether the visualizations are local or remote. A hana app must be running to control a visualization and you can start a hana app on any machine on the network and it will automatically, safely and securely join the network to participate in visualization control.

## Current State
Networking is implemented in hana_network crate. Currently, we support TCP for remote comms for local, local comms over Unix Sockets on Mac/Linux

We want to make clients agnostic to the underlying transport and have networking just be easy to use.

In the current model the [hana app](./application.md) will start a process (using hana_process) and then connect to it over unix sockets to send commands. Pretty basic.

As for messages we can send, we currently support only primitive messages like `shutdown` and `ping`. But the types are in place so that we can enforce that a Sender and a Receiver are role based - i.e., the HanaApp can send messages to the Visualization process but (right now) the visualization process can't send any message back.

## Security
we don't yet have a security plan but our intent is to harden the production system using something like rustls
