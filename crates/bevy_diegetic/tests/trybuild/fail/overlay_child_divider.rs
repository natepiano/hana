use bevy::color::Color;
use bevy_diegetic::ChildDivider;
use bevy_diegetic::El;
use bevy_diegetic::In;

fn main() {
    let _ = El::overlay().child_divider(ChildDivider::new(In(0.01), Color::WHITE));
}
