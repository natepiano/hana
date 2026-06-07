//! `setup_camera`, `setup_scene`, and the 9 section setup modules registered by
//! `setup_sections`: `catenary`, `cap_styles`, `solver_comparison`,
//! `entity_attachment`, `shared_hub`, `astar`, `detach_demo`, `inside_view`, and
//! `connector`.

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
