/// O servidor HTTP local lança sem contexto de conta aqui, então aplica só o
/// perfil global. As exceções por conta valem no launch pela UI e no Botting.
fn patch_client_settings_for_launch(settings: &SettingsStore) {
    // Sem conta no contexto, `base_path` fica `None` — a build padrão/produção.
    crate::patch_client_settings_for_launch(
        settings,
        crate::LaunchClientProfile::Normal,
        None,
        None,
    );
}
