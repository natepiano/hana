use bevy::color::Color;
use hana_diegetic::ChildLayoutState;
use hana_diegetic::ChildDivider;
use hana_diegetic::Column;
use hana_diegetic::El;
use hana_diegetic::Overlay;
use hana_diegetic::Padding;
use hana_diegetic::Row;

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
