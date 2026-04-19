//! Events for camera animations and zoom operations.
//!
//! Events are organized by feature. Each group starts with the **trigger** event
//! (fire with `commands.trigger(...)`) followed by the **fired** events it produces
//! (observe with `.add_observer(...)`).
//!
//! # Common patterns
//!
//! **Duration**: several events accept a `duration` field. When set to
//! `Duration::ZERO` the operation completes instantly. When `duration > Duration::ZERO`
//! the operation animates through [`PlayAnimation`].
//!
//! **Easing**: events that animate also accept an
//! [`EaseFunction`](bevy::math::curve::easing::EaseFunction) that controls the
//! interpolation curve.
//!
//! # Event ordering
//!
//! Events nest from outermost operation-level events to the inner animation and move
//! lifecycle. Every animated path goes through [`PlayAnimation`], so
//! [`AnimationBegin`]/[`AnimationEnd`] and [`CameraMoveBegin`]/[`CameraMoveEnd`] fire
//! for all animated operations.
//!
//! `ZoomToFit` wraps the animation lifecycle with [`ZoomBegin`] and [`ZoomEnd`].
//! `AnimateToFit` uses [`AnimationSource::AnimateToFit`] to distinguish itself from a
//! plain [`PlayAnimation`]. Instant operations bypass the move lifecycle.
//!
//! # Emitted event data
//!
//! | Event                    | `camera` | `target` | `margin` | `duration` | `easing` | `source` | `camera_move` |
//! |--------------------------|----------|----------|----------|------------|----------|----------|---------------|
//! | [`ZoomBegin`]            | yes      | yes      | yes      | yes        | yes      | no       | no            |
//! | [`ZoomEnd`]              | yes      | yes      | yes      | yes        | yes      | no       | no            |
//! | [`ZoomCancelled`]        | yes      | yes      | yes      | yes        | yes      | no       | no            |
//! | [`AnimationBegin`]       | yes      | no       | no       | no         | no       | yes      | no            |
//! | [`AnimationEnd`]         | yes      | no       | no       | no         | no       | yes      | no            |
//! | [`AnimationCancelled`]   | yes      | no       | no       | no         | no       | yes      | yes           |
//! | [`AnimationRejected`]    | yes      | no       | no       | no         | no       | yes      | no            |
//! | [`CameraMoveBegin`]      | yes      | no       | no       | no         | no       | no       | yes           |
//! | [`CameraMoveEnd`]        | yes      | no       | no       | no         | no       | no       | yes           |

mod animation;
mod fit;
mod look;
mod zoom;

pub use animation::AnimationBegin;
pub use animation::AnimationCancelled;
pub use animation::AnimationEnd;
pub use animation::AnimationRejected;
pub use animation::AnimationSource;
pub use animation::CameraMoveBegin;
pub use animation::CameraMoveEnd;
pub use animation::PlayAnimation;
pub use fit::AnimateToFit;
pub use fit::SetFitTarget;
pub use look::LookAt;
pub use look::LookAtAndZoomToFit;
pub use zoom::ZoomBegin;
pub use zoom::ZoomCancelled;
pub use zoom::ZoomContext;
pub use zoom::ZoomEnd;
pub use zoom::ZoomToFit;
