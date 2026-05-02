//! Demonstrates core `bevy_kana` types and their compile-time safety.
//!
//! Run with: `cargo run --example basics`

use bevy::math::Quat;
use bevy::math::Vec2;
use bevy::math::Vec3;
use bevy_kana::Displacement;
use bevy_kana::Orientation;
use bevy_kana::Position;
use bevy_kana::ScreenPosition;
use bevy_kana::Velocity;

fn main() {
    println!("=== Semantic types: compile-time safety ===\n");

    // Same-type arithmetic works
    let start_position = Position(Vec3::new(1.0, 0.0, 0.0));
    let end_position = Position(Vec3::new(3.0, 4.0, 0.0));
    let centroid = (start_position + end_position) / 2.0;
    println!("Centroid of {start_position:?} and {end_position:?}: {centroid:?}");

    // Cross-type mixing is a compile error — uncomment to see:
    // let velocity = Velocity(Vec3::new(1.0, 0.0, 0.0));
    // let bad = start_position + velocity;  // ERROR: expected `Position`, found `Velocity`

    println!("\n=== Deref: transparent access to inner type ===\n");

    let position = Position(Vec3::new(10.0, 0.0, 5.0));
    let velocity = Velocity(Vec3::new(1.0, 0.0, -0.5));
    println!("position.x = {}", position.x);
    println!("velocity.length() = {}", velocity.length());

    // Use `Deref` to access `Vec3` methods like dot product
    println!("dot(position, velocity) = {}", position.dot(*velocity));

    println!("\n=== into_inner / From / Into: Bevy API interop ===\n");

    // Escape hatch for APIs that expect raw `Vec3`
    let raw: Vec3 = position.into_inner();
    println!("into_inner: {raw:?}");

    // `From`/`Into` conversions work both ways
    let from_vec: Position = Vec3::new(1.0, 2.0, 3.0).into();
    let back_to_vec: Vec3 = from_vec.into();
    println!("roundtrip: {back_to_vec:?}");

    println!("\n=== Orientation: rotation wrapper ===\n");

    let rotation = Orientation::from(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    let rotated = rotation * Vec3::X;
    println!("X rotated 90° around Y: {rotated:?}");

    // Composition
    let double = rotation * rotation;
    let result = double * Vec3::X;
    println!("X rotated 180° around Y: {result:?}");

    // Inverse
    let undone = rotation * rotation.inverse();
    let identity_result = undone * Vec3::X;
    println!("Rotation * inverse = identity: {identity_result:?}");

    // Interpolation
    let start_orientation = Orientation::from(Quat::IDENTITY);
    let end_orientation = Orientation::from(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    let halfway = start_orientation.slerp(end_orientation, 0.5);
    let slerp_result = halfway * Vec3::X;
    println!("Slerp halfway (0° to 90°): {slerp_result:?}");

    println!("\n=== Displacement and Velocity ===\n");

    let displacement = Displacement(Vec3::new(5.0, 0.0, 0.0));
    let velocity = Velocity(Vec3::new(2.0, 0.0, 0.0));

    let total = displacement + displacement;
    let combined = velocity + velocity;
    println!("Double displacement: {total:?}");
    println!("Combined velocity: {combined:?}");

    // Scale velocity by the frame time delta for per-frame movement
    let time_delta = 0.016;
    let frame_velocity = velocity * time_delta;
    println!("Velocity * time_delta({time_delta}): {frame_velocity:?}");

    println!("\n=== ScreenPosition: 2D pixel-space ===\n");

    let cursor = ScreenPosition(Vec2::new(640.0, 480.0));
    let offset = ScreenPosition(Vec2::new(10.0, -5.0));
    let moved = cursor + offset;
    println!("Cursor {cursor:?} + offset {offset:?} = {moved:?}");
}
