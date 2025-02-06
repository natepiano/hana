[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/bevyengine/bevy#license)
# Hana: Distributed Visualization Management
Hana (花) - Named after the Japanese word for "flower," reflecting the system's ability to let visualizations bloom across multiple displays.

Imagine a large scale music performance with video walls, lighting, lasers, and interactive visuals all in sync. Imagine being able to set up and control it with ease. Synchronize to the music, control it with input from keyboards, microphones, sensors and apps. Play and improvise the visualizations just as you do the music - controlling color, complexity, movement, rotation, zoom, transitions and more.

Hana is a distributed visualization management system that enables control and display of visualizations across multiple screens, devices and machines.
## Overview
Hana is a distributed visualization management system built in [rust](https://www.rust-lang.org) using the [bevy](https://bevyengine.org/) game engine. The system consists of a management application that can control both local and remote displays, with any instance capable of acting as a controller.

Visualizations are implemented as standalone binaries that be locally controller or remotely controlled by the management application. These visualizations can be developed and distributed independently, with a plugin-based system for easy integration.

The intent is to create an open source library of visualizations, ready to use and easy to integrate into the system. The system is designed to be modular and extensible, with a focus on ease of use and performance.
## Inspiration
The hana system is meant to be pluggable and modular - drawing inspiration from the plugin system, modularity, simplicity and ease of use of [vcv rack](https://vcvrack.com) software as well as inspiration from bevy to make an application interface that is as easy to use as a game.
## Documentation
[docs on github.io](https://natemccoy.github.io/hana/)
## Goals and Features
### High-Level Goals
- Enable seamless distributed visualization across monitors (and more) attached to computers on a local network.
- Provide an intuitive 3D management interface.
- Support a wide range of input devices and control methods.
- Facilitate the creation and sharing of visualizations.
### Key Features
- Cross-platform compatibility (Windows, macOS, Linux).
- Any instance is capable of acting as a controller.
- Plugin-based visualization system with library of reusable plugins
### Use cases
- Audio/visual installations both interactive and evolving/emergent
- Live performance incorporating real time improvisational control of visualizations
- Educational and artistic exploration of visualizations
## License
Hana is free, open source and permissively licensed!
Except where noted (below and/or in individual files), all code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.
This means you can select the license you prefer!

### Your contributions

Unless you explicitly state otherwise,
any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license,
shall be dual licensed as above,
without any additional terms or conditions.
