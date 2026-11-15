/// O servidor HTTP local lança sem contexto de conta aqui: aplica o perfil
/// global.
///
/// `client_dir`: a pasta de onde o cliente vai abrir (`windows::client_dir`).
/// `None` quando nem ela foi achada — aí o patch tenta a build padrão.
fn patch_client_settings_for_launch(settings: &SettingsStore, client_dir: Option<&str>) {
    crate::patch_client_settings_for_launch(
        settings,
        crate::LaunchClientProfile::Normal,
        client_dir,
    );
}
