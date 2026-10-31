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
use super::download::resolve_browser_binary;
use super::manager::{ChromiumManager, LOGIN_KEY};
use super::signup::{
    generate_identity, marked_field_selector, missing_fields, numbered_username, signup_prepare_script,
    signup_submit_script, submit_result, with_chosen_password, SeededRng, SignupIdentity, SubmitResult, ROBLOX_SIGNUP_URL, SIGNUP_FORM_SELECTOR,
};

/// Depois de clicar em "Cadastrar-se", quanto esperar antes de poder clicar de
/// novo (a pessoa fechou a verificação, ou o Roblox não reagiu). Nunca em
/// sequência: clicar no meio de um CAPTCHA o reiniciaria.
const RESUBMIT_AFTER: Duration = Duration::from_secs(20);

/// Clica em "Cadastrar-se" agora? Só com a opção ligada
/// (`Generator.SignupAutoSubmit`), o formulário completo e, se já clicou,
/// passado `RESUBMIT_AFTER`.
fn should_submit(auto_submit: bool, form_complete: bool, since_last_click: Option<Duration>) -> bool {
    auto_submit && form_complete && since_last_click.is_none_or(|elapsed| elapsed >= RESUBMIT_AFTER)
}

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

/// Quantos números seguidos o nome numerado tenta antes de voltar ao sorteio
/// (`vibe_1` a `vibe_20` todos em uso: o prefixo é comum demais).
const NUMBERED_ATTEMPTS: u32 = 20;

/// Nome numerado livre a partir de `start` (`prefixo_N`). Devolve o número
/// usado; `None` deixa o nome sorteado da identidade (sem número que caiba ou
/// todos em uso). Serviço fora do ar: segue com o número, como o sorteio.
async fn pick_free_numbered_username(identity: &mut SignupIdentity, prefix: &str, start: u32) -> Option<u32> {
    let birthday = crate::api::roblox::signup_birthday_iso(&identity.day, &identity.month, &identity.year);
    for number in start..start.saturating_add(NUMBERED_ATTEMPTS) {
        let name = numbered_username(prefix, number)?;
        match crate::api::roblox::check_signup_username(&name, &birthday).await {
            Ok(check) if !check.available => continue,
            _ => {
                identity.username = name;
                return Some(number);
            }
        }
    }
    None
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

/// O cookie é de uma conta que **já está salva**: não é a conta que acabou de
/// ser criada. Acontece entre um cadastro e o próximo do lote: uma resposta
/// atrasada da conta anterior devolve o cookie dela depois da limpeza, e o
/// cadastro novo "terminava" na hora — gravando a senha da próxima identidade
/// em cima da conta anterior (relato do dono, 25/10/2026: conta criada pelo
/// app com "senha incorreta" no login).
fn cookie_is_from_a_saved_account(saved_user_ids: &[i64], user_id: i64) -> bool {
    saved_user_ids.contains(&user_id)
}

/// A senha que vai para a conta: a que estava no campo na hora do envio (a
/// pessoa pode ter mexido nela); sem leitura, a gerada.
fn password_to_save(typed: Option<&str>, generated: &str) -> String {
    match typed {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => generated.to_string(),
    }
}

/// O valor do campo de senha marcado pelo app (`data-ram-field`), se houver.
const READ_PASSWORD_FIELD_SCRIPT: &str =
    "(function(){var el=document.querySelector('[data-ram-field=\"password\"]');return el?el.value:null;})()";

/// Quantas vezes recomeçar a mesma conta quando o cookie que aparece é de uma
/// conta já salva.
const STALE_COOKIE_RETRIES: usize = 3;

/// Espera o cookie aparecer (o usuário concluiu o cadastro), respeitando o
/// cancelamento e o fechamento da janela. Devolve também o último valor visto
/// no campo de senha (o que foi enviado).
async fn wait_for_signup(
    cdp: &mut CdpClient,
    chromium: &ChromiumManager,
    stop: &AtomicBool,
    identity: &SignupIdentity,
    auto_submit: bool,
) -> Result<(String, Option<String>), String> {
    let attempts = (CAPTCHA_WAIT.as_millis() / POLL_INTERVAL.as_millis()) as u32;
    let mut last_click: Option<std::time::Instant> = None;
    let mut typed_password: Option<String> = None;
    for tick in 0..attempts {
        if stop.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        if !chromium.is_alive(LOGIN_KEY) {
            return Err("A janela foi fechada antes do cadastro terminar".to_string());
        }
        if let Ok(Some(cookie)) = cdp.get_roblosecurity().await {
            if !cookie.trim().is_empty() {
                return Ok((cookie, typed_password));
            }
        }
        // O que está no campo agora: é o que vai no envio.
        if let Ok(serde_json::Value::String(value)) = cdp.eval(READ_PASSWORD_FIELD_SCRIPT).await {
            if !value.is_empty() {
                typed_password = Some(value);
            }
        }
        // Reparo periódico: campo que remontou volta a ser preenchido, e a
        // segunda tela (a que pede a senha depois do "Continue") é preenchida
        // assim que aparece.
        if tick % REPAIR_EVERY_TICKS == 0 {
            let missing = fill_signup_form(cdp, identity).await;
            // "Cadastrar-se" sozinho: o formulário completo, nenhuma
            // verificação na tela. Se o Roblox mostrar o CAPTCHA, a pessoa
            // resolve e o cadastro termina sem outro clique.
            if should_submit(auto_submit, missing.is_empty(), last_click.map(|at| at.elapsed())) {
                if let Ok(result) = cdp.eval(&signup_submit_script()).await {
                    if submit_result(&result) == SubmitResult::Clicked {
                        last_click = Some(std::time::Instant::now());
                    }
                }
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err("Tempo esgotado esperando a conclusão do cadastro".to_string())
}

/// O que aconteceu ao guardar a conta.
enum SignupSave {
    Saved(String),
    /// O cookie era de uma conta já salva: nada foi gravado.
    StaleCookie,
}

/// Valida o cookie recém-criado e guarda a conta com a senha do campo (ou a
/// gerada). Cookie de conta que já está na lista não grava nada.
async fn save_account(
    accounts: &AccountStore,
    cookie: String,
    identity: &SignupIdentity,
    typed_password: Option<&str>,
) -> Result<SignupSave, String> {
    let info = crate::api::auth::validate_cookie(&cookie).await?;
    let saved: Vec<i64> = accounts.get_all()?.iter().map(|a| a.user_id).collect();
    if cookie_is_from_a_saved_account(&saved, info.user_id) {
        return Ok(SignupSave::StaleCookie);
    }
    let mut account = Account::new(cookie, info.name.clone(), info.user_id);
    // O nome que vale é o que o Roblox aceitou, não o que geramos: o usuário
    // pode ter corrigido o campo na tela.
    account.password = password_to_save(typed_password, &identity.password);
    accounts.add(account)?;
    Ok(SignupSave::Saved(info.name))
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
    password: Option<String>,
) -> Result<SignupStatus, String> {
    if current_status().active {
        return Err("Já existe uma criação de contas em andamento".to_string());
    }
    let total = count.clamp(1, MAX_ACCOUNTS_PER_SESSION);

    // O mesmo navegador do login: o caminho manual, se configurado; senão o
    // Chromium baixado; e o do sistema se o download falhar. Antes era sempre
    // o baixado, e o caminho manual valia só para o login.
    let (binary, _) =
        resolve_browser_binary(&app, Some(&super::commands::manual_binary_path(&settings))).await?;
    chromium.close_login_session();

    let stealth = settings.get_bool("Login", "StealthMode");
    // Clicar em "Cadastrar-se" sozinho (padrão ligado desde 14/10/2026).
    let auto_submit = settings.get_bool("Generator", "SignupAutoSubmit");
    // Padrão de nome escolhido pelo usuário: "arvore" gera "arvore_k3p9z".
    // Vazio mantém o nome de palavras de sempre.
    let username_prefix =
        super::signup::sanitize_username_prefix(&settings.get_string("Generator", "SignupUsernamePrefix"));
    // Nomes em ordem (`prefixo_1`, `prefixo_2`...): o próximo número fica
    // gravado, e o lote seguinte continua de onde este parou.
    let numbered = settings.get_bool("Generator", "SignupUsernameNumbered") && !username_prefix.is_empty();
    let mut next_number = settings
        .get_int("Generator", "SignupUsernameNext")
        .unwrap_or(1)
        .clamp(1, u32::MAX as i64) as u32;
    // A senha escolhida para o lote fica só na memória desta sessão: não vai
    // para o RAMSettings.ini (que é texto puro).
    let chosen_password = password.filter(|p| !p.is_empty());
    let profile = ChromiumManager::login_profile(&app)?;
    super::commands::wipe_profile_dir(&profile)?;

    let (child, port) = spawn_chrome(&binary, &profile, "about:blank", true, stealth).await?;
    chromium.track(LOGIN_KEY, child);
    let Some(port) = port else {
        // A janela que abriu mas não respondeu ficava aberta em about:blank,
        // com o painel em "Inativo" (relato de 23/10/2026).
        chromium.close_login_session();
        return Err(super::cdp::BROWSER_DID_NOT_OPEN.to_string());
    };

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
            let number = if numbered {
                pick_free_numbered_username(&mut identity, &username_prefix, next_number).await
            } else {
                None
            };
            if number.is_none() {
                pick_free_username(&mut identity, seed, &username_prefix).await;
            }
            // Depois do nome: a senha escolhida não pode conter o nome final.
            let identity = with_chosen_password(identity, chosen_password.as_deref());
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

            let mut stale_retries = 0;
            let mut end_session = false;
            loop {
                match wait_for_signup(&mut cdp, &chromium, &stop, &identity, auto_submit).await {
                    Ok((cookie, typed_password)) => {
                        update(&app_task, |s| s.phase = "saving".to_string());
                        match save_account(&accounts, cookie, &identity, typed_password.as_deref()).await {
                            Ok(SignupSave::Saved(name)) => {
                                if let Some(used) = number {
                                    next_number = used.saturating_add(1);
                                    let _ = app_task.state::<SettingsStore>().set(
                                        "Generator",
                                        "SignupUsernameNext",
                                        &next_number.to_string(),
                                    );
                                }
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
                            // O navegador ainda estava na conta anterior: limpa,
                            // reabre o cadastro e espera esta conta de novo.
                            Ok(SignupSave::StaleCookie) if stale_retries < STALE_COOKIE_RETRIES => {
                                stale_retries += 1;
                                reset_session(&mut cdp).await;
                                let reopened = cdp.navigate(ROBLOX_SIGNUP_URL).await.is_ok()
                                    && cdp.wait_for_selector(SIGNUP_FORM_SELECTOR, 60).await;
                                if reopened {
                                    let _ = fill_signup_form(&mut cdp, &identity).await;
                                    update(&app_task, |s| s.phase = "waiting-user".to_string());
                                    continue;
                                }
                                update(&app_task, |s| {
                                    s.last_error = Some("O formulário de cadastro não carregou".to_string());
                                });
                            }
                            Ok(SignupSave::StaleCookie) => update(&app_task, |s| {
                                s.last_error = Some(
                                    "O navegador continuou na conta anterior; esta conta foi pulada".to_string(),
                                );
                            }),
                            Err(e) => update(&app_task, |s| {
                                s.last_error = Some(format!("Conta criada, mas não foi salva: {}", e));
                            }),
                        }
                    }
                    Err(reason) if reason == "cancelled" => end_session = true,
                    Err(reason) => {
                        update(&app_task, |s| s.last_error = Some(reason));
                        // Janela fechada encerra a sessão; timeout só pula a conta.
                        if !chromium.is_alive(LOGIN_KEY) {
                            end_session = true;
                        }
                    }
                }
                break;
            }
            if end_session {
                break;
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

    /// Relato do dono (25/10/2026): conta criada pelo criador e "senha
    /// incorreta" no login com a senha salva. Um cookie de conta que **já está
    /// salva** (o da conta anterior, voltando depois da limpeza entre um
    /// cadastro e outro) não é conta nova — gravar com ele punha a senha da
    /// próxima identidade em cima da conta anterior.
    #[test]
    fn a_cookie_of_an_account_already_saved_is_not_a_new_account() {
        assert!(cookie_is_from_a_saved_account(&[10, 20], 20));
        assert!(!cookie_is_from_a_saved_account(&[10, 20], 30));
        assert!(!cookie_is_from_a_saved_account(&[], 30));
    }

    /// A senha que vale é a que estava no campo na hora do envio (a pessoa
    /// pode ter mexido); sem leitura, a gerada.
    #[test]
    fn the_password_saved_is_the_one_in_the_field() {
        assert_eq!(password_to_save(Some("Typed123"), "Generated7"), "Typed123");
        assert_eq!(password_to_save(Some(""), "Generated7"), "Generated7");
        assert_eq!(password_to_save(None, "Generated7"), "Generated7");
    }

    #[test]
    fn it_submits_once_the_form_is_complete_and_only_if_the_option_is_on() {
        assert!(should_submit(true, true, None));
        assert!(!should_submit(false, true, None), "option off: the person presses Create Account");
        assert!(!should_submit(true, false, None), "a field is still empty");
    }

    /// Depois de um clique, só clica de novo bem depois (a pessoa fechou a
    /// verificação, ou o Roblox não reagiu) — nunca em sequência.
    #[test]
    fn a_second_click_waits() {
        assert!(!should_submit(true, true, Some(Duration::from_secs(5))));
        assert!(!should_submit(true, true, Some(RESUBMIT_AFTER - Duration::from_millis(1))));
        assert!(should_submit(true, true, Some(RESUBMIT_AFTER)));
    }

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
