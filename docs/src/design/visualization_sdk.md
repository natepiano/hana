# Visualization SDK
{{#include ../misc/ai.md}}
## Purpose
Describe the Visualization SDK and its role in the hana system (TBD)

Basically, this is the API that developers will use to create visualizations. It will be a Rust library that provides the necessary functionality to create visualizations and communicate with the hana system.

The library will likely consist of a bevy plugin that provides the necessary functionality to create visualizations and communicate with the hana system given that bevy is the rendering engine of choice for hana - and being able to rely on the bevy ecosystem for visualization development is a huge win.

Maybe there could be a core API that is implemented in rust and then a bevy plugin that wraps the API for bevy developers. This would allow for a more generic API that can be implemented in other rust-based rendering engines.
