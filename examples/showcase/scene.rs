use super::*;

#[derive(Component)]
pub(super) enum MeshShape {
    Cuboid(Vec3),
    Sphere(f32),
    Torus {
        minor_radius: f32,
        major_radius: f32,
    },
}

pub(super) fn spawn_scene_objects(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    // Ground plane (clickable from above — deselects and zooms to scene bounds)
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.3, 0.5, 0.3).with_alpha(0.85),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(pointer::on_ground_clicked)
        .id();

    // Underside plane (clickable from below — deselects and animates back to scene)
    commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_rotation(Quat::from_rotation_x(PI)),
        ))
        .observe(pointer::on_below_clicked);

    // Directional light
    commands.spawn((
        DirectionalLight {
            illuminance: 3000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, PI / 4.0, -PI / 4.0)),
    ));

    // Cuboid
    let cuboid_size = Vec3::new(1.0, 1.0, 1.0);
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(cuboid_size.x, cuboid_size.y, cuboid_size.z))),
            MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
            Transform::from_xyz(-2.5, MESH_CENTER_Y, 0.0),
            MeshShape::Cuboid(cuboid_size),
        ))
        .observe(pointer::on_mesh_clicked)
        .observe(pointer::on_mesh_dragged)
        .observe(pointer::on_mesh_hover)
        .observe(pointer::on_mesh_unhover);

    // Sphere
    let sphere_radius = 0.5;
    commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(sphere_radius).mesh().uv(128, 64))),
            MeshMaterial3d(materials.add(Color::srgb(0.9, 0.3, 0.2))),
            Transform::from_xyz(0.0, MESH_CENTER_Y, 0.0),
            MeshShape::Sphere(sphere_radius),
        ))
        .observe(pointer::on_mesh_clicked)
        .observe(pointer::on_mesh_dragged)
        .observe(pointer::on_mesh_hover)
        .observe(pointer::on_mesh_unhover);

    // Torus
    let torus_minor = 0.25;
    let torus_major = 0.75;
    commands
        .spawn((
            Mesh3d(
                meshes.add(
                    Torus::new(torus_minor, torus_major)
                        .mesh()
                        .minor_resolution(64)
                        .major_resolution(64),
                ),
            ),
            MeshMaterial3d(materials.add(Color::srgb(0.9, 0.5, 0.1))),
            Transform::from_xyz(2.5, MESH_CENTER_Y, 0.0),
            MeshShape::Torus {
                minor_radius: torus_minor,
                major_radius: torus_major,
            },
        ))
        .observe(pointer::on_mesh_clicked)
        .observe(pointer::on_mesh_dragged)
        .observe(pointer::on_mesh_hover)
        .observe(pointer::on_mesh_unhover);

    ground
}
