use super::*;

pub(super) fn ua_security_controls(
    ui: &mut egui::Ui,
    lang: Lang,
    dialog: &mut crate::app::ConnectDialog,
    security_changed: &mut bool,
) {
    // UA security controls: policy + mode combos, trust checkbox,
    // identity combo and session timeout.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::SecPolicyLabel)).weak());
        let before = dialog.ua_security_policy;
        egui::ComboBox::from_id_salt("ua_sec_policy")
            .selected_text(security_policy_label(lang, before))
            .width(160.0)
            .show_ui(ui, |ui| {
                for p in UaSecurityPolicy::ALL {
                    ui.selectable_value(
                        &mut dialog.ua_security_policy,
                        p,
                        security_policy_label(lang, p),
                    );
                }
            });
        if dialog.ua_security_policy != before {
            *security_changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::SecModeLabel)).weak());
        let before = dialog.ua_security_mode;
        egui::ComboBox::from_id_salt("ua_sec_mode")
            .selected_text(security_mode_label(lang, before))
            .width(160.0)
            .show_ui(ui, |ui| {
                for m in UaSecurityMode::ALL {
                    ui.selectable_value(
                        &mut dialog.ua_security_mode,
                        m,
                        security_mode_label(lang, m),
                    );
                }
            });
        if dialog.ua_security_mode != before {
            *security_changed = true;
        }
    });
    ui.checkbox(
        &mut dialog.ua_trust_server_cert,
        tr(lang, Tr::TrustServerCertCheck),
    )
    .on_hover_text(tr(lang, Tr::TrustServerCertTip));
    // Discovery does not expose user-token policies yet.
    // Do not silently replace X509 with UserName based on
    // the endpoint's message security mode. The worker
    // validates the requested identity at Connect time.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::IdentityLabel)).weak());
        let mut identity = dialog.ua_identity;
        egui::ComboBox::from_id_salt("ua_identity")
            .selected_text(match identity {
                UaIdentity::Anonymous => tr(lang, Tr::IdentityAnonymous),
                UaIdentity::UserName => tr(lang, Tr::IdentityUserName),
                UaIdentity::Certificate => tr(lang, Tr::IdentityCertificate),
            })
            .width(160.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut identity,
                    UaIdentity::Anonymous,
                    tr(lang, Tr::IdentityAnonymous),
                );
                ui.selectable_value(
                    &mut identity,
                    UaIdentity::UserName,
                    tr(lang, Tr::IdentityUserName),
                );
                ui.selectable_value(
                    &mut identity,
                    UaIdentity::Certificate,
                    tr(lang, Tr::IdentityCertificate),
                );
            });
        dialog.ua_identity = identity;
    });
    if dialog.ua_identity == UaIdentity::UserName {
        user_password_ui(ui, lang, &mut dialog.ua_user, &mut dialog.ua_password);
    }
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::SessionTimeoutLabel)).weak());
        ui.add(
            egui::DragValue::new(&mut dialog.ua_session_timeout)
                .range(SESSION_TIMEOUT_MIN_MS..=SESSION_TIMEOUT_MAX_MS),
        );
    });
    ui.label(egui::RichText::new(tr(lang, Tr::SessionTimeoutInfo)).weak());
}
