//! `setup_sections` and the 9 section setup modules it registers: `catenary`,
//! `cap_styles`, `solver_comparison`, `entity_attachment`, `shared_hub`,
//! `orthogonal_routing`, `detach_demo`, `inside_view`, and `connector`.

mod cap_styles;
mod catenary;
mod constants;
mod entity_attachment;
mod inside_view;
mod orthogonal_routing;
mod scaffold;
mod shared_hub;
mod solver_comparison;

pub(crate) use orthogonal_routing::sync_movable_obstacles;
pub(crate) use scaffold::SharedCableMaterial;
pub(crate) use scaffold::setup_sections;
