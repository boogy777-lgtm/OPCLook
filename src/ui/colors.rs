use crate::bff::model::QualityKind;

pub(super) fn quality_color(kind: QualityKind) -> egui::Color32 {
    match kind {
        QualityKind::Good => green(),
        QualityKind::Uncertain => yellow(),
        QualityKind::Bad => red(),
    }
}

pub(super) fn green() -> egui::Color32 {
    egui::Color32::from_rgb(64, 200, 96)
}
pub(super) fn yellow() -> egui::Color32 {
    egui::Color32::from_rgb(230, 180, 40)
}

pub(super) fn red() -> egui::Color32 {
    egui::Color32::from_rgb(235, 90, 80)
}
