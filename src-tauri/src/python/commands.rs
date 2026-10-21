// Comandos da tela Python (`python_*`). Sempre registrados no
// `generate_handler!`; sem a feature `python-scripts` os de ação respondem
// `NOT_AVAILABLE` e os de leitura devolvem vazio, para a tela não quebrar.

#![allow(unused_variables)]

use std::collections::HashMap;

use super::types::*;

pub const NOT_AVAILABLE: &str = "Python automation is not available in this build";

#[cfg(feature = "python-scripts")]
const UNAVAILABLE: &str = "not implemented yet";
#[cfg(not(feature = "python-scripts"))]
const UNAVAILABLE: &str = NOT_AVAILABLE;

#[allow(unused_macros)]
macro_rules! unavailable {
    () => {
        Err(UNAVAILABLE.to_string())
    };
}

#[tauri::command]
pub async fn python_runtime_status(app: tauri::AppHandle) -> Result<PythonRuntimeStatus, String> {
    #[cfg(feature = "python-scripts")]
    return Ok(super::runtime::status(&app));
    #[cfg(not(feature = "python-scripts"))]
    Ok(PythonRuntimeStatus::default())
}

/// Baixa e instala o Python portátil. Só roda pelo clique em "Install Python";
/// uma segunda chamada com a primeira em andamento é recusada.
#[tauri::command]
pub async fn python_install_runtime(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return install_runtime(app).await;
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// Reinstala: o runtime novo substitui o atual só no fim (os scripts nunca são
/// tocados — moram na pasta de trabalho).
#[tauri::command]
pub async fn python_reinstall_runtime(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return install_runtime(app).await;
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[cfg(feature = "python-scripts")]
async fn install_runtime(app: tauri::AppHandle) -> Result<(), String> {
    use super::runtime;
    use tauri::Emitter;
    let Some(_guard) = runtime::begin_install() else {
        return Err("Python is already being installed".into());
    };
    // Depois de marcar "instalando" (o Run recusa a partir daqui): nenhuma
    // execução nasce entre a conferência e a troca.
    runtime::install_allowed(super::runs_active())?;
    let base = runtime::runtime_dir(&app);
    let workspace = crate::data::settings::get_python_workspace_dir();
    let emitter = app.clone();
    let mut on_progress = move |p: PythonInstallProgress| {
        // Os passos também vão para o terminal da tela (linhas `system`).
        if let Some(line) = runtime::install_log_line(&p) {
            super::service::system_log(&emitter, None, line);
        }
        let _ = emitter.emit("python-install-progress", p);
    };
    // O pip roda bloqueando (minutos): a instalação inteira vai para uma
    // thread de bloqueio.
    let result = tokio::task::spawn_blocking(move || {
        let spec = runtime::RuntimeSpec::official();
        tauri::async_runtime::block_on(runtime::install_into(
            &base,
            &workspace,
            &spec,
            &runtime::RealStepRunner,
            &mut on_progress,
        ))
    })
    .await
    .unwrap_or_else(|e| Err(format!("The install stopped unexpectedly: {e}")));
    if let Err(error) = &result {
        super::service::system_log(&app, None, format!("Python install failed: {error}"));
        let _ = app.emit(
            "python-install-progress",
            PythonInstallProgress {
                stage: "error".into(),
                message: error.clone(),
                downloaded: 0,
                total: 0,
            },
        );
    }
    result
}

#[cfg(feature = "python-scripts")]
fn ws_root() -> std::path::PathBuf {
    crate::data::settings::get_python_workspace_dir()
}

#[cfg(feature = "python-scripts")]
fn developer_mode_on(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.state::<crate::data::settings::SettingsStore>()
        .get_bool("Python", "DeveloperMode")
}

/// O comando que a tela mostra para rodar um script pelo terminal.
#[cfg(feature = "python-scripts")]
pub fn cli_command(python_exe: Option<&std::path::Path>) -> String {
    let exe = python_exe
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "python".into());
    format!("\"{exe}\" -m multialt run scripts/farm.py --account Main_01 --mode one_at_a_time")
}

#[tauri::command]
pub async fn python_workspace_info(app: tauri::AppHandle) -> Result<PythonWorkspaceInfo, String> {
    #[cfg(feature = "python-scripts")]
    {
        let root = ws_root();
        super::workspace::sync_workspace(&root)?;
        let exe = super::runtime::python_exe(&app);
        return Ok(PythonWorkspaceInfo {
            root: root.to_string_lossy().to_string(),
            scripts_dir: root.join(super::workspace::SCRIPTS_DIR).to_string_lossy().to_string(),
            assets_dir: root.join(super::workspace::ASSETS_DIR).to_string_lossy().to_string(),
            python_exe: exe.as_ref().map(|p| p.to_string_lossy().to_string()),
            cli_command: cli_command(exe.as_deref()),
        });
    }
    #[cfg(not(feature = "python-scripts"))]
    Ok(PythonWorkspaceInfo::default())
}

#[tauri::command]
pub fn python_list_scripts() -> Result<Vec<PythonScriptFile>, String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::list_scripts(&ws_root());
    #[cfg(not(feature = "python-scripts"))]
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_read_script(name: String) -> Result<String, String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::read_script(&ws_root(), &name);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_write_script(name: String, source: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::write_script(&ws_root(), &name, &source);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// `template`: "blank" | "per_account" | "controller" | "dashboard".
#[tauri::command]
pub fn python_create_script(name: String, template: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        let root = ws_root();
        super::workspace::sync_workspace(&root)?;
        return super::workspace::create_script(&root, &name, &template);
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_delete_script(name: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::delete_script(&ws_root(), &name);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_list_default_blocks() -> Result<Vec<DefaultBlockInfo>, String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::list_blocks(&ws_root());
    #[cfg(not(feature = "python-scripts"))]
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_read_default_block(name: String) -> Result<String, String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::read_block(&ws_root(), &name);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// Só com o Modo desenvolvedor ligado.
#[tauri::command]
pub fn python_write_default_block(
    app: tauri::AppHandle,
    name: String,
    source: String,
) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        if !developer_mode_on(&app) {
            return Err(super::workspace::DEVELOPER_MODE_OFF.into());
        }
        return super::workspace::write_block(&ws_root(), &name, &source);
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// Restaura **um** bloco (a tela confirma antes).
#[tauri::command]
pub fn python_restore_default_block(name: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::restore_block(&ws_root(), &name);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_open_workspace_folder() -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        let root = ws_root();
        super::workspace::sync_workspace(&root)?;
        return super::workspace::open_folder(&root);
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_open_in_editor(name: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        let path = super::workspace::script_path(&ws_root(), &name)?;
        if !path.is_file() {
            return Err(format!("{name} does not exist"));
        }
        return super::workspace::open_in_editor(&path);
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_get_run_configs() -> Result<HashMap<String, PythonRunConfig>, String> {
    #[cfg(feature = "python-scripts")]
    return Ok(super::workspace::load_run_configs(&ws_root()));
    #[cfg(not(feature = "python-scripts"))]
    Ok(HashMap::new())
}

#[tauri::command]
pub fn python_save_run_config(config: PythonRunConfig) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return super::workspace::save_run_config(&ws_root(), &config);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// Começa uma execução pelo app. Devolve o id.
#[tauri::command]
pub async fn python_start_run(app: tauri::AppHandle, config: PythonRunConfig) -> Result<String, String> {
    #[cfg(feature = "python-scripts")]
    return start_app_run(&app, config);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[cfg(feature = "python-scripts")]
fn start_app_run(app: &tauri::AppHandle, config: PythonRunConfig) -> Result<String, String> {
    use super::{runs, service, workspace};
    workspace::validate_script_name(&config.script)?;
    if config.accounts.is_empty() {
        return Err("Pick at least one account".into());
    }
    if let PythonTrigger::Interval { minutes } = config.trigger {
        if minutes == 0 {
            return Err("The interval must be at least 1 minute".into());
        }
    }
    if super::runtime::status(app).installing {
        return Err("Python is being installed. Wait for it to finish, then press Run.".into());
    }
    let python_exe = super::runtime::python_exe(app)
        .ok_or("Python is not installed yet. Click \"Install Python\" first.")?;
    let root = ws_root();
    service::prepare_workspace(app, &root)?;
    if !workspace::script_path(&root, &config.script)?.is_file() {
        return Err(format!("{} does not exist in scripts/", config.script));
    }
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<i64> = config.accounts.iter().copied().filter(|id| seen.insert(*id)).collect();
    let accounts = service::account_views(app, &ids)?;
    let svc = service::service(app);
    let executor = std::sync::Arc::new(runs::ProcessExecutor {
        python_exe,
        workspace: root,
    });
    let id = svc.manager.start(config, accounts, "app", executor)?;
    // Chacoalhar o mouse para as execuções (`Afk.ShakeToStop`).
    #[cfg(target_os = "windows")]
    crate::ensure_shake_watch(app);
    Ok(id)
}

#[tauri::command]
pub async fn python_stop_run(app: tauri::AppHandle, run_id: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    return super::service::service(&app).manager.stop(&run_id);
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub async fn python_stop_all_runs(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        super::service::service(&app).manager.stop_all();
        return Ok(());
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_list_runs(app: tauri::AppHandle) -> Result<Vec<PythonRunInfo>, String> {
    #[cfg(feature = "python-scripts")]
    return Ok(super::service::service(&app).manager.list());
    #[cfg(not(feature = "python-scripts"))]
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_get_dashboard(app: tauri::AppHandle) -> Result<Vec<DashboardSection>, String> {
    #[cfg(feature = "python-scripts")]
    return Ok(super::service::service(&app).dashboard.sections());
    #[cfg(not(feature = "python-scripts"))]
    Ok(Vec::new())
}

#[tauri::command]
pub fn python_clear_dashboard(app: tauri::AppHandle, script: String) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        let svc = super::service::service(&app);
        svc.dashboard.clear_section(&script);
        let _ = svc.dashboard.save_if_dirty();
        return Ok(());
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// A captura da janela de uma conta (só cliente aberto pelo MultiAlt), para o
/// inspetor. Imagem grande sai reduzida (as frações não mudam).
#[tauri::command]
pub async fn python_inspector_capture(user_id: i64) -> Result<InspectorFrame, String> {
    #[cfg(feature = "python-scripts")]
    return tokio::task::spawn_blocking(move || super::inspector::capture_frame(user_id))
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// Salva o recorte `rect` (frações) do quadro `frame_id` que o inspetor
/// mostrou (o último da conta) em `assets/<name>`. Devolve o caminho salvo.
#[tauri::command]
pub async fn python_save_asset(user_id: i64, rect: FracRect, name: String, frame_id: u64) -> Result<String, String> {
    #[cfg(feature = "python-scripts")]
    return tokio::task::spawn_blocking(move || {
        let root = ws_root();
        let path = super::inspector::asset_path(&root, &name)?;
        super::inspector::frames().save(user_id, frame_id, &rect, &path)?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

/// Liga/desliga o Modo desenvolvedor: grava `Python.DeveloperMode` e abre ou
/// fecha o canal do terminal.
#[tauri::command]
pub async fn python_set_developer_mode(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(feature = "python-scripts")]
    {
        use tauri::Manager;
        app.state::<crate::data::settings::SettingsStore>()
            .set("Python", "DeveloperMode", if enabled { "true" } else { "false" })?;
        return super::apply_developer_mode(&app, enabled);
    }
    #[cfg(not(feature = "python-scripts"))]
    unavailable!()
}

#[tauri::command]
pub fn python_get_logs(app: tauri::AppHandle) -> Result<Vec<PythonLogLine>, String> {
    #[cfg(feature = "python-scripts")]
    return Ok(super::service::service(&app).logs.all());
    #[cfg(not(feature = "python-scripts"))]
    Ok(Vec::new())
}
