# Player
I suspect there will be a play only mode that allows you to control the things that you find most important for a live performance.

Anything that you've parameterized.

For inspiration think about Transit2 - Andrew Huang VST effect with Baby Audio. The big knob - super important.

And possibly the node editor would allow you to tag parameters to automatically show up in the player.  So you only interact with the things that are important to you.

And you can jump quickly back and forth between player and node editor so it is low friction to make a parameter available for the player

Have to think hard about this - in VCV rack you see everything all the time. Which can be both an advantage and also overwhelming. Omri Cohen et al., have come up with mechanisms to create meta modules that allow for playing so you don't have to drown in the complexity of all the controls.  but they do take work to configure - so if we automate this and make it "the player" that could be a key feature.

The likely answer is that we just compile a play only version if we measure that it can be more performant than running it with the full hana environment.

**important** for this reason it's important to keep hana modular and separable so that we can create #cfg 'chokepoints' that allow us to easily conditionally compile.

**research**
- measure speed of running node graph on its own - in FPS
- see if you can devise mechanisms to "compile" nodes -  in a way that wouldn't make sense to just be part of the main app anyway. It would only be useful if the compile was a heavy compute lift in terms of clock time. Although it seems this could be an always-on background task...
