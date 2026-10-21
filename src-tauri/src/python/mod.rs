// Automação do Roblox Player em Python (docs/features/python-automation.md).
//
// O Rust guarda o Python portátil, a pasta de trabalho e as *primitivas* (foco,
// tecla, clique, captura), e é nelas que moram as travas de segurança. Quem
// decide *como* trocar de janela e chamar o `main` do usuário é Python, nos
// blocos padrão editáveis. Os comandos da tela ficam em `commands`, sempre
// registrados; sem a feature `python-scripts` eles só dizem que a build não
// tem a funcionalidade.

#[cfg(feature = "python-scripts")]
pub mod bridge;
pub mod commands;
#[cfg(feature = "python-dev-pipe")]
pub mod devpipe;
#[cfg(feature = "python-scripts")]
pub mod dashboard;
pub mod errors;
#[cfg(feature = "python-scripts")]
pub mod focus;
#[cfg(feature = "python-scripts")]
pub mod inspector;
#[cfg(feature = "python-scripts")]
pub mod primitives;
#[cfg(feature = "python-scripts")]
pub mod runs;
#[cfg(feature = "python-scripts")]
pub mod service;
#[cfg(feature = "python-scripts")]
pub mod runtime;
pub mod types;
#[cfg(feature = "python-scripts")]
pub mod workspace;

/// Para todas as execuções Python (o chacoalhar do mouse, `Afk.ShakeToStop`).
/// Devolve quantas receberam o parar.
pub fn emergency_stop() -> usize {
    #[cfg(feature = "python-scripts")]
    if let Some(service) = service::existing() {
        return service.manager.stop_all();
    }
    0
}

/// Quantas execuções Python estão vivas (rodando ou esperando o intervalo).
pub fn runs_active() -> usize {
    #[cfg(feature = "python-scripts")]
    if let Some(service) = service::existing() {
        return service.manager.active();
    }
    0
}

#[cfg(all(feature = "python-dev-pipe", target_os = "windows"))]
static DEV_PIPE: std::sync::Mutex<Option<devpipe::PipeServer>> = std::sync::Mutex::new(None);

/// Abre ou fecha o canal do terminal conforme o Modo desenvolvedor. Sem a
/// feature `python-dev-pipe` o modo ainda libera a edição dos blocos, e o
/// canal avisa que não existe nesta build. Precisa rodar dentro do runtime
/// assíncrono (o servidor do pipe é uma task).
#[cfg(feature = "python-scripts")]
pub fn apply_developer_mode(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(all(feature = "python-dev-pipe", target_os = "windows"))]
    {
        let mut slot = DEV_PIPE.lock().map_err(|e| e.to_string())?;
        if !enabled {
            if slot.take().is_some() {
                service::system_log(app, None, "Developer mode off: the terminal channel is closed.");
            }
            return Ok(());
        }
        if slot.is_some() {
            return Ok(());
        }
        let root = crate::data::settings::get_python_workspace_dir();
        workspace::sync_workspace(&root)?;
        let sid = devpipe::current_user_sid()?;
        let name = devpipe::pipe_name_for_sid(&sid);
        let resolver_app = app.clone();
        let resolve: devpipe::Resolver =
            std::sync::Arc::new(move |names: &[String]| service::account_views_by_name(&resolver_app, names));
        let log_app = app.clone();
        let on_error = std::sync::Arc::new(move |message: String| service::system_log(&log_app, None, message));
        // O vigia do chacoalhar só fica de pé com execução viva: liga a cada
        // execução do terminal, como o Run da tela faz.
        let shake_app = app.clone();
        let on_started: devpipe::OnStarted = std::sync::Arc::new(move || crate::ensure_shake_watch(&shake_app));
        let server = devpipe::PipeServer::start(
            name,
            &sid,
            &root,
            resolve,
            service::service(app).manager.clone(),
            on_error,
            on_started,
        )?;
        service::system_log(
            app,
            None,
            "Developer mode on: the terminal channel is open (python -m multialt run ...).",
        );
        *slot = Some(server);
        return Ok(());
    }
    #[cfg(not(all(feature = "python-dev-pipe", target_os = "windows")))]
    {
        if enabled {
            service::system_log(
                app,
                None,
                "Developer mode on: default blocks can be edited. The terminal channel is not available in this build.",
            );
        }
        Ok(())
    }
}

/// No boot do app: com o Modo desenvolvedor ligado, reabre o canal.
#[cfg(feature = "python-scripts")]
pub fn on_app_start(app: &tauri::AppHandle) {
    use tauri::Manager;
    let enabled = app
        .state::<crate::data::settings::SettingsStore>()
        .get_bool("Python", "DeveloperMode");
    if !enabled {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = apply_developer_mode(&app, true) {
            eprintln!("Python developer mode: {e}");
        }
    });
}
