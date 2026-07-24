use hana_diegetic::Button;
use hana_diegetic::El;
use hana_diegetic::Tooltip;

fn main() {
    let widget = El::new().button("nested", Button::new());
    let _ = Tooltip::new(widget);
}
