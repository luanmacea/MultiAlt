//! Sessão de criação de contas em série.
//!
//! Abre **uma** janela do Chromium na página de cadastro e repete, para cada
//! conta pedida:
//!
//! 1. limpa a sessão anterior (cookies do Roblox) e recarrega o formulário;
//! 2. gera a identidade e preenche tudo;
//! 3. **espera o usuário resolver o CAPTCHA e clicar em "Criar conta"**;
//! 4. quando o `.ROBLOSECURITY` aparece, a conta entrou: valida o cookie e
//!    guarda a conta com a senha gerada.
//!
//! O app nunca envia o formulário nem tenta passar pela verificação do Roblox —
//! o que ele economiza é a digitação.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::data::accounts::{Account, AccountStore};
use crate::data::settings::SettingsStore;

use super::cdp::{spawn_chrome, CdpClient};
use super::download::ensure_chromium;
use super::manager::{ChromiumManager, LOGIN_KEY};
use super::signup::{
    generate_identity, marked_field_selector, missing_fields, signup_prepare_script, SeededRng,
    SignupIdentity, ROBLOX_SIGNUP_URL, SIGNUP_FORM_SELECTOR,
};

/// Quanto tempo esperamos o usuário concluir **um** cadastro (CAPTCHA
/// incluído) antes de desistir daquela conta.
const CAPTCHA_WAIT: Duration = Duration::from_secs(300);
/// Intervalo entre as checagens do cookie.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// A cada quantas checagens o formulário é reparado (≈2 s com o intervalo
/// atual). Rápido o bastante para a segunda tela não ficar vazia, devagar o
/// bastante para não atrapalhar quem está digitando.
const REPAIR_EVERY_TICKS: u32 = 4;

/// Teto de contas por sessão, para um clique errado não virar uma maratona.
pub const MAX_ACCOUNTS_PER_SESSION: usize = 50;

/// Estado publicado no evento `signup-progress` e por `get_signup_status`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignupStatus {
    pub active: bool,
    /// Conta atual (1-based) e total pedido.
    pub current: usize,
    pub total: usize,
    pub created: usize,
    /// `idle` | `opening` | `filling` | `waiting-user` | `saving` | `done` | `error`
    pub phase: String,
    /// Identidade da conta em preenchimento, para a UI mostrar usuário e senha.
    pub identity: Option<SignupIdentity>,
    pub last_error: Option<String>,
    /// Usuários já criados nesta sessão.
    pub created_usernames: Vec<String>,
}

impl Default for SignupStatus {
    fn default() -> Self {
        Self {
            active: false,
            current: 0,
            total: 0,
            created: 0,
            phase: "idle".to_string(),
            identity: None,
            last_error: None,
            created_usernames: Vec::new(),
        }
    }
}

struct SignupState {
    status: Mutex<SignupStatus>,
    stop: Arc<AtomicBool>,
}

fn state() -> &'static SignupState {
    static STATE: OnceLock<SignupState> = OnceLock::new();
    STATE.get_or_init(|| SignupState {
        status: Mutex::new(SignupStatus::default()),
        stop: Arc::new(AtomicBool::new(false)),
    })
}

fn current_status() -> SignupStatus {
    state()
        .status
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default()
}

/// Atualiza o estado e publica em `signup-progress` — a UI nunca lê o estado
/// por polling.
fn update<F: FnOnce(&mut SignupStatus)>(app: &AppHandle, edit: F) {
    let snapshot = {
        let Ok(mut status) = state().status.lock() else {
            return;
        };
        edit(&mut status);
        status.clone()
    };
    let _ = app.emit("signup-progress", snapshot);
}

/// Semente da identidade: relógio + índice, para duas contas do mesmo lote
/// nunca saírem iguais nem no mesmo milissegundo.
fn identity_seed(index: usize) -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0) as u64;
    nanos ^ ((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

fn current_year() -> i32 {
    // Ano aproximado a partir da época; precisão de dias não importa para a
    // idade sorteada (18–40 anos).
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    1970 + (secs / 31_556_952) as i32
}

/// Limpa a sessão do Roblox entre um cadastro e o próximo, para o formulário
/// voltar deslogado.
async fn reset_session(cdp: &mut CdpClient) {
    let _ = cdp.send("Network.clearBrowserCookies", serde_json::json!({})).await;
    let _ = cdp.delete_roblosecurity(".roblox.com").await;
    let _ = cdp.delete_roblosecurity("www.roblox.com").await;
}

/// Quantos nomes tentar antes de desistir e usar o último sorteado.
const USERNAME_ATTEMPTS: usize = 6;

/// Sorteia um nome de usuário que o Roblox aceite.
///
/// Descobrir que o nome está em uso só na hora do envio queima o CAPTCHA que a
/// pessoa acabou de resolver — e, com o formulário já preenchido, ela nem
/// conseguia aceitar uma sugestão. Então o nome é conferido **antes**, contra
/// `auth/v2/usernames/validate`.
///
/// Falha de rede não trava a sessão: segue com o nome sorteado e o formulário
/// valida como sempre validou.
///
/// O `prefix` viaja junto porque o re-sorteio tem que manter o padrão que o
/// usuário escolheu: sem ele, a segunda tentativa entregaria um nome de
/// palavras no meio de um lote "arvore_*".
async fn pick_free_username(identity: &mut SignupIdentity, seed: u64, prefix: &str) {
    let birthday = crate::api::roblox::signup_birthday_iso(
        &identity.day,
        &identity.month,
        &identity.year,
    );

    for attempt in 0..USERNAME_ATTEMPTS {
        match crate::api::roblox::check_signup_username(&identity.username, &birthday).await {
            Ok(check) if check.available => return,
            // Serviço fora do ar: não dá para saber, segue com o que tem.
            Err(_) => return,
            Ok(_) => {}
        }
        if attempt + 1 == USERNAME_ATTEMPTS {
            return;
        }
        let mut rng = SeededRng::new(seed ^ ((attempt as u64 + 1).wrapping_mul(0x9E37_79B9)));
        identity.username = super::signup::generate_username(&mut rng, prefix);
    }
}

/// Preenche o que falta no formulário que estiver na tela.
///
/// Roda no início **e em laço** enquanto se espera o usuário, porque:
///
/// - o React remonta campos depois da primeira renderização, e um valor
///   escrito cedo demais some (foi o nome de usuário chegando vazio);
/// - o Roblox serve duas versões do cadastro, e a segunda só pede a senha
///   depois do "Continue" — sem o reparo, o usuário teria que copiar a senha da
///   janela do app e colar na mão.
///
/// Devolve os campos que ainda ficaram faltando.
async fn fill_signup_form(cdp: &mut CdpClient, identity: &SignupIdentity) -> Vec<String> {
    let Ok(result) = cdp.eval(&signup_prepare_script(identity)).await else {
        return Vec::new();
    };

    let mut still_missing = Vec::new();
    for field in missing_fields(&result) {
        let value = match field.as_str() {
            "username" => &identity.username,
            "password" => &identity.password,
            _ => continue,
        };
        match cdp.type_into(&marked_field_selector(&field), value).await {
            Ok(true) => {}
            _ => still_missing.push(field),
        }
    }
    still_missing
}

/// Espera o cookie aparecer (o usuário concluiu o cadastro), respeitando o
/// cancelamento e o fechamento da janela.
async fn wait_for_signup(
    cdp: &mut CdpClient,
    chromium: &ChromiumManager,
    stop: &AtomicBool,
    identity: &SignupIdentity,
) -> Result<String, String> {
    let attempts = (CAPTCHA_WAIT.as_millis() / POLL_INTERVAL.as_millis()) as u32;
    for tick in 0..attempts {
        if stop.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        if !chromium.is_alive(LOGIN_KEY) {
            return Err("A janela foi fechada antes do cadastro terminar".to_string());
        }
        if let Ok(Some(cookie)) = cdp.get_roblosecurity().await {
            if !cookie.trim().is_empty() {
                return Ok(cookie);
            }
        }
        // Reparo periódico: campo que remontou volta a ser preenchido, e a
        // segunda tela (a que pede a senha depois do "Continue") é preenchida
        // assim que aparece.
        if tick % REPAIR_EVERY_TICKS == 0 {
            let _ = fill_signup_form(cdp, identity).await;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err("Tempo esgotado esperando a conclusão do cadastro".to_string())
}

/// Valida o cookie recém-criado e guarda a conta com a senha gerada.
async fn save_account(
    accounts: &AccountStore,
    cookie: String,
    identity: &SignupIdentity,
) -> Result<String, String> {
    let info = crate::api::auth::validate_cookie(&cookie).await?;
    let mut account = Account::new(cookie, info.name.clone(), info.user_id);
    // O nome que vale é o que o Roblox aceitou, não o que geramos: o usuário
    // pode ter corrigido o campo na tela.
    account.password = identity.password.clone();
    accounts.add(account)?;
    Ok(info.name)
}

#[tauri::command]
pub fn get_signup_status() -> SignupStatus {
    current_status()
}

#[tauri::command]
pub fn stop_signup_session(app: AppHandle) -> Result<(), String> {
    state().stop.store(true, Ordering::Relaxed);
    update(&app, |s| {
        if s.active {
            s.phase = "stopping".to_string();
        }
    });
    Ok(())
}

/// Abre o browser e roda a sessão até criar `count` contas, ser cancelada ou a
/// janela ser fechada.
#[tauri::command]
pub async fn start_signup_session(
    app: AppHandle,
    chromium: State<'_, ChromiumManager>,
    settings: State<'_, SettingsStore>,
    count: usize,
) -> Result<SignupStatus, String> {
    if current_status().active {
        return Err("Já existe uma criação de contas em andamento".to_string());
    }
    let total = count.clamp(1, MAX_ACCOUNTS_PER_SESSION);

    let binary = ensure_chromium(&app).await?;
    chromium.close_login_session();

    let stealth = settings.get_bool("Login", "StealthMode");
    // Padrão de nome escolhido pelo usuário: "arvore" gera "arvore_k3p9z".
    // Vazio mantém o nome de palavras de sempre.
    let username_prefix =
        super::signup::sanitize_username_prefix(&settings.get_string("Generator", "SignupUsernamePrefix"));
    let profile = ChromiumManager::login_profile(&app)?;
    super::commands::wipe_profile_dir(&profile)?;

    let (child, port) = spawn_chrome(&binary, &profile, "about:blank", true, stealth).await?;
    chromium.track(LOGIN_KEY, child);
    let port = port.ok_or("Não foi possível abrir o navegador de cadastro")?;

    let mut cdp = match CdpClient::connect(port).await {
        Ok(cdp) => cdp,
        Err(e) => {
            chromium.close_login_session();
            return Err(e);
        }
    };
    if stealth {
        let _ = cdp.inject_stealth().await;
    }

    state().stop.store(false, Ordering::Relaxed);
    update(&app, |s| {
        *s = SignupStatus {
            active: true,
            total,
            phase: "opening".to_string(),
            ..SignupStatus::default()
        };
    });

    let app_task = app.clone();
    let stop = state().stop.clone();
    tauri::async_runtime::spawn(async move {
        let chromium = app_task.state::<ChromiumManager>();
        let accounts = app_task.state::<AccountStore>();

        for index in 0..total {
            if stop.load(Ordering::Relaxed) || !chromium.is_alive(LOGIN_KEY) {
                break;
            }

            reset_session(&mut cdp).await;
            if cdp.navigate(ROBLOX_SIGNUP_URL).await.is_err() {
                update(&app_task, |s| {
                    s.phase = "error".to_string();
                    s.last_error = Some("Não foi possível abrir a página de cadastro".to_string());
                });
                break;
            }

            update(&app_task, |s| {
                s.current = index + 1;
                s.phase = "filling".to_string();
                s.identity = None;
            });

            if !cdp.wait_for_selector(SIGNUP_FORM_SELECTOR, 60).await {
                update(&app_task, |s| {
                    s.phase = "error".to_string();
                    s.last_error = Some("O formulário de cadastro não carregou".to_string());
                });
                break;
            }

            let seed = identity_seed(index);
            let mut identity = generate_identity(seed, current_year(), &username_prefix);
            pick_free_username(&mut identity, seed, &username_prefix).await;
            let missing = fill_signup_form(&mut cdp, &identity).await;

            update(&app_task, |s| {
                s.phase = "waiting-user".to_string();
                s.identity = Some(identity.clone());
                s.last_error = if missing.is_empty() {
                    None
                } else {
                    // Não para a sessão: o laço de reparo tenta de novo, e o
                    // usuário pode digitar na mão com o valor da tela.
                    Some(format!(
                        "Não consegui preencher: {}. Vou tentar de novo enquanto você resolve o CAPTCHA.",
                        missing.join(", ")
                    ))
                };
            });

            match wait_for_signup(&mut cdp, &chromium, &stop, &identity).await {
                Ok(cookie) => {
                    update(&app_task, |s| s.phase = "saving".to_string());
                    match save_account(&accounts, cookie, &identity).await {
                        Ok(name) => {
                            // A lista principal recarrega neste evento. Sem
                            // ele, as contas criadas só apareciam no próximo
                            // start do app — inclusive as de uma sessão que o
                            // usuário parou no meio.
                            let _ = app_task.emit(
                                "generator-account-added",
                                serde_json::json!({ "username": name }),
                            );
                            update(&app_task, |s| {
                                s.created += 1;
                                s.created_usernames.push(name);
                                s.last_error = None;
                            });
                        }
                        Err(e) => update(&app_task, |s| {
                            s.last_error = Some(format!("Conta criada, mas não foi salva: {}", e));
                        }),
                    }
                }
                Err(reason) if reason == "cancelled" => break,
                Err(reason) => {
                    update(&app_task, |s| s.last_error = Some(reason));
                    // Janela fechada encerra a sessão; timeout só pula a conta.
                    if !chromium.is_alive(LOGIN_KEY) {
                        break;
                    }
                }
            }
        }

        chromium.close_login_session();
        let _ = super::commands::wipe_profile_dir(&ChromiumManager::login_profile(&app_task).unwrap_or_default());
        update(&app_task, |s| {
            s.active = false;
            s.identity = None;
            s.phase = if s.phase == "error" { "error" } else { "done" }.to_string();
        });
    });

    Ok(current_status())
}

#[cfg(test)]
mod signup_session_tests {
    use super::*;

    /// Duas contas do mesmo lote não podem receber a mesma semente, nem quando
    /// o relógio devolve o mesmo instante.
    #[test]
    fn the_seed_changes_with_the_index() {
        let seeds: std::collections::HashSet<u64> = (0..32).map(identity_seed).collect();
        assert_eq!(seeds.len(), 32);
    }

    #[test]
    fn the_year_is_plausible() {
        let year = current_year();
        assert!((2020..2100).contains(&year), "ano improvável: {}", year);
    }

    #[test]
    fn the_default_status_is_idle_and_inactive() {
        let status = SignupStatus::default();
        assert!(!status.active);
        assert_eq!(status.phase, "idle");
        assert_eq!(status.created, 0);
        assert!(status.identity.is_none());
        assert!(status.created_usernames.is_empty());
    }

    /// O teto existe para um clique errado não virar uma maratona de cadastros.
    #[test]
    fn the_requested_count_is_clamped_to_the_session_limit() {
        for (asked, expected) in [(0usize, 1usize), (1, 1), (10, 10), (999, MAX_ACCOUNTS_PER_SESSION)] {
            assert_eq!(asked.clamp(1, MAX_ACCOUNTS_PER_SESSION), expected);
        }
    }
}
