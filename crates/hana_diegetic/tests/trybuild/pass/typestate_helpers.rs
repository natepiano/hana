use bevy::color::Color;
use bevy::image::Image;
use bevy::asset::Handle;
use hana_diegetic::ChildLayoutState;
use hana_diegetic::ChildDivider;
use hana_diegetic::Column;
use hana_diegetic::El;
use hana_diegetic::Appearance;
use hana_diegetic::ImeAppOwnedFieldSpec;
use hana_diegetic::ImeEditableFieldSpec;
use hana_diegetic::LayoutBuilder;
use hana_diegetic::LayoutContentBuilder;
use hana_diegetic::Overlay;
use hana_diegetic::Padding;
use hana_diegetic::Row;
use hana_diegetic::Slider;
use hana_diegetic::Text;
use hana_diegetic::TextStyle;
use hana_diegetic::WidgetBuilder;
use hana_diegetic::WidgetPart;

fn row_panel() -> El<Row> { El::row() }

fn column_panel() -> El<Column> { El::column() }

fn overlay_panel() -> El<Overlay> { El::overlay() }

fn decorate<L: ChildLayoutState>(el: El<L>) -> El<L> { el.padding(Padding::all(1.0)) }

fn disabled_part() -> El<Row, WidgetPart> {
    El::new()
        .background(Color::WHITE)
        .disabled(Appearance::new().background(Color::BLACK))
}

fn add_widget_content(builder: &mut WidgetBuilder<'_, Slider>) {
    builder.with(El::column(), |builder| {
        builder.text(
            Text::new("label", TextStyle::default()).layout(
                El::new()
                    .background(Color::NONE)
                    .disabled(Appearance::new().background(Color::BLACK)),
            ),
        );
        builder.image(
            disabled_part(),
            Handle::<Image>::default(),
            Color::WHITE,
        );
    });
}

fn add_ordinary_content(builder: &mut impl LayoutContentBuilder) {
    builder.with(El::column(), |builder| {
        builder.text(Text::new("ordinary", TextStyle::default()));
    });
}

fn field_spec() -> ImeEditableFieldSpec {
    ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("gain"))
}

fn widget_roots() {
    let mut button = LayoutBuilder::with_widget_root(El::new().button("button"));
    button.with(disabled_part(), |_| {});
    let _ = button.build();

    let mut slider = LayoutBuilder::with_widget_root(El::new().widget("slider", Slider::new(0.0..=1.0)));
    add_ordinary_content(&mut slider);
    add_widget_content(&mut slider);
    let _ = slider.build();

    let mut editable = LayoutBuilder::with_widget_root(El::new().editable_field("editable", field_spec()));
    editable.with(disabled_part(), |_| {});
    let _ = editable.build();
}

fn main() {
    let _ = decorate(row_panel().gap(1.0).child_divider(ChildDivider::new(1.0, Color::WHITE)));
    let _ =
        decorate(column_panel().gap(1.0).child_divider(ChildDivider::new(1.0, Color::WHITE)));
    let _ = decorate(overlay_panel());
    let mut panel = LayoutBuilder::new(100.0, 50.0);
    add_ordinary_content(&mut panel);
    panel.with(El::new().widget("slider", Slider::new(0.0..=1.0)), |builder| {
        builder.with(disabled_part(), |builder| {
            builder.with(disabled_part(), |_| {});
        });
    });
    let _ = panel.build();
    widget_roots();
}
