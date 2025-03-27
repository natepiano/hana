# ux
For the time being, this doc is for design notes and research for the UX. It won't have a structure until one emerges.
# sysinfo
We want to show the FPS and other useful things like CPU utilization, GPU utilization, etc. Probably on its own standalone node that you can spin around to see different metrics? And some of it may be actually control systems i.e., specifying a desired frame rate (maybe use bevy_framespace as the limiter) and showing the actual frame rate. You'd want that for development anyway.
# vector drawing
I'm not sure yet if we'll use vector drawing to draw ux widgets - possibly a slider can be done with vello, (bevy_vello) or bevy_vector_shapes.
# physics
in general, we want physics to be part of the experience.  Possibly nodes are floating in space, but the cables that connect them should feel the tug of gravity
## research
- avian3d physics - built for bevy - worth checking out rather than using rapier3d as you did in nateroids
# splash
https://crates.io/crates/bevy_verlet - the cloth screen getting cut and disintegrating would be a good reveal for the stage and the node editor, n'est-ce pas?
# ambiance
## atmosphere
https://crates.io/crates/bevy_atmosphere - procedural atmosphere generator - maybe this could be incorporated for the POC as long as it's not too heavy
## fluid dynamics
https://crates.io/crates/bevy_eulerian_fluid - maybe this is animating the floor?
## research
- bevy_mod_outline - outlines meshes - might be interesting for POC of nodes
- bevy_skybox - what it says on the tin
- bevy_rich_text3d - Mesh based raster rich text implementation for bevy.
- bevy_skein - a blender plugin that works with bevy to allow specifying components on meshes in blender that get exported as .gltf and will automatically instantiate those components in bevy
