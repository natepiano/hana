use bevy::camera::visibility::RenderLayers;
use bevy::picking::Pickable;
use bevy::picking::backend::HitData;
use bevy::picking::backend::PointerHits;
use bevy::picking::backend::ray::RayMap;
use bevy::picking::mesh_picking::MeshPickingCamera;
use bevy::picking::mesh_picking::MeshPickingSettings;
use bevy::picking::mesh_picking::ray_cast::MeshRayCast;
use bevy::picking::mesh_picking::ray_cast::MeshRayCastSettings;
use bevy::prelude::*;

use super::PanelWidget;
use super::PanelWidgets;
use super::WidgetOf;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::PanelOwned;
use crate::render;
use crate::render::PanelInteractionMesh;

#[derive(Clone, Copy)]
enum PickableMarkers {
    Optional,
    Required,
}

pub(super) fn update_hits(
    backend_settings: Res<MeshPickingSettings>,
    ray_map: Res<RayMap>,
    picking_cameras: Query<(&Camera, Has<MeshPickingCamera>, Option<&RenderLayers>)>,
    interaction_meshes: Query<&PanelOwned, With<PanelInteractionMesh>>,
    panels: Query<(
        &DiegeticPanel,
        &ComputedDiegeticPanel,
        &GlobalTransform,
        Option<&PanelWidgets>,
    )>,
    widgets: Query<(&PanelWidget, &WidgetOf, Option<&Pickable>)>,
    pickables: Query<&Pickable>,
    layers: Query<&RenderLayers>,
    mut ray_cast: MeshRayCast,
    mut pointer_hits: MessageWriter<PointerHits>,
) {
    for (&ray_id, &ray) in ray_map.iter() {
        let Ok((camera, camera_can_pick, camera_layers)) = picking_cameras.get(ray_id.camera)
        else {
            continue;
        };
        if backend_settings.require_markers && !camera_can_pick {
            continue;
        }

        let camera_layers = camera_layers.cloned().unwrap_or_default();
        let filter = |mesh_entity| {
            let Ok(ownership) = interaction_meshes.get(mesh_entity) else {
                return false;
            };
            let panel_entity = ownership.owner();
            let marker_requirement =
                !backend_settings.require_markers || pickables.get(panel_entity).is_ok();
            let mesh_layers = layers.get(mesh_entity).cloned().unwrap_or_default();
            let render_layers_match = camera_layers.intersects(&mesh_layers);
            let panel_is_pickable = pickables
                .get(panel_entity)
                .ok()
                .is_none_or(|pickable| pickable.is_hoverable);
            marker_requirement && render_layers_match && panel_is_pickable
        };
        let early_exit = |mesh_entity| {
            interaction_meshes
                .get(mesh_entity)
                .ok()
                .and_then(|ownership| pickables.get(ownership.owner()).ok())
                .is_some_and(|pickable| pickable.should_block_lower)
        };
        let settings = MeshRayCastSettings {
            visibility:      backend_settings.ray_cast_visibility,
            filter:          &filter,
            early_exit_test: &early_exit,
        };
        let panel_hits = ray_cast.cast_ray(ray, &settings);
        let mut picks = Vec::new();

        for (mesh_entity, hit) in panel_hits {
            let Ok(ownership) = interaction_meshes.get(*mesh_entity) else {
                continue;
            };
            let panel_entity = ownership.owner();
            let Ok((panel, computed, panel_transform, panel_widgets)) = panels.get(panel_entity)
            else {
                continue;
            };
            let Some(panel_local) =
                render::project_flat_panel_hit(hit.point, panel, panel_transform)
            else {
                continue;
            };

            let matching_widgets = matching_widgets(
                panel_entity,
                panel_local,
                computed,
                panel_widgets,
                &widgets,
                if backend_settings.require_markers {
                    PickableMarkers::Required
                } else {
                    PickableMarkers::Optional
                },
            );

            let mut widget_depth = hit.distance;
            for (_, widget_entity) in matching_widgets {
                widget_depth = widget_depth.next_down();
                picks.push((
                    widget_entity,
                    HitData::new(
                        ray_id.camera,
                        widget_depth,
                        Some(hit.point),
                        Some(hit.normal),
                    ),
                ));
            }
            picks.push((
                panel_entity,
                HitData::new(
                    ray_id.camera,
                    hit.distance,
                    Some(hit.point),
                    Some(hit.normal),
                ),
            ));
        }

        if !picks.is_empty() {
            pointer_hits.write(PointerHits::new(
                ray_id.pointer,
                picks,
                camera_order(camera.order),
            ));
        }
    }
}

fn matching_widgets(
    panel_entity: Entity,
    panel_local: Vec2,
    computed: &ComputedDiegeticPanel,
    panel_widgets: Option<&PanelWidgets>,
    widgets: &Query<(&PanelWidget, &WidgetOf, Option<&Pickable>)>,
    markers: PickableMarkers,
) -> Vec<(usize, Entity)> {
    let mut matching = panel_widgets.map_or_else(Vec::new, |panel_widgets| {
        panel_widgets
            .iter()
            .filter_map(|widget_entity| {
                let (widget, widget_of, pickable) = widgets.get(widget_entity).ok()?;
                if widget_of.panel() != panel_entity
                    || pickable.is_some_and(|pickable| !pickable.is_hoverable)
                    || (matches!(markers, PickableMarkers::Required) && pickable.is_none())
                {
                    return None;
                }
                let record = computed
                    .widget_records()
                    .iter()
                    .find(|record| record.id() == widget.id())?;
                record
                    .clipped_rect()
                    .filter(|rect| rect.contains(panel_local))
                    .map(|_| (record.interaction_rank(), widget_entity))
            })
            .collect::<Vec<_>>()
    });
    matching.sort_by_key(|(rank, _)| *rank);
    matching
}

#[expect(
    clippy::cast_precision_loss,
    reason = "matches Bevy's mesh backend conversion of the complete isize camera-order domain"
)]
const fn camera_order(order: isize) -> f32 { order as f32 }

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::asset::AssetPlugin;
    use bevy::camera::NormalizedRenderTarget;
    use bevy::camera::visibility::RenderLayers;
    use bevy::camera::visibility::VisibilityPlugin;
    use bevy::ecs::message::MessageCursor;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::MeshPlugin;
    use bevy::picking::InteractionPlugin;
    use bevy::picking::PickingPlugin;
    use bevy::picking::PickingSettings;
    use bevy::picking::backend::PointerHits;
    use bevy::picking::backend::ray::RayId;
    use bevy::picking::backend::ray::RayMap;
    use bevy::picking::events::Out;
    use bevy::picking::events::Over;
    use bevy::picking::events::Pointer;
    use bevy::picking::hover::PickingInteraction;
    use bevy::picking::mesh_picking::MeshPickingPlugin;
    use bevy::picking::mesh_picking::MeshPickingSettings;
    use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
    use bevy::picking::mesh_picking::ray_cast::RayCastVisibility;
    use bevy::picking::pointer::Location;
    use bevy::picking::pointer::PointerId;
    use bevy::picking::pointer::PointerLocation;
    use bevy::picking::pointer::update_pointer_map;
    use bevy::prelude::*;
    use bevy::transform::TransformPlugin;
    use bevy::window::WindowRef;

    use super::camera_order;
    use crate::Button;
    use crate::DiegeticPanel;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::cascade;
    use crate::cascade::SdfMaterial;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::PanelOwned;
    use crate::render;
    use crate::render::PanelGeometryPlugin;
    use crate::render::PanelInteractionMesh;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::ComputedWidgetRecord;
    use crate::widgets::WidgetsPlugin;

    #[cfg(target_pointer_width = "64")]
    const LARGE_CAMERA_ORDER: isize = 1_isize << 40;
    #[cfg(target_pointer_width = "64")]
    const LARGE_CAMERA_ORDER_F32: f32 = 1_099_511_627_776.0;

    #[test]
    fn camera_order_preserves_common_signed_values() {
        assert_close(camera_order(-2), -2.0);
        assert_close(camera_order(3), 3.0);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn camera_order_preserves_values_outside_i32_range() {
        assert_close(camera_order(LARGE_CAMERA_ORDER), LARGE_CAMERA_ORDER_F32);
        assert_close(camera_order(-LARGE_CAMERA_ORDER), -LARGE_CAMERA_ORDER_F32);
    }

    struct PickingScene {
        app:             App,
        panel:           Entity,
        widget:          Entity,
        interaction:     Entity,
        back:            Entity,
        front_first:     Entity,
        front_last:      Entity,
        camera:          Entity,
        pointer:         PointerId,
        local_hit:       Vec3,
        background_hit:  Vec3,
        clipped_hit:     Vec3,
        overlap_hit:     Vec3,
        partial_inside:  Vec3,
        partial_outside: Vec3,
    }

    fn picking_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            MeshPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ));
        app.insert_resource(PickingSettings {
            is_input_enabled: false,
            is_window_picking_enabled: false,
            ..default()
        });
        app.init_asset::<Shader>().add_plugins((
            PickingPlugin,
            InteractionPlugin,
            MeshPickingPlugin,
        ));
        app.world_mut()
            .resource_mut::<MeshPickingSettings>()
            .ray_cast_visibility = RayCastVisibility::Visible;
        render::seed_default_material_cascades(&mut app);
        app.add_plugins(cascade::cascade_plugin::<SdfMaterial>())
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin, PanelGeometryPlugin));
        app
    }

    fn spawn_picking_panel(app: &mut App) -> Entity {
        let mut layout = LayoutBuilder::new(100.0, 50.0);
        layout.with(El::row().size(20.0, 10.0).clip(), |layout| {
            layout.with(
                El::new().size(30.0, 10.0).button("partial", Button::new()),
                |_| {},
            );
            layout.with(
                El::new().size(10.0, 10.0).button("clipped", Button::new()),
                |_| {},
            );
        });
        layout.with(El::overlay().size(20.0, 20.0), |layout| {
            layout.with(
                El::new()
                    .size(20.0, 20.0)
                    .button("back", Button::new())
                    .z_index(-1),
                |_| {},
            );
            layout.with(
                El::new()
                    .size(20.0, 20.0)
                    .button("front-first", Button::new())
                    .z_index(2),
                |_| {},
            );
            layout.with(
                El::new()
                    .size(20.0, 20.0)
                    .button("front-last", Button::new())
                    .z_index(2),
                |_| {},
            );
        });
        layout.with(El::row().size(100.0, 10.0), |layout| {
            layout.with(El::new().size(30.0, 10.0), |_| {});
            layout.with(
                El::new().size(20.0, 10.0).button("target", Button::new()),
                |_| {},
            );
        });
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(layout.build())
            .build()
            .expect("panel builds");
        app.world_mut().spawn(panel).id()
    }

    fn spawn_picking_input(app: &mut App) -> (Entity, PointerId) {
        let camera = app
            .world_mut()
            .spawn(Camera {
                order: 7,
                ..default()
            })
            .id();
        let pointer = PointerId::Touch(42);
        let window = app.world_mut().spawn(Window::default()).id();
        let window = WindowRef::Entity(window)
            .normalize(None)
            .expect("entity window references always normalize");
        let pointer_location = PointerLocation::new(Location {
            target:   NormalizedRenderTarget::Window(window),
            position: Vec2::ZERO,
        });
        app.world_mut().spawn((pointer, pointer_location.clone()));
        app.world_mut().spawn((PointerId::Mouse, pointer_location));
        app.world_mut()
            .run_system_once(update_pointer_map)
            .expect("pointer map updates");
        (camera, pointer)
    }

    fn build_picking_scene() -> PickingScene {
        let mut app = picking_app();
        let panel = spawn_picking_panel(&mut app);
        app.update();
        app.update();

        let widget = resolve_widget(&mut app, panel, "target");
        let back = resolve_widget(&mut app, panel, "back");
        let front_first = resolve_widget(&mut app, panel, "front-first");
        let front_last = resolve_widget(&mut app, panel, "front-last");
        let interaction = interaction_mesh(&mut app, panel);
        let (camera, pointer) = spawn_picking_input(&mut app);
        let local_hit = record_center_world(&app, panel, "target");
        let overlap_hit = record_center_world(&app, panel, "front-last");
        let clipped_hit = record_center_world(&app, panel, "clipped");
        let partial_record = widget_record(&app, panel, "partial");
        let partial_clip = partial_record
            .clipped_rect()
            .expect("partially clipped widget should retain a visible rect");
        let partial_inside = panel_point_to_world(&app, panel, partial_clip.center());
        let outside_point = Vec2::new(
            partial_record.rect().x + partial_record.rect().width - 0.5,
            partial_record
                .rect()
                .height
                .mul_add(0.5, partial_record.rect().y),
        );
        assert!(!partial_clip.contains(outside_point));
        let panel_component = app
            .world()
            .get::<DiegeticPanel>(panel)
            .expect("panel should remain live");
        let layout_to_points = panel_component.layout_unit().to_points();
        let background_hit = panel_point_to_world(
            &app,
            panel,
            (
                panel_component.width().mul_add(layout_to_points, -1.0),
                panel_component.height().mul_add(layout_to_points, -1.0),
            ),
        );
        let partial_outside = panel_point_to_world(&app, panel, (outside_point.x, outside_point.y));

        PickingScene {
            app,
            panel,
            widget,
            interaction,
            back,
            front_first,
            front_last,
            camera,
            pointer,
            local_hit,
            background_hit,
            clipped_hit,
            overlap_hit,
            partial_inside,
            partial_outside,
        }
    }

    fn resolve_widget(app: &mut App, panel: Entity, id: &str) -> Entity {
        let id = PanelElementId::named(id);
        app.world_mut()
            .run_system_once(move |reader: PanelWidgetReader| reader.entity(panel, &id))
            .expect("reader system should run")
            .expect("widget should be reified")
    }

    fn interaction_mesh(app: &mut App, panel: Entity) -> Entity {
        let world = app.world_mut();
        let mut query = world.query::<(Entity, &PanelInteractionMesh, &PanelOwned)>();
        let entities = query
            .iter(world)
            .filter(|(_, _, ownership)| ownership.owner() == panel)
            .map(|(entity, _, _)| entity)
            .collect::<Vec<_>>();
        assert_eq!(
            entities.len(),
            1,
            "production should create one interaction mesh for the panel"
        );
        entities[0]
    }

    fn widget_record<'a>(app: &'a App, panel: Entity, id: &str) -> &'a ComputedWidgetRecord {
        app.world()
            .get::<ComputedDiegeticPanel>(panel)
            .expect("panel should have computed output")
            .widget_records()
            .iter()
            .find(|record| record.id() == &PanelElementId::named(id))
            .expect("widget should have a production computed record")
    }

    fn record_center_world(app: &App, panel: Entity, id: &str) -> Vec3 {
        let record = widget_record(app, panel, id);
        panel_point_to_world(app, panel, record.rect().center())
    }

    fn panel_point_to_world(app: &App, panel: Entity, point: (f32, f32)) -> Vec3 {
        let panel_component = app
            .world()
            .get::<DiegeticPanel>(panel)
            .expect("panel remains live");
        let (anchor_x, anchor_y) = panel_component.anchor_offsets();
        let scale = panel_component.points_to_world();
        let local = Vec3::new(
            point.0.mul_add(scale, -anchor_x),
            point.1.mul_add(-scale, anchor_y),
            0.0,
        );
        app.world()
            .get::<GlobalTransform>(panel)
            .expect("panel should have a global transform")
            .transform_point(local)
    }

    #[track_caller]
    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[track_caller]
    fn assert_hit_group(group: &PointerHits, scene: &PickingScene) {
        assert_eq!(group.pointer, scene.pointer);
        assert_close(group.order, 7.0);
        assert!(
            group
                .picks
                .iter()
                .all(|(_, hit)| hit.camera == scene.camera)
        );
        assert!(
            group
                .picks
                .iter()
                .any(|(entity, _)| *entity == scene.widget)
        );
        assert!(group.picks.iter().any(|(entity, _)| *entity == scene.panel));
        assert!(
            !group
                .picks
                .iter()
                .any(|(entity, _)| *entity == scene.interaction)
        );
        let widget_depth = group
            .picks
            .iter()
            .find(|(entity, _)| *entity == scene.widget)
            .map(|(_, hit)| hit.depth)
            .expect("widget hit has depth");
        let panel_depth = group
            .picks
            .iter()
            .find(|(entity, _)| *entity == scene.panel)
            .map(|(_, hit)| hit.depth)
            .expect("panel hit has depth");
        assert!(widget_depth < panel_depth);
    }

    fn settle_panel_source_change(scene: &mut PickingScene) {
        scene.app.world_mut().resource_mut::<RayMap>().map.clear();
        scene.app.update();
    }

    fn cast_scene_ray(
        scene: &mut PickingScene,
        cursor: &mut MessageCursor<PointerHits>,
        origin: Vec3,
        direction: Dir3,
    ) -> Vec<PointerHits> {
        scene.app.world_mut().resource_mut::<RayMap>().map.insert(
            RayId::new(scene.camera, scene.pointer),
            Ray3d::new(origin, direction),
        );
        scene.app.update();
        cursor
            .read(scene.app.world().resource::<Messages<PointerHits>>())
            .cloned()
            .collect()
    }

    #[test]
    fn front_and_back_rays_report_off_origin_widget_before_panel_root() {
        let mut scene = build_picking_scene();
        let mut cursor = MessageCursor::<PointerHits>::default();

        for (origin, direction) in [
            (scene.local_hit + Vec3::Z, Dir3::NEG_Z),
            (scene.local_hit - Vec3::Z, Dir3::Z),
        ] {
            let hits = cast_scene_ray(&mut scene, &mut cursor, origin, direction);
            assert_eq!(hits.len(), 1);
            assert_hit_group(&hits[0], &scene);
        }
    }

    #[test]
    fn production_interaction_surface_has_two_sided_picking_components() {
        let scene = build_picking_scene();
        let world = scene.app.world();

        assert!(world.get::<RayCastBackfaces>(scene.interaction).is_some());
        assert_eq!(
            world
                .get::<PanelOwned>(scene.interaction)
                .map(|ownership| ownership.owner()),
            Some(scene.panel),
        );
        let pickable = world
            .get::<Pickable>(scene.interaction)
            .expect("interaction mesh should carry Pickable::IGNORE");
        assert!(!pickable.is_hoverable);
        assert!(!pickable.should_block_lower);
        let material_handle = world
            .get::<MeshMaterial3d<StandardMaterial>>(scene.interaction)
            .expect("interaction mesh should carry a standard material");
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&material_handle.0)
            .expect("interaction material should remain in assets");
        assert!(material.double_sided);
        assert_eq!(material.cull_mode, None);
    }

    #[test]
    fn reparented_interaction_surface_keeps_its_recorded_panel_identity() {
        let mut scene = build_picking_scene();
        let other_panel = spawn_picking_panel(&mut scene.app);
        scene.app.update();
        scene.app.update();
        let other_interaction = interaction_mesh(&mut scene.app, other_panel);
        scene
            .app
            .world_mut()
            .entity_mut(other_interaction)
            .despawn();
        scene
            .app
            .world_mut()
            .entity_mut(scene.interaction)
            .insert(ChildOf(other_panel));

        scene
            .app
            .world_mut()
            .get_mut::<ComputedDiegeticPanel>(scene.panel)
            .expect("recorded owner should retain computed output")
            .set_changed();
        scene.app.update();

        assert_eq!(
            interaction_mesh(&mut scene.app, scene.panel),
            scene.interaction
        );
        scene
            .app
            .world_mut()
            .entity_mut(other_panel)
            .insert(RenderLayers::layer(6));
        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(RenderLayers::layer(4));
        scene.app.update();
        assert_eq!(
            scene.app.world().get::<RenderLayers>(scene.interaction),
            Some(&RenderLayers::layer(4)),
        );

        scene
            .app
            .world_mut()
            .entity_mut(scene.camera)
            .insert(RenderLayers::layer(4));
        let mut cursor = MessageCursor::<PointerHits>::default();
        let origin = scene.local_hit + Vec3::Z;
        let hits = cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z);
        assert_eq!(hits.len(), 1);
        assert_hit_group(&hits[0], &scene);
        assert!(
            !hits[0]
                .picks
                .iter()
                .any(|(entity, _)| *entity == other_panel),
        );

        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .remove::<DiegeticPanel>();
        scene.app.update();
        assert!(scene.app.world().get_entity(scene.interaction).is_err());
        assert!(scene.app.world().get_entity(other_panel).is_ok());
    }

    #[test]
    fn mouse_and_touch_hits_drive_widget_hover_and_out_presentation() {
        let mut scene = build_picking_scene();
        let ray = Ray3d::new(scene.local_hit + Vec3::Z, Dir3::NEG_Z);
        scene
            .app
            .world_mut()
            .resource_mut::<RayMap>()
            .map
            .insert(RayId::new(scene.camera, scene.pointer), ray);
        scene
            .app
            .world_mut()
            .resource_mut::<RayMap>()
            .map
            .insert(RayId::new(scene.camera, PointerId::Mouse), ray);
        let mut over_cursor = MessageCursor::<Pointer<Over>>::default();
        scene.app.update();

        assert_eq!(
            scene.app.world().get::<PickingInteraction>(scene.widget),
            Some(&PickingInteraction::Hovered)
        );
        let over_pointers = over_cursor
            .read(scene.app.world().resource::<Messages<Pointer<Over>>>())
            .filter(|event| event.entity == scene.widget)
            .map(|event| event.pointer_id)
            .collect::<Vec<_>>();
        assert!(over_pointers.contains(&PointerId::Mouse));
        assert!(over_pointers.contains(&scene.pointer));

        scene.app.world_mut().resource_mut::<RayMap>().map.clear();
        let mut out_cursor = MessageCursor::<Pointer<Out>>::default();
        scene.app.update();

        assert_eq!(
            scene.app.world().get::<PickingInteraction>(scene.widget),
            Some(&PickingInteraction::None)
        );
        let out_pointers = out_cursor
            .read(scene.app.world().resource::<Messages<Pointer<Out>>>())
            .filter(|event| event.entity == scene.widget)
            .map(|event| event.pointer_id)
            .collect::<Vec<_>>();
        assert!(out_pointers.contains(&PointerId::Mouse));
        assert!(out_pointers.contains(&scene.pointer));
    }

    #[test]
    fn panel_background_reports_only_the_panel_root() {
        let mut scene = build_picking_scene();
        let mut cursor = MessageCursor::<PointerHits>::default();
        let origin = scene.background_hit + Vec3::Z;
        let hits = cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].picks.len(), 1);
        assert_eq!(hits[0].picks[0].0, scene.panel);
    }

    #[test]
    fn production_records_gate_partial_and_full_ancestor_clipping() {
        let mut scene = build_picking_scene();
        let partial = resolve_widget(&mut scene.app, scene.panel, "partial");
        let clipped = resolve_widget(&mut scene.app, scene.panel, "clipped");
        let mut cursor = MessageCursor::<PointerHits>::default();
        let partial_inside = scene.partial_inside;
        let partial_outside = scene.partial_outside;
        let clipped_hit = scene.clipped_hit;

        let inside = cast_scene_ray(
            &mut scene,
            &mut cursor,
            partial_inside + Vec3::Z,
            Dir3::NEG_Z,
        );
        assert_eq!(inside.len(), 1);
        assert!(inside[0].picks.iter().any(|(entity, _)| *entity == partial));

        let outside = cast_scene_ray(
            &mut scene,
            &mut cursor,
            partial_outside + Vec3::Z,
            Dir3::NEG_Z,
        );
        assert_eq!(outside.len(), 1);
        assert!(
            !outside[0]
                .picks
                .iter()
                .any(|(entity, _)| *entity == partial)
        );

        let fully_clipped =
            cast_scene_ray(&mut scene, &mut cursor, clipped_hit + Vec3::Z, Dir3::NEG_Z);
        assert_eq!(fully_clipped.len(), 1);
        assert!(
            !fully_clipped[0]
                .picks
                .iter()
                .any(|(entity, _)| *entity == clipped)
        );
    }

    #[test]
    fn actual_ray_selects_highest_z_then_latest_source_widget() {
        let mut scene = build_picking_scene();
        let mut cursor = MessageCursor::<PointerHits>::default();
        let overlap_hit = scene.overlap_hit;
        let hits = cast_scene_ray(&mut scene, &mut cursor, overlap_hit + Vec3::Z, Dir3::NEG_Z);

        assert_eq!(hits.len(), 1);
        let depth = |entity| {
            hits[0]
                .picks
                .iter()
                .find(|(picked, _)| *picked == entity)
                .map(|(_, hit)| hit.depth)
                .expect("overlapping widget should be in the production hit group")
        };
        assert!(depth(scene.front_last) < depth(scene.front_first));
        assert!(depth(scene.front_first) < depth(scene.back));
        let nearest = hits[0]
            .picks
            .iter()
            .min_by(|left, right| left.1.depth.total_cmp(&right.1.depth))
            .map(|(entity, _)| *entity)
            .expect("hit group should not be empty");
        assert_eq!(nearest, scene.front_last);
    }

    #[test]
    fn visibility_layers_and_panel_pickable_filter_panel_hits() {
        let mut scene = build_picking_scene();
        let mut cursor = MessageCursor::<PointerHits>::default();
        let origin = scene.local_hit + Vec3::Z;

        scene
            .app
            .world_mut()
            .entity_mut(scene.camera)
            .insert(RenderLayers::layer(1));
        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(RenderLayers::layer(2));
        settle_panel_source_change(&mut scene);
        assert!(cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).is_empty());

        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(RenderLayers::layer(1));
        settle_panel_source_change(&mut scene);
        assert_eq!(
            cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).len(),
            1
        );

        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(Visibility::Hidden);
        settle_panel_source_change(&mut scene);
        assert!(cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).is_empty());

        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(Visibility::Visible);
        settle_panel_source_change(&mut scene);
        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(Pickable::IGNORE);
        assert!(cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).is_empty());
    }

    #[test]
    fn removing_panel_layers_resets_the_production_interaction_surface() {
        let mut scene = build_picking_scene();
        let mut cursor = MessageCursor::<PointerHits>::default();
        let origin = scene.local_hit + Vec3::Z;
        scene
            .app
            .world_mut()
            .entity_mut(scene.camera)
            .insert(RenderLayers::layer(3));
        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .insert(RenderLayers::layer(3));
        settle_panel_source_change(&mut scene);
        assert_eq!(
            cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).len(),
            1
        );

        scene
            .app
            .world_mut()
            .entity_mut(scene.panel)
            .remove::<RenderLayers>();
        settle_panel_source_change(&mut scene);
        assert!(cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).is_empty());

        scene
            .app
            .world_mut()
            .entity_mut(scene.camera)
            .remove::<RenderLayers>();
        assert_eq!(
            cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z).len(),
            1
        );
    }

    #[test]
    fn two_cameras_keep_their_camera_and_order_in_separate_groups() {
        let mut scene = build_picking_scene();
        let second_camera = scene
            .app
            .world_mut()
            .spawn(Camera {
                order: 11,
                ..default()
            })
            .id();
        let origin = scene.local_hit + Vec3::Z;
        scene.app.world_mut().resource_mut::<RayMap>().map.insert(
            RayId::new(second_camera, scene.pointer),
            Ray3d::new(origin, Dir3::NEG_Z),
        );
        let mut cursor = MessageCursor::<PointerHits>::default();
        let mut hits = cast_scene_ray(&mut scene, &mut cursor, origin, Dir3::NEG_Z);
        hits.sort_by(|left, right| left.order.total_cmp(&right.order));

        assert_eq!(hits.len(), 2);
        assert_close(hits[0].order, 7.0);
        assert_close(hits[1].order, 11.0);
        assert!(
            hits[0]
                .picks
                .iter()
                .all(|(_, hit)| hit.camera == scene.camera)
        );
        assert!(
            hits[1]
                .picks
                .iter()
                .all(|(_, hit)| hit.camera == second_camera)
        );
    }
}
