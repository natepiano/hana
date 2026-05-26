//! Screen-space overlay support for diegetic panels.

mod constants;

use bevy::camera::Camera3d;
use bevy::camera::Camera3dDepthTextureUsage;
use bevy::camera::ClearColorConfig;
use bevy::camera::RenderTarget;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;
use constants::SCREEN_SPACE_CAMERA_FAR;
use constants::SCREEN_SPACE_CAMERA_Z;
use constants::SCREEN_SPACE_LIGHT_ILLUMINANCE;
use constants::SCREEN_SPACE_PANEL_RESIZE_EPSILON;

use crate::layout::Sizing;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::CoordinateSpace;
use crate::panel::DiegeticPanel;
use crate::panel::PanelSystems;
use crate::panel::ScreenPosition;

/// Marker on overlay cameras spawned by the screen-space system. Carries the
/// `(camera_order, render_layers, window)` triple so observers can match
/// panels against existing cameras without a side registry.
#[derive(Component)]
pub(crate) struct ScreenSpaceCamera {
    pub render_layers: RenderLayers,
    pub order:         isize,
    pub window:        Entity,
}

/// Marker on directional lights spawned alongside overlay cameras.
#[derive(Component)]
pub(crate) struct ScreenSpaceLight {
    pub render_layers: RenderLayers,
}

pub(crate) struct ScreenSpacePlugin;

impl Plugin for ScreenSpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(setup_screen_space_view)
            .add_observer(cleanup_screen_space_view)
            .add_observer(cleanup_screen_space_on_window_close)
            .add_systems(
                Update,
                position_screen_space_panels.after(PanelSystems::ResolveWorldFit),
            )
            .add_systems(PostUpdate, propagate_screen_space_render_layers);
    }
}

/// Resolves a [`WindowRef`] to a concrete window [`Entity`].
///
/// `WindowRef::Primary` requires a [`PrimaryWindow`] to exist; missing it
/// is a misconfiguration (e.g. headless tests without `WindowPlugin`) and
/// is reported once via `warn_once!` so positioning failures are visible
/// instead of silent.
fn resolve_window_ref(
    window_ref: WindowRef,
    primary: &Query<Entity, With<PrimaryWindow>>,
) -> Option<Entity> {
    match window_ref {
        WindowRef::Primary => {
            let resolved = primary.single().ok();
            if resolved.is_none() {
                bevy::log::warn_once!(
                    "bevy_diegetic: screen panel asked for WindowRef::Primary but no \
                     PrimaryWindow exists; panel will be ignored"
                );
            }
            resolved
        },
        WindowRef::Entity(entity) => Some(entity),
    }
}

/// Positions and sizes screen-space panels relative to their target window.
///
/// Runs after the panel layout sequence so `Fit` panels are placed from
/// their measured dimensions instead of the temporary build-time size.
fn position_screen_space_panels(
    windows: Query<(Entity, &Window)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut panels: Query<(&mut Transform, &mut DiegeticPanel, &ComputedDiegeticPanel)>,
) {
    let mut by_entity: HashMap<Entity, (f32, f32)> = HashMap::default();
    for (entity, window) in &windows {
        let w = window.width();
        let h = window.height();
        if w > 0.0 && h > 0.0 {
            by_entity.insert(entity, (w, h));
        }
    }
    if by_entity.is_empty() {
        return;
    }

    for (mut transform, mut panel, computed) in &mut panels {
        let CoordinateSpace::Screen {
            position,
            width,
            height,
            window: window_ref,
            ..
        } = panel.coordinate_space()
        else {
            continue;
        };
        let position = *position;
        let width = *width;
        let height = *height;
        let window_ref = *window_ref;

        let Some(window_entity) = resolve_window_ref(window_ref, &primary) else {
            continue;
        };
        let Some(&(window_width, window_height)) = by_entity.get(&window_entity) else {
            continue;
        };
        let half_width = window_width / 2.0;
        let half_height = window_height / 2.0;
        let (content_width, content_height) = (computed.content_width(), computed.content_height());

        let new_width = resolve_screen_axis(width, window_width, content_width, panel.width());
        if (panel.width() - new_width).abs() > SCREEN_SPACE_PANEL_RESIZE_EPSILON {
            panel.set_width(new_width);
        }
        let new_height = resolve_screen_axis(height, window_height, content_height, panel.height());
        if (panel.height() - new_height).abs() > SCREEN_SPACE_PANEL_RESIZE_EPSILON {
            panel.set_height(new_height);
        }

        let (screen_x, screen_y) = match position {
            ScreenPosition::Screen => {
                let (fx, fy) = panel.anchor().offset_fraction();
                (fx * window_width, fy * window_height)
            },
            ScreenPosition::At(pos) => (pos.x, pos.y),
        };

        transform.translation.x = screen_x - half_width;
        transform.translation.y = half_height - screen_y;
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
/// For each unique `(camera_order, render_layers, window)` triple, a single
/// shared orthographic camera is created with `ScalingMode::WindowSize`
/// (1 world unit = 1 logical pixel) and pinned to its target window via
/// `Camera.target`. A directional light on the same render layers provides
/// stable illumination for PBR text materials; the light is keyed by
/// `render_layers` only because directional-light contributions accumulate
/// across cameras sharing a layer.
///
/// Sharing is detected by querying existing cameras for a matching triple —
/// no central registry is maintained.
fn setup_screen_space_view(
    trigger: On<Add, DiegeticPanel>,
    panels: Query<&DiegeticPanel>,
    cameras: Query<&ScreenSpaceCamera>,
    lights: Query<&ScreenSpaceLight>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok(panel) = panels.get(trigger.entity) else {
        return;
    };
    let CoordinateSpace::Screen {
        camera_order,
        ref render_layers,
        window: window_ref,
        ..
    } = *panel.coordinate_space()
    else {
        return;
    };
    let Some(window_entity) = resolve_window_ref(window_ref, &primary) else {
        return;
    };

    commands
        .entity(trigger.entity)
        .insert(render_layers.clone());

    let camera_exists = cameras.iter().any(|cam| {
        cam.order == camera_order
            && cam.render_layers == *render_layers
            && cam.window == window_entity
    });
    if !camera_exists {
        commands.spawn((
            ScreenSpaceCamera {
                render_layers: render_layers.clone(),
                order:         camera_order,
                window:        window_entity,
            },
            Camera3d {
                depth_texture_usages: Camera3dDepthTextureUsage(
                    (TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING).bits(),
                ),
                ..default()
            },
            Camera {
                order: camera_order,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderTarget::Window(WindowRef::Entity(window_entity)),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::WindowSize,
                far: SCREEN_SPACE_CAMERA_FAR,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 0.0, SCREEN_SPACE_CAMERA_Z).looking_at(Vec3::ZERO, Vec3::Y),
            bevy::render::view::Msaa::default(),
            render_layers.clone(),
        ));
    }

    let light_exists = lights
        .iter()
        .any(|light| light.render_layers == *render_layers);
    if light_exists {
        return;
    }

    commands.spawn((
        ScreenSpaceLight {
            render_layers: render_layers.clone(),
        },
        DirectionalLight {
            illuminance: SCREEN_SPACE_LIGHT_ILLUMINANCE,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -std::f32::consts::FRAC_PI_4,
            std::f32::consts::FRAC_PI_4,
            0.0,
        )),
        render_layers.clone(),
    ));
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
        if !panel.coordinate_space().is_screen() {
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

/// Despawns the overlay camera and light when the last panel using them is
/// removed.
///
/// Reads the removed panel's `(camera_order, render_layers, window)` while
/// the component is still live (`On<Remove>` fires before the component is
/// dropped). Cameras are keyed by the full triple — despawned only when no
/// surviving panel matches all three. Lights are keyed by `render_layers`
/// alone (singleton per layer, app-wide) — despawned only when no panel on
/// that layer survives in *any* window.
///
/// This observer is the sole owner of camera and light despawn. The
/// `cleanup_screen_space_on_window_close` observer despawns panels only;
/// teardown of their cameras/lights cascades through this observer,
/// keeping single-owner cleanup.
fn cleanup_screen_space_view(
    trigger: On<Remove, DiegeticPanel>,
    panels: Query<(Entity, &DiegeticPanel)>,
    cameras: Query<(Entity, &ScreenSpaceCamera)>,
    lights: Query<(Entity, &ScreenSpaceLight)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok((_, removed_panel)) = panels.get(trigger.entity) else {
        return;
    };
    let CoordinateSpace::Screen {
        camera_order,
        ref render_layers,
        ..
    } = *removed_panel.coordinate_space()
    else {
        return;
    };

    // For each matching camera, check whether any *other* surviving panel
    // still resolves to that camera's window. If not, the camera is dead.
    // Iterating cameras (not windows) is what lets us clean up orphans
    // whose `WindowRef::Primary` panels can no longer be resolved because
    // the primary window itself was despawned.
    for (cam_entity, cam) in &cameras {
        if cam.order != camera_order || cam.render_layers != *render_layers {
            continue;
        }
        let still_used = panels.iter().any(|(entity, panel)| {
            if entity == trigger.entity {
                return false;
            }
            let CoordinateSpace::Screen {
                camera_order: other_order,
                render_layers: other_layers,
                window: other_window_ref,
                ..
            } = panel.coordinate_space()
            else {
                return false;
            };
            *other_order == camera_order
                && other_layers == render_layers
                && resolve_window_ref(*other_window_ref, &primary) == Some(cam.window)
        });
        if !still_used {
            commands.entity(cam_entity).despawn();
        }
    }

    let light_still_in_use = panels.iter().any(|(entity, panel)| {
        if entity == trigger.entity {
            return false;
        }
        match panel.coordinate_space() {
            CoordinateSpace::Screen {
                render_layers: other_layers,
                ..
            } => other_layers == render_layers,
            CoordinateSpace::World { .. } => false,
        }
    });
    if light_still_in_use {
        return;
    }
    for (entity, light) in &lights {
        if light.render_layers == *render_layers {
            commands.entity(entity).despawn();
        }
    }
}

/// Despawns screen-space panels whose target window was removed.
///
/// Camera and light teardown cascade through [`cleanup_screen_space_view`]
/// when those panels are despawned — this observer owns panel teardown
/// only, preserving single-owner cleanup.
fn cleanup_screen_space_on_window_close(
    trigger: On<Remove, Window>,
    panels: Query<(Entity, &DiegeticPanel)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let removed = trigger.entity;
    for (entity, panel) in &panels {
        let CoordinateSpace::Screen {
            window: window_ref, ..
        } = panel.coordinate_space()
        else {
            continue;
        };
        if resolve_window_ref(*window_ref, &primary) == Some(removed) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::camera::RenderTarget;
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;
    use bevy::window::WindowRef;

    use super::ScreenSpaceCamera;
    use super::ScreenSpaceLight;
    use super::ScreenSpacePlugin;
    use super::resolve_screen_axis;
    use crate::Anchor;
    use crate::Fit;
    use crate::layout::Dimension;
    use crate::layout::Sizing;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::DiegeticPanel;
    use crate::panel::PanelSystems;

    fn px(value: f32) -> Dimension { Dimension { value, unit: None } }

    #[track_caller]
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}",
        );
    }

    fn write_known_content_size(
        mut panels: Query<&mut ComputedDiegeticPanel, With<DiegeticPanel>>,
    ) {
        for mut panel in &mut panels {
            panel.set_content_size(240.0, 80.0);
        }
    }

    #[test]
    fn bottom_right_fit_panel_uses_layout_content_size_in_first_update() -> Result<(), &'static str>
    {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn((
            Window {
                resolution: (800_u32, 600_u32).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app.configure_sets(
            Update,
            PanelSystems::ResolveWorldFit.after(PanelSystems::ComputeLayout),
        );
        app.add_systems(
            Update,
            write_known_content_size.in_set(PanelSystems::ComputeLayout),
        );
        app.add_plugins(ScreenSpacePlugin);

        let Ok(panel) = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .layout(|_| {})
            .build()
        else {
            return Err("Fit screen panel should build");
        };
        let panel = app.world_mut().spawn(panel).id();

        app.update();

        let Some(panel_component) = app.world().get::<DiegeticPanel>(panel) else {
            return Err("panel should still exist");
        };
        assert_close(panel_component.width(), 240.0);
        assert_close(panel_component.height(), 80.0);
        let (anchor_x, anchor_y) = panel_component.anchor_offsets();
        assert_close(anchor_x, 240.0);
        assert_close(anchor_y, 80.0);

        let Some(transform) = app.world().get::<Transform>(panel) else {
            return Err("panel should have a transform");
        };
        assert_close(transform.translation.x, 400.0);
        assert_close(transform.translation.y, -300.0);

        Ok(())
    }

    /// Two windows of different sizes each host one bottom-right `Fit` panel
    /// pinned via `.window_entity(...)`. Each panel must position itself
    /// against its own window's dimensions, not the primary's.
    #[test]
    fn panels_resolve_against_their_own_window() -> Result<(), &'static str> {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let primary = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (800_u32, 600_u32).into(),
                    ..Default::default()
                },
                PrimaryWindow,
            ))
            .id();
        let secondary = app
            .world_mut()
            .spawn(Window {
                resolution: (1200_u32, 400_u32).into(),
                ..Default::default()
            })
            .id();
        app.configure_sets(
            Update,
            PanelSystems::ResolveWorldFit.after(PanelSystems::ComputeLayout),
        );
        app.add_systems(
            Update,
            write_known_content_size.in_set(PanelSystems::ComputeLayout),
        );
        app.add_plugins(ScreenSpacePlugin);

        let Ok(primary_panel) = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .window(WindowRef::Primary)
            .layout(|_| {})
            .build()
        else {
            return Err("primary panel should build");
        };
        let primary_panel = app.world_mut().spawn(primary_panel).id();

        let Ok(secondary_panel) = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .window_entity(secondary)
            .layout(|_| {})
            .build()
        else {
            return Err("secondary panel should build");
        };
        let secondary_panel = app.world_mut().spawn(secondary_panel).id();

        app.update();

        let Some(primary_transform) = app.world().get::<Transform>(primary_panel) else {
            return Err("primary panel should have a transform");
        };
        // 800 × 600 window → bottom-right anchor lands at (+400, -300).
        assert_close(primary_transform.translation.x, 400.0);
        assert_close(primary_transform.translation.y, -300.0);

        let Some(secondary_transform) = app.world().get::<Transform>(secondary_panel) else {
            return Err("secondary panel should have a transform");
        };
        // 1200 × 400 window → bottom-right anchor lands at (+600, -200).
        assert_close(secondary_transform.translation.x, 600.0);
        assert_close(secondary_transform.translation.y, -200.0);

        let _ = primary;
        Ok(())
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

    /// `Percent` multiplies the window axis by the fraction.
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

    /// Helper for the multi-window cleanup tests: builds an app with two
    /// windows and one bottom-right `Fit` panel pinned to each, then runs
    /// one update to let the setup observers spawn cameras and lights.
    fn build_two_window_app() -> (App, Entity, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let primary = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (800_u32, 600_u32).into(),
                    ..Default::default()
                },
                PrimaryWindow,
            ))
            .id();
        let secondary = app
            .world_mut()
            .spawn(Window {
                resolution: (1200_u32, 400_u32).into(),
                ..Default::default()
            })
            .id();
        app.configure_sets(
            Update,
            PanelSystems::ResolveWorldFit.after(PanelSystems::ComputeLayout),
        );
        app.add_systems(
            Update,
            write_known_content_size.in_set(PanelSystems::ComputeLayout),
        );
        app.add_plugins(ScreenSpacePlugin);

        let primary_panel = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .window(WindowRef::Primary)
            .layout(|_| {})
            .build()
            .expect("primary panel builds");
        let primary_panel = app.world_mut().spawn(primary_panel).id();

        let secondary_panel = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .window_entity(secondary)
            .layout(|_| {})
            .build()
            .expect("secondary panel builds");
        let secondary_panel = app.world_mut().spawn(secondary_panel).id();

        app.update();
        (app, primary, secondary, primary_panel, secondary_panel)
    }

    /// Two windows, two panels on the same render layer: each spawns its
    /// own overlay camera pointed at its own window; the directional light
    /// is a single shared instance for the layer.
    #[test]
    fn two_windows_spawn_one_camera_each_one_shared_light() {
        let (mut app, primary, secondary, _, _) = build_two_window_app();

        let mut cam_q = app
            .world_mut()
            .query::<(&ScreenSpaceCamera, &RenderTarget)>();
        let mut cameras: Vec<(Entity, RenderTarget)> = Vec::new();
        for (cam, target) in cam_q.iter(app.world()) {
            cameras.push((cam.window, target.clone()));
        }
        assert_eq!(cameras.len(), 2, "one camera per window");

        let mut targets_primary = false;
        let mut targets_secondary = false;
        for (window, target) in &cameras {
            assert!(*window == primary || *window == secondary);
            if let RenderTarget::Window(WindowRef::Entity(e)) = target {
                if *e == primary {
                    targets_primary = true;
                }
                if *e == secondary {
                    targets_secondary = true;
                }
            }
        }
        assert!(targets_primary, "camera targets primary window");
        assert!(targets_secondary, "camera targets secondary window");

        let mut light_q = app.world_mut().query::<&ScreenSpaceLight>();
        assert_eq!(
            light_q.iter(app.world()).count(),
            1,
            "one light shared across layer"
        );
    }

    /// Despawning one window despawns its panel and camera while leaving
    /// the shared light and the other window's panel/camera intact.
    #[test]
    fn despawning_one_window_keeps_other_alive() {
        let (mut app, primary, secondary, primary_panel, secondary_panel) = build_two_window_app();
        app.world_mut().entity_mut(secondary).despawn();
        app.update();

        assert!(
            app.world().get_entity(secondary_panel).is_err(),
            "panel for despawned window is gone"
        );
        assert!(
            app.world().get_entity(primary_panel).is_ok(),
            "primary panel survives"
        );

        let mut cam_q = app.world_mut().query::<&ScreenSpaceCamera>();
        let cameras: Vec<Entity> = cam_q.iter(app.world()).map(|c| c.window).collect();
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0], primary);

        let mut light_q = app.world_mut().query::<&ScreenSpaceLight>();
        assert_eq!(
            light_q.iter(app.world()).count(),
            1,
            "light survives while a layer panel remains"
        );
    }

    /// Despawning both windows tears down every camera, light, and panel.
    #[test]
    fn despawning_both_windows_clears_everything() {
        let (mut app, primary, secondary, primary_panel, secondary_panel) = build_two_window_app();
        app.world_mut().entity_mut(primary).despawn();
        app.world_mut().entity_mut(secondary).despawn();
        app.update();

        assert!(app.world().get_entity(primary_panel).is_err());
        assert!(app.world().get_entity(secondary_panel).is_err());

        let mut cam_q = app.world_mut().query::<&ScreenSpaceCamera>();
        assert_eq!(cam_q.iter(app.world()).count(), 0);

        let mut light_q = app.world_mut().query::<&ScreenSpaceLight>();
        assert_eq!(light_q.iter(app.world()).count(), 0);
    }
}
