use super::*;

#[derive(Component)]
pub(crate) struct Selected;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub(crate) struct SelectionGizmo;

pub(crate) fn init_selection_gizmo(mut config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<SelectionGizmo>();
    config.depth_bias = GIZMO_DEPTH_BIAS;
    config.line.width = GIZMO_LINE_WIDTH;
    config.render_layers = RenderLayers::layer(SELECTION_GIZMO_LAYER);
}

fn draw_shape_gizmo(
    gizmos: &mut Gizmos<SelectionGizmo>,
    transform: &Transform,
    shape: &scene::MeshShape,
    color: Color,
) {
    match shape {
        scene::MeshShape::Cuboid(size) => {
            gizmos.cube(
                Transform::from_translation(transform.translation)
                    .with_rotation(transform.rotation)
                    .with_scale(*size * GIZMO_SCALE),
                color,
            );
        },
        scene::MeshShape::Sphere(radius) => {
            gizmos.sphere(
                Isometry3d::new(transform.translation, transform.rotation),
                radius * GIZMO_SCALE,
                color,
            );
        },
        scene::MeshShape::Torus {
            minor_radius,
            major_radius,
        } => {
            gizmos.primitive_3d(
                &Torus::new(*minor_radius * GIZMO_SCALE, *major_radius * GIZMO_SCALE),
                Isometry3d::new(transform.translation, transform.rotation),
                color,
            );
        },
    }
}

pub(crate) fn draw_selection_gizmo(
    mut gizmos: Gizmos<SelectionGizmo>,
    query: Query<(&Transform, &scene::MeshShape), With<Selected>>,
) {
    let color = Color::from(DEEP_SKY_BLUE);
    for (transform, shape) in &query {
        draw_shape_gizmo(&mut gizmos, transform, shape, color);
    }
}

pub(crate) fn draw_hover_gizmo(
    mut gizmos: Gizmos<SelectionGizmo>,
    hovered: Res<pointer::HoveredEntity>,
    query: Query<(&Transform, &scene::MeshShape), Without<Selected>>,
) {
    let Some(entity) = hovered.0 else {
        return;
    };
    let Ok((transform, shape)) = query.get(entity) else {
        return;
    };
    draw_shape_gizmo(&mut gizmos, transform, shape, Color::from(ORANGE));
}

type GizmoLayerQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static FitOverlay>,
        Option<&'static RenderLayers>,
    ),
    With<OrbitCam>,
>;

/// Cameras with `FitOverlay` lose the selection gizmo layer so the outline
/// doesn't compete with the debug overlay. Cameras without it get the layer back.
pub(crate) fn sync_selection_gizmo_layers(mut commands: Commands, camera_query: GizmoLayerQuery) {
    let with_selection = RenderLayers::from_layers(&[0, SELECTION_GIZMO_LAYER]);
    let without_selection = RenderLayers::layer(0);

    for (entity, has_visualization, current_layers) in &camera_query {
        let desired = if has_visualization.is_some() {
            &without_selection
        } else {
            &with_selection
        };
        if current_layers != Some(desired) {
            commands.entity(entity).insert(desired.clone());
        }
    }
}
