# Hana - Create Visualizations
Hana (花) - Named after the Japanese word for "flower," reflecting the system's ability to let visualizations bloom in a rich multi-device environment.

Imagine a large scale music performance with video walls, lighting, lasers, and interactive visuals all in sync. You can set it up with ease, synchronize the visuals to the music, control them with input from keyboards, microphones, sensors and apps. Play and improvise the visualizations just as you do the music - controlling color, complexity, movement, rotation, zoom, transitions and more.

Hana allows you to make these visualizations in a game-like environment where everything just works as you expect. And delights you with thoughtfulness in ease of use.

Hana allows you to work together in a multi-creator environment.

Hana is a distributed visualization management system that enables efficient and intuitive control and display of visualizations across multiple screens, devices and machines.
## Overview
Built in [rust](https://www.rust-lang.org) using the [bevy](https://bevyengine.org/) game engine, hana consists of a `node editor`, an `environment editor`, and a `player`.

The node editor is the heart - allowing you to make beautiful visualizations.

Nodes themselves can be implemented as plugins, allowing for people to create open source versions and allow also for a marketplace to emerge.

The environment editor is where you place screens, projectors, lights, lasers, sensors. The environment editor is also where you attach machines to devices so that you can visualize the network both in terms of assignment of compute responsibilities but also in terms of physical location.

The player runs visualizations on devices like screens and projectors. The player can run on multiple machines in a mesh network synchronizing timing with each other and controlling their own local devices. If you need additional GPUs, or device connections, just add more machines running player.

Visualizations will either run in a player - or bevy authors can use a plugin crate that provides the network and control protocol so that people can code their own visualizations however they like.

Potentially we can make a library that provides the wire protocol in a way that can be linked into programs written in other languages - aspirational.

Hana will have a cloud-based library for people to find open source nodes to install or commercial nodes to purchase.

The library can also host saved node graphs and environments to allow for content sharing and examples for many purposes.

The intent is to create an open source library of nodes, environments and visualizations (node graphs), ready to use and easy to integrate into their system.

Hana is designed to be modular and extensible, with a focus on ease of use and performance.

Everything should just work as you expect.
## Inspiration
The hana system is meant to be pluggable and modular - drawing inspiration from the plugin system, modularity, simplicity and ease of use of [vcv rack](https://vcvrack.com) software as well as inspiration from bevy to make a management app that is as easy to use as a game.
## Repository
[hana](https://github.com/natepiano/hana)
## Goals and Use Cases
### High-Level Goals
- Enable seamless distributed visualization across screens, projectors and other devices connected to computers on a local network.
- Provide an intuitive 3D management interface.
- Support a wide range of input devices and control methods.
- Facilitate the creation and sharing of visualizations.
### Use cases
- Audio/visual installations both interactive and evolving/emergent
- Live performance incorporating real time improvisational control of visualizations
- Educational and artistic exploration of visualizations
- Fun
## License
Hana is free, open source and permissively licensed!
Except where noted (below and/or in individual files), all code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](https://github.com/natepiano/hana/LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/natepiano/hana/LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.
This means you can select the license you prefer!

### Your contributions

Unless you explicitly state otherwise,
any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license,
shall be dual licensed as above,
without any additional terms or conditions.
