//! Network endpoints for the Hana system
//!
//! This module contains implementations of network endpoints for different roles
//! in the Hana system. The endpoints provide type-safe message sending and receiving
//! based on their roles.
mod base_endpoint;
mod role_based_endpoint;

// Re-export the endpoints for public use
pub use role_based_endpoint::{HanaEndpoint, VisualizationEndpoint};
