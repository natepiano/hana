use bevy_diegetic::ChildLayoutState;
use bevy_diegetic::Column;
use bevy_diegetic::El;
use bevy_diegetic::Padding;
use bevy_diegetic::Row;

fn row_panel() -> El<Row> { El::row() }

fn column_panel() -> El<Column> { El::column() }

fn decorate<L: ChildLayoutState>(el: El<L>) -> El<L> { el.padding(Padding::all(1.0)) }

fn main() {
    let _ = decorate(row_panel());
    let _ = decorate(column_panel());
}
