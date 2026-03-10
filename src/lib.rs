//! `bevy_diegetic` — Diegetic UI for Bevy.
//!
//! Provides an in-world UI layout engine inspired by [Clay](https://github.com/nicbarker/clay),
//! implemented in pure Rust with no global state and full thread safety.
//!
//! # Modules
//!
//! - [`layout`] — The core layout engine (types, tree builder, computation, render commands).

pub mod layout;
