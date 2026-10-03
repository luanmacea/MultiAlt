// Clientes do Roblox abertos fora do app (pelo site): a varredura periódica que
// os reconhece pelo log e os comandos da seção "Em jogo" para os que não deu
// para reconhecer (mostrar a janela, identificar à mão).
//
// O reconhecimento em si mora em `platform/windows/external_clients.rs`. Nada
// aqui fecha cliente: adotar só registra o PID no `ProcessTracker`, igual a um
// launch do app. Ver docs/features/external-clients.md.

/// Intervalo da varredura. Sem cliente de fora, cada passada custa só a lista
/// de processos.
#[cfg(target_os = "windows")]
const EXTERNAL_CLIENT_SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Conta → browser tracker id salvo, que é o que o tracker guarda por conta.
#[cfg(target_os = "windows")]
fn saved_account_trackers(accounts: &AccountStore) -> HashMap<i64, String> {
    accounts
        .get_all()
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.user_id, a.browser_tracker_id.trim().to_string()))
        .collect()
}

/// Liga a varredura em segundo plano. A primeira passada é imediata: é ela que
/// traz de volta, depois de reiniciar o app, os clientes que já estavam abertos.
#[cfg(target_os = "windows")]
pub(crate) fn start_external_client_scanner(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            let adopted = tokio::task::spawn_blocking(move || {
                let accounts = saved_account_trackers(handle.state::<AccountStore>().inner());
                platform::windows::scan_external_clients(&accounts)
            })
            .await
            .unwrap_or_default();

            for (pid, user_id) in adopted {
                emit_launch_log(
                    &app,
                    user_id,
                    "info",
                    "external",
                    format!("Cliente aberto fora do app reconhecido pelo log do Roblox (PID {pid})"),
                );
            }
            tokio::time::sleep(EXTERNAL_CLIENT_SCAN_INTERVAL).await;
        }
    });
}

/// O que identificar à mão vai fazer com o tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExternalIdentifyPlan {
    /// Contas que hoje apontam para este PID (identificação anterior errada).
    untrack: Vec<i64>,
    browser_tracker_id: String,
}

/// Valida a identificação manual. `tracked`: (conta, PID) que o app acompanha.
fn plan_external_identification(
    alive_pids: &[u32],
    accounts: &HashMap<i64, String>,
    tracked: &[(i64, u32)],
    pid: u32,
    user_id: i64,
) -> Result<ExternalIdentifyPlan, String> {
    if !alive_pids.contains(&pid) {
        return Err("That Roblox client is no longer running.".into());
    }
    let Some(browser_tracker_id) = accounts.get(&user_id) else {
        return Err("That account is not in your account list.".into());
    };
    let mut untrack: Vec<i64> = tracked
        .iter()
        .filter(|(uid, tracked_pid)| *tracked_pid == pid && *uid != user_id)
        .map(|(uid, _)| *uid)
        .collect();
    untrack.sort_unstable();
    Ok(ExternalIdentifyPlan {
        untrack,
        browser_tracker_id: browser_tracker_id.clone(),
    })
}

/// Clientes do Roblox abertos fora do app que não deu para reconhecer.
#[tauri::command]
fn get_unidentified_clients() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        return serde_json::to_value(platform::windows::unidentified_clients())
            .map_err(|e| e.to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(serde_json::json!([]))
    }
}

/// Diz ao app de quem é um cliente aberto fora dele. Se a conta já tinha outro
/// cliente registrado, este passa a ser o dela (o outro vira "não
/// identificado" na próxima varredura). Não fecha nada.
#[tauri::command]
fn identify_external_client(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    pid: u32,
    user_id: i64,
) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let tracker = platform::windows::tracker();
        let tracked: Vec<(i64, u32)> = tracker
            .get_all()
            .into_iter()
            .map(|p| (p.user_id, p.pid))
            .collect();
        let accounts = saved_account_trackers(state.inner());
        let plan = plan_external_identification(
            &platform::windows::get_roblox_pids(),
            &accounts,
            &tracked,
            pid,
            user_id,
        )?;
        for uid in plan.untrack {
            tracker.untrack(uid);
        }
        tracker.track_adopted(user_id, pid, plan.browser_tracker_id);
        platform::windows::forget_unidentified_client(pid);
        emit_launch_log(
            &app,
            user_id,
            "info",
            "external",
            format!("Cliente aberto fora do app identificado à mão (PID {pid})"),
        );
        return Ok(true);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, state, pid, user_id);
        Err("Not supported on this platform".into())
    }
}

/// Traz para a frente a janela de um cliente pelo PID — só se o PID ainda for
/// um `RobloxPlayerBeta` (PID reaproveitado nunca é focado).
#[tauri::command]
fn focus_client_window(pid: u32) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        if !platform::windows::get_roblox_pids().contains(&pid) {
            return Ok(false);
        }
        let Some(hwnd) = platform::windows::find_main_window(pid) else {
            return Ok(false);
        };
        return Ok(platform::windows::focus_window(hwnd));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
        Ok(false)
    }
}

#[cfg(test)]
mod external_client_command_tests {
    use super::*;

    fn accounts() -> HashMap<i64, String> {
        HashMap::from([(10, "bt-10".to_string()), (20, "bt-20".to_string())])
    }

    #[test]
    fn identifying_a_client_that_closed_is_refused() {
        let err = plan_external_identification(&[1, 2], &accounts(), &[], 99, 10).unwrap_err();
        assert!(err.contains("no longer running"), "{err}");
    }

    #[test]
    fn identifying_with_an_account_that_is_not_saved_is_refused() {
        let err = plan_external_identification(&[99], &accounts(), &[], 99, 30).unwrap_err();
        assert!(err.contains("not in your account list"), "{err}");
    }

    #[test]
    fn identifying_carries_the_saved_browser_tracker_id() {
        let plan = plan_external_identification(&[99], &accounts(), &[], 99, 20).unwrap();
        assert_eq!(plan.browser_tracker_id, "bt-20");
        assert!(plan.untrack.is_empty());
    }

    #[test]
    fn re_identifying_a_pid_drops_the_account_it_was_wrongly_given() {
        // O PID 99 estava com a conta 10; a pessoa diz que é da 20. A conta 30
        // (outro PID) não é tocada.
        let plan = plan_external_identification(
            &[99, 50],
            &accounts(),
            &[(10, 99), (30, 50)],
            99,
            20,
        )
        .unwrap();
        assert_eq!(plan.untrack, vec![10]);
    }

    #[test]
    fn identifying_again_with_the_same_account_untracks_nothing() {
        let plan = plan_external_identification(&[99], &accounts(), &[(10, 99)], 99, 10).unwrap();
        assert!(plan.untrack.is_empty());
    }

    #[test]
    fn focusing_a_pid_that_is_not_a_roblox_client_does_nothing() {
        // PID perto de u32::MAX: nunca é um cliente de verdade.
        assert_eq!(focus_client_window(4_294_967_290).unwrap(), false);
    }

    #[test]
    fn the_unidentified_list_is_always_a_json_array() {
        assert!(get_unidentified_clients().unwrap().is_array());
    }
}
