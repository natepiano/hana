# Node Editor
## General
- Nodes, cables, controls should be meshes that look real - with lighting and shadows and physics as appropriate. Gravity would be off for nodes but on for cables maybe - so the physics engine would be what makes them hang - they'd need to be articulated cables for that to work, I believe.

## Node
- by default nodes are cubes with controls, visualizations, connectors on different faces
  - however a node could be standard solids - it could just be a flat panel but it could be a sphere, a pyramid, platonic solids
- when engaging with a node, it should be obviously selected somehow, you can rotate faces to interact with the different controls it makes available.
- Nodes appear with an animation - is there a distinct animation for each type? Is there an accompanying sound for each type?
- There could be a visualization of what the node itself does
- Could there be another visualization of what is done by the node up to that point? possibly. For example, if an LFO is a source, then you'd only see it's output waveform. But if there is an input to the LFO that's modifying it, you could see the combined effect to that point
- if appropriate, a node should automatically support multiple inputs and multiple outputs - self adapting without requiring extra configuration
- nodes could have different looks - themes that could be add-ons
- any control on a node can be modulated / parameterized by outputs from other nodes - which could be midi, or sensor values, or whatever has been transformed.

## interaction
there will have to be some kind of focus reticule, mouse cursor, something that indicates which control is being interacted with. I notice that changing the volume in the upper right hand control panel of AppleTV+ as you move the cursor over the volume, it grows to become easier to manipulate. This is a nice touch. Affordance that makes the control more obvious and also easier to interact with.

## groups
- You can group sections - and name them for convenience - a section container shouldn't get in the way of editing

## minigame
- Nodes could expose an entire mini-game environment where you dive in and configure. Maybe you dive ino the cube and it becomes bigger on the inside. as an example, an LFO might show a very much larger version of the wave form, hanging in space in front of you.  Waveform editing tools appear and you can freeze it and add control nodes to reshape the envelope - stair step, random, bezier, etc. - could all be available. You could stretch the waveform to fit a timescale - or like ableton use the control points to stretch to a particular part of the timescale.
- The basic idea of the minigame could be implemented in different ways by different nodes - allowing for a wide range of interactive experiences within the context of the node editor.

## cables
- cables connecting would look like real audio cables
- when close enough to a cable, you can see a visualization representing the data that it carries. For example, if it's an audio, or control voltages, an oscilloscope view of the wave form can display. Midi data - maybe that shows the notes being played. If it's video, not sure...
- and if you're looking at an oscilloscope view of a cable, you can expand it for control over the oscilloscope data. I like the idea that every single cable is automatically an oscilloscope without having to add one where you want to see one. Of course you can turn this (or any visualization) off.
- when zoomed out, possibly the cables have light running through them so it looks cool - different visualizations could be applied
- cables automatically route
- cables can be tensioned differently
- moving connected nodes can either cause the cables to extend - or you could lock the section of nodes (or all nodes) and pulling on a cable pulls everything it's attached to - physics engine can manage this
- cables can have transparency (including invisible)
- connecting cables - it should be clearly impossible (faded out?) to connect a cable to something that isn't valid.

## Navigation
- Essentially it's a voxel world - you can zoom in and out, rotate and move to any position around it - very fast and responsive.
- You can move nodes, groups of nodes, or all nodes
- there should be range selection

## global information
- information like FPS - maybe on a glowing sphere (or data separated by glowing lines) - like a node, it could be a shape changer and people could customize what is visible, with intelligent defaults. Kind of like information on a car dashboard where you move the thumb wheel on the steering wheel to change the view to see global stats about what's happening.
- so yeah, probably the global information display shares something with nodes in terms of how they're constructed. And i suppose the output of the global information could be a feedback loop back into nodes...
- along with FPS, for troubleshooting purposes we should also be able to see things such as
  - CPU use across cores
  - GPU use
  - Memory usage?
  - Network usage?
- and it could be interesting to zoom in and chart these results but that's a big nice to have

## Node Target -> Output -> Visualization
- Ultimately the nodes are working to create and/or modify a visualization so you should be able to see that visualization while editing.
- Possibly it's in the background - on a billboard in the distance as an example.
- or there's a natural direction to the cables ultimately with an output to a "screen" - so you can rotate towards seeing the screen at any time. Or it could be in a screen-split window.
- if there are output nodes, which likely there would be - they can double as the screen and be viewed in different ways - as a node, as a screen, as a 3d model you can walk around, etc. Lights. Lasers. Etc.

## Recursion
- the node editor itself is a kind of visualization - so could you make the node editor a node and modify it somehow? what parameters would it expose? You might expose the things you use to control the node editor such as visibility of cables, tensioning of cables, distance between nodes, rotation, themes, all of these things - so if you "play" a node graph, it would itself look pretty cool

## AR
- The node editor could be hanging in space in front of you and could be manipulated by your hands. Spin dials, move sliders, type on virtual keyboards. Grab nodes, sections or the whole thing - pan, zoom, rotate, etc.
- Inventory of nodes, assets, and materials could have an AR view so you can see many things simultaneously and quickly find what you're looking for.

# evaluator
there is a capability of the node editor to give you feedback on any nodes that are not actually contributing to the final output - maybe some kind of indicator of how much they're contributing. I.e., it would be nice if you could optionally trim away nodes that aren't having a real effect - without needing to do a bunch of trial and error to find out.
