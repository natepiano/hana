//! `setup_camera`, `setup_scene`, and the 9 section setup modules registered by
//! `setup_sections`: `catenary`, `cap_styles`, `solver_comparison`,
//! `entity_attachment`, `shared_hub`, `orthogonal_routing`, `detach_demo`,
//! `inside_view`, and `connector`.

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
pub(crate) use scaffold::SceneEntities;
pub(crate) use scaffold::SharedCableMaterial;
pub(crate) use scaffold::setup_camera;
pub(crate) use scaffold::setup_scene;
pub(crate) use scaffold::setup_sections;
