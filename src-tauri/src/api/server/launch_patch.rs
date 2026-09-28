/// O servidor HTTP local lança sem contexto de conta aqui, então aplica só o
/// perfil global. As exceções por conta valem no launch pela UI e no Botting.
///
/// `client_dir`: a pasta de onde o cliente vai abrir (`windows::client_dir`).
/// `None` quando nem ela foi achada — aí o patch tenta a build padrão.
fn patch_client_settings_for_launch(settings: &SettingsStore, client_dir: Option<&str>) {
    crate::patch_client_settings_for_launch(
        settings,
        crate::LaunchClientProfile::Normal,
        None,
        client_dir,
    );
}
