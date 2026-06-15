use bevy::color::Color;
use bevy_diegetic::ChildLayoutState;
use bevy_diegetic::ChildDivider;
use bevy_diegetic::Column;
use bevy_diegetic::El;
use bevy_diegetic::Overlay;
use bevy_diegetic::Padding;
use bevy_diegetic::Row;

fn row_panel() -> El<Row> { El::row() }

fn column_panel() -> El<Column> { El::column() }

fn overlay_panel() -> El<Overlay> { El::overlay() }

fn decorate<L: ChildLayoutState>(el: El<L>) -> El<L> { el.padding(Padding::all(1.0)) }

fn main() {
    let _ = decorate(row_panel().gap(1.0).child_divider(ChildDivider::new(1.0, Color::WHITE)));
    let _ =
        decorate(column_panel().gap(1.0).child_divider(ChildDivider::new(1.0, Color::WHITE)));
    let _ = decorate(overlay_panel());
}
