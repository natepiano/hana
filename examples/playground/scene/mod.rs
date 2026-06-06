//! Camera, ground, directional light, and all 7 section scene setups.

mod astar;
mod cap_styles;
mod catenary;
mod constants;
mod entity_attachment;
mod inside_view;
mod scaffold;
mod shared_hub;
mod solver_comparison;

pub(crate) use scaffold::SceneEntities;
pub(crate) use scaffold::SharedCableMaterial;
pub(crate) use scaffold::setup_camera;
pub(crate) use scaffold::setup_scene;
pub(crate) use scaffold::setup_sections;
