use bevy::prelude::*;
use bevy_liminal::OutlineMethod;

use crate::benchmark_state::OutlinePresence;
use crate::grid::GridSpawnSpec;
use crate::grid::spawn_grid;
use crate::viewport::ViewportInfo;

#[derive(Clone, Copy)]
pub(super) struct ScenarioDefinition {
    pub(super) name:          &'static str,
    pub(super) key:           KeyCode,
    pub(super) scenario_kind: ScenarioKind,
}

#[derive(Clone, Copy)]
pub(super) enum ScenarioKind {
    Grid {
        count:     u32,
        width:     f32,
        cube_fill: f32,
    },
}

pub(super) fn spawn_scenario(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    scenario: &ScenarioDefinition,
    viewport: &ViewportInfo,
    outline_presence: OutlinePresence,
    outline_method: OutlineMethod,
) {
    let ScenarioKind::Grid {
        count,
        width,
        cube_fill,
    } = scenario.scenario_kind;
    spawn_grid(
        commands,
        meshes,
        materials,
        GridSpawnSpec {
            count,
            width,
            cube_fill,
            viewport,
            outline_presence,
            outline_method,
        },
    );
}
