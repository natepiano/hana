//! Screen-space overlay support for diegetic panels.

use bevy::camera::Camera3d;
use bevy::camera::Camera3dDepthTextureUsage;
use bevy::camera::ClearColorConfig;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;

use crate::layout::Sizing;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::PanelMode;
use crate::panel::PanelSystems;
use crate::panel::ScreenPosition;

/// Marker on overlay cameras spawned by the screen-space system.
#[derive(Component)]
pub(crate) struct ScreenSpaceCamera {
    pub render_layers: RenderLayers,
    pub camera_order:  isize,
}

/// Marker on directional lights spawned alongside overlay cameras.
#[derive(Component)]
pub(crate) struct ScreenSpaceLight {
    pub render_layers: RenderLayers,
}

pub(crate) struct ScreenSpacePlugin;

impl Plugin for ScreenSpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                position_screen_space_panels.before(PanelSystems::ComputeLayout),
                setup_screen_space_cameras.after(PanelSystems::ComputeLayout),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                propagate_screen_space_render_layers,
                cleanup_screen_space_cameras,
            ),
        );
    }
}

/// Positions and sizes screen-space panels relative to the window.
///
/// Runs before `compute_panel_layouts` so that any dimension changes
/// trigger layout recomputation via Bevy change detection.
fn position_screen_space_panels(
    windows: Query<&Window>,
    mut panels: Query<(&mut Transform, &mut DiegeticPanel, &ComputedDiegeticPanel)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let win_w = window.width();
    let win_h = window.height();
    if win_w <= 0.0 || win_h <= 0.0 {
        return;
    }
    let half_w = win_w / 2.0;
    let half_h = win_h / 2.0;

    for (mut transform, mut panel, computed) in &mut panels {
        let PanelMode::Screen {
            position,
            width,
            height,
            ..
        } = panel.mode()
        else {
            continue;
        };
        let position = *position;
        let width = *width;
        let height = *height;
        let (content_w, content_h) = (computed.content_width(), computed.content_height());

        let new_w = resolve_screen_axis(width, win_w, content_w, panel.width());
        if (panel.width() - new_w).abs() > 0.01 {
            panel.set_width(new_w);
        }
        let new_h = resolve_screen_axis(height, win_h, content_h, panel.height());
        if (panel.height() - new_h).abs() > 0.01 {
            panel.set_height(new_h);
        }

        let (screen_x, screen_y) = match position {
            ScreenPosition::Screen => {
                let (fx, fy) = panel.anchor().offset_fraction();
                (fx * win_w, fy * win_h)
            },
            ScreenPosition::At(pos) => (pos.x, pos.y),
        };

        transform.translation.x = screen_x - half_w;
        transform.translation.y = half_h - screen_y;
    }
}

/// Resolves one axis of a screen-space panel's [`Sizing`] to a pixel value.
///
/// - `Fixed`    → the dimension's value in pixels.
/// - `Percent`  → `window_axis * frac`.
/// - `Fit`      → the last-computed content size, clamped to `[min, max]`, with
///   `max.unwrap_or(window_axis)` as the growth budget on frame 1.
/// - `Grow`     → the window axis clamped to `[min, max]`.
fn resolve_screen_axis(sizing: Sizing, window_axis: f32, content: f32, current: f32) -> f32 {
    match sizing {
        Sizing::Fixed(dim) => dim.value,
        Sizing::Percent(frac) => window_axis * frac,
        Sizing::Fit { min, max } => {
            let upper = max.value.min(window_axis);
            let lower = min.value;
            // Use last-computed content size if the layout engine has produced
            // one; otherwise allow room to grow on the first frame.
            let target = if content > 0.0 {
                content
            } else {
                current.max(upper)
            };
            target.clamp(lower, upper)
        },
        Sizing::Grow { min, max } => window_axis.clamp(min.value, max.value),
    }
}

/// Spawns overlay cameras and lights for newly added screen-space panels.
///
/// For each unique `(camera_order, render_layers)` pair, a single shared
/// orthographic camera is created with `ScalingMode::WindowSize`
/// (1 world unit = 1 logical pixel). A directional light on the same
/// render layers provides stable illumination for PBR text materials.
fn setup_screen_space_cameras(
    added_panels: Query<(Entity, &DiegeticPanel), Added<DiegeticPanel>>,
    existing_cameras: Query<&ScreenSpaceCamera>,
    mut commands: Commands,
) {
    let mut spawned_this_frame: Vec<(isize, RenderLayers)> = Vec::new();

    for (panel_entity, panel) in &added_panels {
        let PanelMode::Screen {
            camera_order,
            ref render_layers,
            ..
        } = *panel.mode()
        else {
            continue;
        };

        let layers = render_layers;
        let order = camera_order;

        commands.entity(panel_entity).insert(layers.clone());

        let camera_exists = existing_cameras
            .iter()
            .any(|cam| cam.render_layers == *layers && cam.camera_order == order)
            || spawned_this_frame
                .iter()
                .any(|(o, l)| *o == order && *l == *layers);

        if camera_exists {
            continue;
        }

        spawned_this_frame.push((order, layers.clone()));

        commands.spawn((
            ScreenSpaceCamera {
                render_layers: layers.clone(),
                camera_order:  order,
            },
            Camera3d {
                depth_texture_usages: Camera3dDepthTextureUsage(
                    (TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING).bits(),
                ),
                ..default()
            },
            Camera {
                order,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::WindowSize,
                far: 2000.0,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 0.0, 1000.0).looking_at(Vec3::ZERO, Vec3::Y),
            bevy::render::view::Msaa::default(),
            layers.clone(),
        ));

        commands.spawn((
            ScreenSpaceLight {
                render_layers: layers.clone(),
            },
            DirectionalLight {
                illuminance: 5000.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(
                EulerRot::XYZ,
                -std::f32::consts::FRAC_PI_4,
                std::f32::consts::FRAC_PI_4,
                0.0,
            )),
            layers.clone(),
        ));
    }
}

/// Propagates [`RenderLayers`] from screen-space panel parents to their
/// children that are missing the component.
///
/// Runs after text/image/gizmo reconciliation so that newly spawned
/// children are picked up.
fn propagate_screen_space_render_layers(
    panels_with_layers: Query<(Entity, &RenderLayers, &DiegeticPanel)>,
    children_query: Query<&Children>,
    existing_layers: Query<&RenderLayers>,
    mut commands: Commands,
) {
    for (panel_entity, panel_layers, panel) in &panels_with_layers {
        if !panel.mode().is_screen() {
            continue;
        }
        let Ok(children) = children_query.get(panel_entity) else {
            continue;
        };
        propagate_layers_recursive(
            &children_query,
            &existing_layers,
            &mut commands,
            children,
            panel_layers,
        );
    }
}

fn propagate_layers_recursive(
    children_query: &Query<&Children>,
    existing_layers: &Query<&RenderLayers>,
    commands: &mut Commands,
    children: &Children,
    layers: &RenderLayers,
) {
    for child in children.iter() {
        if existing_layers.get(child).is_err() {
            commands.entity(child).insert(layers.clone());
        }
        if let Ok(grandchildren) = children_query.get(child) {
            propagate_layers_recursive(
                children_query,
                existing_layers,
                commands,
                grandchildren,
                layers,
            );
        }
    }
}

/// Despawns overlay cameras and lights when no screen-space panels
/// reference their `(camera_order, render_layers)` pair.
fn cleanup_screen_space_cameras(
    panels: Query<&DiegeticPanel>,
    cameras: Query<(Entity, &ScreenSpaceCamera)>,
    lights: Query<(Entity, &ScreenSpaceLight)>,
    mut commands: Commands,
    mut prev_pairs: Local<Vec<(isize, RenderLayers)>>,
) {
    let current_pairs: Vec<(isize, RenderLayers)> = panels
        .iter()
        .filter_map(|panel| {
            if let PanelMode::Screen {
                camera_order,
                ref render_layers,
                ..
            } = *panel.mode()
            {
                Some((camera_order, render_layers.clone()))
            } else {
                None
            }
        })
        .collect();

    if current_pairs.len() == prev_pairs.len()
        && current_pairs
            .iter()
            .zip(prev_pairs.iter())
            .all(|(a, b)| a.0 == b.0 && a.1 == b.1)
    {
        *prev_pairs = current_pairs;
        return;
    }

    for (entity, cam) in &cameras {
        let still_active = current_pairs
            .iter()
            .any(|(order, layers)| *order == cam.camera_order && *layers == cam.render_layers);
        if !still_active {
            commands.entity(entity).despawn();
        }
    }

    for (entity, light) in &lights {
        let still_active = current_pairs
            .iter()
            .any(|(_, layers)| *layers == light.render_layers);
        if !still_active {
            commands.entity(entity).despawn();
        }
    }

    *prev_pairs = current_pairs;
}

#[cfg(test)]
mod tests {
    use super::resolve_screen_axis;
    use crate::layout::Dimension;
    use crate::layout::Sizing;

    fn px(value: f32) -> Dimension { Dimension { value, unit: None } }

    #[track_caller]
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}",
        );
    }

    /// Fixed pixel value is returned unchanged regardless of window, content,
    /// or previous panel size.
    #[test]
    fn fixed_returns_exact_value() {
        let size = Sizing::Fixed(px(280.0));
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 280.0);
        assert_close(resolve_screen_axis(size, 800.0, 500.0, 100.0), 280.0);
        assert_close(resolve_screen_axis(size, 2000.0, 0.0, 0.0), 280.0);
    }

    /// Percent multiplies the window axis by the fraction.
    #[test]
    fn percent_scales_with_window() {
        let size = Sizing::Percent(0.25);
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 200.0);
        assert_close(resolve_screen_axis(size, 1600.0, 0.0, 0.0), 400.0);
    }

    /// `Fit` on the first frame (content unknown) grows up to the `max` cap,
    /// clamped by the window.
    #[test]
    fn fit_first_frame_uses_max_budget() {
        // Unbounded Fit: grows up to the window axis.
        let size = Sizing::FIT;
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 800.0);

        // Fit with an explicit max smaller than the window: grows to the max.
        let size = Sizing::Fit {
            min: px(0.0),
            max: px(400.0),
        };
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 400.0);

        // Fit with an explicit max larger than the window: clamped to window.
        let size = Sizing::Fit {
            min: px(0.0),
            max: px(2000.0),
        };
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 800.0);
    }

    /// Once the layout engine reports a content size, `Fit` shrinks the panel
    /// to that size, clamped to `[min, max]`.
    #[test]
    fn fit_shrinks_to_content_when_known() {
        let size = Sizing::FIT;
        // Content well under window → panel equals content.
        assert_close(resolve_screen_axis(size, 800.0, 320.0, 800.0), 320.0);
        // Content larger than window → clamped to window.
        assert_close(resolve_screen_axis(size, 800.0, 1200.0, 800.0), 800.0);
    }

    /// `Fit { min, max }` clamps content to the configured bounds.
    #[test]
    fn fit_clamps_content_to_min_max() {
        let size = Sizing::Fit {
            min: px(100.0),
            max: px(500.0),
        };
        // Content below min → floor at min.
        assert_close(resolve_screen_axis(size, 800.0, 50.0, 0.0), 100.0);
        // Content within range → unchanged.
        assert_close(resolve_screen_axis(size, 800.0, 300.0, 0.0), 300.0);
        // Content above max → cap at max.
        assert_close(resolve_screen_axis(size, 800.0, 600.0, 0.0), 500.0);
    }

    /// `Grow` fills the window axis, clamped to `[min, max]`.
    #[test]
    fn grow_fills_window_clamped() {
        // Unbounded Grow: equals window.
        let size = Sizing::GROW;
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 800.0);

        // Grow with explicit cap below window: capped.
        let size = Sizing::Grow {
            min: px(100.0),
            max: px(500.0),
        };
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 500.0);

        // Grow with min above window: floored at min.
        let size = Sizing::Grow {
            min: px(1000.0),
            max: px(f32::INFINITY),
        };
        assert_close(resolve_screen_axis(size, 800.0, 0.0, 0.0), 1000.0);
    }
}
