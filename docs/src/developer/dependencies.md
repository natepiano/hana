# Dependencies

Taking a dependency on a crate can lock you in. The APIs they expose are by definition, opinionated. So we want to choose dependencies based on things that are not likely to change over time. That we can be very happy with because they just work.

Something like tokio may fall into this category as it is so well vetted in the industry but even this choice may backfire.

The concern is a major architectural change down the road because we locked in too early.

Even bevy is a concern - but it does offer a lot of utility from the ECS system and rendering pipeline. But it also means that if we go outside its lane, it may be more difficult than if we hadn't used it. Think of Tiny Glades choosing to use the ECS but not using the rendering from bevy.

As an example, there are a couple of networking crates for bevy but i don't understand them well enough and i don't know how well they will be maintained, so for now I'm just creating the networking based on underlying tokio async support.

However, if we insulate a dependency sufficiently so that it doesn't "leak" into the rest of the codebase and we're only working with our own wrapper interface, that could be means to make an early choice that has a restricted surface area and can be easily replaced if needed.
