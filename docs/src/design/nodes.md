# Nodes
For the time being, this doc is for design notes and research for nodes. It won't have a structure until one emerges.

# node data types
## nodes are going to need to be able to process
- meshes
- images
- video
- cv
- audio
- device inputs
- osc
- midi
## interop
One of the great things about modular synthesizers is the unification of audio and control voltages - which are just (typically) slower frequencies within the same voltage range. Allowing users to plug audio into a CV input and CV into an audio input and things just work. They can be weird but they work.

In EVERY way hana should allow anything to plug into anything to plug into anything. Implement automatic default conversion. Which will need to be a trait based system for managing the appropriate `From` conversions, et al. What would it mean to plug audio into video? CV into midi? Are there appropriate defaults? Should we disallow certain types - in which case input/output port pairs would visually be disabled?

# node shaders/compute
This is almost certainly going to benefit from GPU compute utilization and custom shaders to be able to process data efficiently in the node graph.
## research
Bevy millions balls uses gpu compute -find a link to this. It uses GPU compute to deal with collisions on a LOT of balls. If hana is going to work, you're going to need to do this sort of GPU compute thing and get good at it.

# nodes as wasm plugins
Nodes ideally will be implemented as wasm plugins. Ideally you could extend the bevy ECS to it and maybe this is worth pursuing.
## research
Settletopia has wasmtime based plugins
