// Launch de UMA conta no Linux (Sober). Incluído por `lib.rs` só no Linux e
// chamado pelos dois caminhos que abrem conta: `launch_roblox_other` (conta
// avulsa) e `run_launch_queue_unix` (fila). A decisão — perfil, cookie, deep
// link, argv — mora em `platform/sober.rs`; a execução, em `platform/linux/`.
// Aqui fica a cola com o resto do app: sessão, servidor privado, sorteio de
// servidor, fila e as linhas do Console (todas por `log_msg!`).
//
// Regra nº 1: etapa opcional que falha vira aviso e o launch segue. Só param:
// conta banida (quem chama), sessão inválida, link privado que não resolve e
// o que falta para abrir de verdade (Flatpak, Sober, `unshare`, `pasta`).

use platform::sober::{self, Runner, RunnerSettings, SetupIssue, SoberWarning};

/// As quatro chaves `[Linux]` do INI.
fn linux_runner_settings(settings: &SettingsStore) -> RunnerSettings {
    RunnerSettings {
        preferred_runner: settings.get_string("Linux", "PreferredRunner"),
        custom_launch_command: settings.get_string("Linux", "CustomLaunchCommand"),
        custom_process_match: settings.get_string("Linux", "CustomProcessMatch"),
        custom_log_dir: settings.get_string("Linux", "CustomLogDir"),
    }
}

/// O que falta instalar, na frase traduzida do Console.
fn linux_setup_issue_msg(issue: SetupIssue) -> LogMsg {
    match issue {
        SetupIssue::FlatpakMissing => log_msg!(
            "Flatpak is not installed. Install the flatpak package from your distribution and try again."
        ),
        SetupIssue::SoberMissing => log_msg!(
            "Sober is not installed. Run: {{command}}",
            command = format!("flatpak install flathub {}", sober::SOBER_APP_ID)
        ),
        SetupIssue::UnshareMissing => log_msg!(
            "unshare was not found. Install the util-linux package; it opens each account in its own network."
        ),
        SetupIssue::PastaMissing => log_msg!(
            "pasta was not found. Install the passt package; it gives each account its own network so several can run at once."
        ),
    }
}

/// Aviso do launch, na frase traduzida do Console.
fn linux_warning_msg(warning: &SoberWarning) -> LogMsg {
    match warning {
        SoberWarning::CustomCommandEmpty => log_msg!(
            "PreferredRunner is custom but CustomLaunchCommand is empty; using Sober"
        ),
        SoberWarning::CustomCommandInvalid(error) => log_msg!(
            "CustomLaunchCommand is invalid ({{error}}); using Sober",
            error = error.as_str()
        ),
        SoberWarning::UnknownRunner(runner) => log_msg!(
            "Runner {{runner}} is not supported on Linux yet; using Sober",
            runner = runner.as_str()
        ),
        SoberWarning::PrivateServerWithoutCode => log_msg!(
            "Private server without a link code; opening the experience instead"
        ),
        SoberWarning::NetworkIsolationUnavailable => log_msg!(
            "Opening this account without its own network. A second account at the same time needs the passt package."
        ),
        SoberWarning::SkippedSymlinks(count) => log_msg!(
            "{{count}} symbolic link(s) in the main Sober folder were not copied",
            count = *count as u64
        ),
    }
}

/// Registra a falha no Console e na fila, e devolve o texto para o erro do comando.
fn linux_launch_failure(
    app: &tauri::AppHandle,
    sequence: &LaunchSequenceGuard,
    user_id: i64,
    step: &str,
    message: LogMsg,
) -> String {
    emit_launch_log(app, user_id, "error", step, message.clone());
    let text = message.render();
    sequence.mark(user_id, LaunchQueueState::Failed, Some(text.clone()));
    text
}

/// Abre a conta no Sober e a deixa no rastreador. `Ok(pid)` = o cliente subiu
/// (a fila já foi marcada `Done`); `Err` = já registrado no Console e na fila.
#[allow(clippy::too_many_arguments)]
async fn launch_account_linux(
    app: &tauri::AppHandle,
    sequence: &LaunchSequenceGuard,
    state: &AccountStore,
    settings: &SettingsStore,
    user_id: i64,
    request: &LaunchRequest,
    mut resolved: ResolvedLaunchJob,
    auto_close_last_process: bool,
) -> Result<u32, String> {
    let tracker = platform::linux::tracker();
    let follow_user = request.follow_user || follow_target(request).is_some();
    if follow_user {
        // O Sober não entra seguindo um usuário: abre o jogo dele (regra nº 1).
        resolved.join_vip = false;
        resolved.link_code.clear();
        emit_launch_log(
            app,
            user_id,
            "warn",
            "follow",
            log_msg!("Following a user is not supported with Sober; opening the game instead"),
        );
    }

    emit_launch_log(
        app,
        user_id,
        "info",
        "start",
        log_msg!(
            "Starting launch — place {{placeId}} ({{target}})",
            placeId = request.place_id,
            target = launch_target_description(resolved.join_vip, &resolved.link_code, &resolved.job_id)
        ),
    );

    // O que falta para abrir de verdade, dito com o que instalar. Só vale para o
    // Sober: o runner `custom` traz o próprio programa.
    let runner_settings = linux_runner_settings(settings);
    if matches!(sober::plan_runner(&runner_settings).runner, Runner::Sober) {
        // Só o que impede de abrir QUALQUER conta bloqueia aqui; sem `pasta`/
        // `unshare` a primeira conta ainda abre (decide `launch_account`).
        if let Some(issue) = platform::linux::diagnose()
            .issues()
            .into_iter()
            .find(|i| matches!(i, SetupIssue::FlatpakMissing | SetupIssue::SoberMissing))
        {
            return Err(linux_launch_failure(
                app,
                sequence,
                user_id,
                "setup",
                linux_setup_issue_msg(issue),
            ));
        }
    }

    // Sessão válida (renova o cookie guardado se precisar) antes de pôr o
    // cookie no perfil.
    if let Err(err) = run_with_session_retry(state, user_id, |cookie| async move {
        api::auth::get_auth_ticket(&cookie).await
    })
    .await
    {
        mark_account_session_dead(state, app, user_id, &err);
        sequence.mark(user_id, LaunchQueueState::Failed, Some(err.clone()));
        return Err(err);
    }
    let cookie = get_cookie(state, user_id)?;

    let private_join = match run_with_session_retry(state, user_id, |cookie| {
        let resolved = resolved.clone();
        let place_id = request.place_id;
        async move { resolve_private_join(&cookie, place_id, &resolved).await }
    })
    .await
    {
        Ok(value) => value,
        Err(err) => {
            sequence.mark(user_id, LaunchQueueState::Failed, Some(err.clone()));
            return Err(err);
        }
    };

    // Sorteio de servidor público (ver o caminho Windows).
    let mut job_id = resolved.job_id.clone();
    if should_shuffle_server(request.shuffle_job, follow_user, &job_id) {
        if let Some(job) = pick_shuffled_public_job(state, user_id, request.place_id).await {
            job_id = job;
        }
    }

    let browser_tracker_id = get_or_create_browser_tracker_id(state, user_id)?;

    if auto_close_last_process && tracker.get_pid(user_id).is_some() {
        let _ = tokio::task::spawn_blocking(move || platform::linux::tracker().kill_for_user(user_id))
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    if tracker.is_launch_cancelled() {
        sequence.mark(user_id, LaunchQueueState::Cancelled, None);
        return Err("Launch cancelled".to_string());
    }

    let data_dir = data::settings::get_runtime_data_dir();
    let place_id = private_join.place_id;
    let join_vip = private_join.use_private_join;
    let link_code = private_join.link_code.clone();
    let launch_data = request.launch_data.clone();
    let launched = tokio::task::spawn_blocking(move || {
        platform::linux::launch_account(&platform::linux::LaunchRequest {
            data_dir: &data_dir,
            user_id,
            cookie: &cookie,
            place_id,
            job_id: &job_id,
            join_vip,
            link_code: &link_code,
            launch_data: &launch_data,
            runner: &runner_settings,
            browser_tracker_id: &browser_tracker_id,
        })
    })
    .await
    .map_err(|e| platform::linux::LaunchFailure::Other(e.to_string()))
    .and_then(|result| result);

    match launched {
        Ok(client) => {
            for warning in &client.warnings {
                emit_launch_log(app, user_id, "warn", "launch", linux_warning_msg(warning));
            }
            emit_launch_log(
                app,
                user_id,
                "success",
                "pid",
                log_msg!("Client started (PID {{pid}})", pid = client.pid),
            );
            sequence.mark(user_id, LaunchQueueState::Done, None);
            Ok(client.pid)
        }
        Err(platform::linux::LaunchFailure::Network(issue)) => Err(linux_launch_failure(
            app,
            sequence,
            user_id,
            "setup",
            linux_setup_issue_msg(issue),
        )),
        Err(platform::linux::LaunchFailure::Other(err)) => Err(linux_launch_failure(
            app,
            sequence,
            user_id,
            "launch",
            log_msg!("Could not open the client: {{error}}", error = err),
        )),
    }
}
