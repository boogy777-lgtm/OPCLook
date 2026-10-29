//! Region ownership and one-shot entry requests for keyboard navigation.
//! Focused egui widgets remain the source of subfocus within each region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Region {
    Header,
    Tags,
    Servers,
    Footer,
}

pub(super) const ORDER: [Region; 4] = [
    Region::Header,
    Region::Tags,
    Region::Servers,
    Region::Footer,
];

fn owner_id() -> egui::Id {
    egui::Id::unique("opclook_keyboard_region")
}
fn entry_id() -> egui::Id {
    egui::Id::unique("opclook_keyboard_region_entry")
}

pub(super) fn current(ctx: &egui::Context) -> Option<Region> {
    ctx.data(|data| data.get_temp(owner_id()))
}

pub(super) fn activate(ctx: &egui::Context, region: Region) {
    ctx.data_mut(|data| {
        data.insert_temp(owner_id(), region);
        data.insert_temp(entry_id(), region);
    });
    ctx.request_repaint();
}

pub(super) fn pointer_enter(ctx: &egui::Context, region: Region) {
    // A menu popup can overlap another panel; its pointer press belongs to
    // the foreground layer, not to the panel behind that rectangle.
    if egui::Popup::is_any_open(ctx)
        && region != Region::Header
        && ctx
            .input(|input| input.pointer.interact_pos())
            .and_then(|pos| ctx.layer_id_at(pos))
            .is_some_and(|layer| layer.order == egui::Order::Foreground)
    {
        return;
    }
    ctx.data_mut(|data| {
        data.insert_temp(owner_id(), region);
        data.remove::<Region>(entry_id());
    });
}

pub(super) fn take_entry(ctx: &egui::Context, region: Region) -> bool {
    ctx.data_mut(|data| {
        if data.get_temp::<Region>(entry_id()) == Some(region) {
            data.remove::<Region>(entry_id());
            true
        } else {
            false
        }
    })
}

pub(super) fn step(current: Option<Region>, reverse: bool) -> Region {
    let index = current.and_then(|region| ORDER.iter().position(|item| *item == region));
    match index {
        Some(index) if reverse => ORDER[(index + ORDER.len() - 1) % ORDER.len()],
        Some(index) => ORDER[(index + 1) % ORDER.len()],
        None if reverse => Region::Footer,
        None => Region::Header,
    }
}
