# Node Editor
For the time being, this doc is for design notes and research for the node editor. It won't have a structure until one emerges.
## voxel world
an idea is to have nodes hanging in space - occupying slots in an oktree for a voxel type world. I think possibly just using [oktree](https://crates.io/crates/oktree) could be a direction to explore. And in fact the visualization on the readme, scaled down, could be an attract screen for the node editor - I do believe.
## the plane
and the voxel world attract mode could be sitting above this plane which acts as a visual cue as to where you are at. the plane itself could look a little like one of these, maybe.
- bevy_debug_grid looks cool
- bevy_infinite_grid
