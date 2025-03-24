[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/bevyengine/bevy#license)
# Hana: Generative Art and Distributed Performance Tool
Hana (花) - Named after the Japanese word for "flower," reflecting the system's ability to let visualizations bloom across multiple displays.

Hana is a generative art and distributed visualization performance system that enables creation, control and display of visualizations across multiple screens, devices and machines.

## Documentation
[docs on github.io](https://natepiano.github.io/hana/)

## try me
```shell
git clone https://github.com/natepiano/hana.git
cd hana
cargo build --workspace
cargo run
```
we have the build --workspace to ensure the examples/basic-visualization is built before running

As of the current version, the hana binary doesn't have any much functionality yet. It allows you to launch a basic visualization in a separate process, ping it with a command that speeds up the animation and then allows you to shut down the visualization.

So that's intentional :) just shut down main hana window when you're ready to.

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
