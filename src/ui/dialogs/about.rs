use super::dialog_spacing;
use crate::app::{App, SessionState};
use crate::i18n::{Tr, tr};

pub(super) fn about_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.about.open {
        app.dialogs.about.certificate_subject = None;
        return;
    }
    let subject = app
        .dialogs
        .about
        .certificate_subject
        .get_or_insert_with(crate::bff::ua::client_cert_subject)
        .clone();
    let lang = app.lang();
    let mut close = false;
    egui::Window::new(tr(lang, Tr::AboutTitle))
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let esc = !crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| i.key_pressed(egui::Key::Escape));
            ui.heading(tr(lang, Tr::TestOpcClient));
            ui.label(tr(lang, Tr::RustEgui2026));
            ui.separator();
            ui.label(tr(lang, Tr::RewrittenFrom));
            ui.label(tr(lang, Tr::OriginalProto));
            ui.label(tr(lang, Tr::ReverseInfo));
            ui.separator();
            ui.label(tr(lang, Tr::SupportedLabel));
            ui.label(tr(lang, Tr::AboutDaLine));
            ui.label(tr(lang, Tr::AboutUaLine));
            ui.label(tr(lang, Tr::AboutTreeLine));
            ui.label(tr(lang, Tr::AboutSettingsLine));
            ui.separator();

            ui.label(egui::RichText::new(tr(lang, Tr::ConnectedServers)).strong());
            if app.sessions.is_empty() {
                ui.label(tr(lang, Tr::NoConnectedServers));
            } else {
                for session in &app.sessions {
                    let state = match session.state {
                        SessionState::Connected => tr(lang, Tr::Connected),
                        SessionState::Connecting => tr(lang, Tr::Connecting),
                        SessionState::Disconnected => tr(lang, Tr::NoLink),
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "• {} ({}) — {state}",
                            session.endpoint().display_name(),
                            session.protocol()
                        ));
                        if !session.info.is_empty() {
                            ui.label(egui::RichText::new(&session.info).weak());
                        }
                    });
                }
                ui.label(egui::RichText::new(tr(lang, Tr::VendorNotAvail)).weak());
            }

            ui.separator();
            ui.label(egui::RichText::new(tr(lang, Tr::LimitationsLabel)).strong());
            ui.label(tr(lang, Tr::LimitScriptLine));
            ui.label(tr(lang, Tr::LimitQualityLine));
            ui.label(tr(lang, Tr::LimitDcomLine));
            ui.separator();
            // PKI / client certificate info (best effort).
            ui.label(egui::RichText::new(tr(lang, Tr::PkiPathLabel)).strong());
            ui.label(crate::bff::ua::pki_dir_public().display().to_string());
            ui.label(egui::RichText::new(tr(lang, Tr::ClientCertSubjectLabel)).strong());
            match &subject {
                Some(subject) => {
                    ui.label(subject);
                }
                None => {
                    ui.label(tr(lang, Tr::NoClientCert));
                }
            }
            ui.separator();
            if ui.button(tr(lang, Tr::Close)).clicked() {
                close = true;
            }
            if esc {
                close = true;
            }
        });
    if close {
        app.dialogs.about.open = false;
        app.dialogs.about.certificate_subject = None;
    }
}
