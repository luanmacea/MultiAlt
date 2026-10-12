// Clientes que fecharam: a conta que estava num jogo e cujo cliente terminou
// (fechou sozinho, foi morto fora do app, caiu e a pessoa fechou a janela)
// fica anotada aqui, com o motivo e o destino para "Entrar novamente". Pedido
// do dono (12/10/2026): a conta sumia do estado ao vivo da lista de contas sem
// dizer que tinha fechado. Ver docs/features/launch.md ("Cliente que fechou").
//
// Regras que não podem se perder:
// - Só cliente que o app abriu. O do site (adotado) é da pessoa: fechar ele é
//   escolha dela, e o app nunca relança cliente adotado (igual à reconexão).
// - Cliente que o próprio app fechou (Fechar, Auto Rejoin, Watcher, teto de
//   memória, a reconexão trocando o cliente) não conta: ninguém precisa ser
//   avisado do que pediu. Conta do Auto Rejoin também não: ele reabre sozinho.
// - Só quem estava num jogo (ou caiu dele): quem saiu pelo menu do Roblox, ou
//   nunca entrou num jogo, fechou porque quis. Sem log não dá para saber — não
//   chuta (mesma regra do "fechou sem sair do jogo" do monitor de quedas).
// - Some quando a conta abre de novo (qualquer cliente novo dela, inclusive
//   pelo site), quando a pessoa dispensa, ou quando o app reabre (só memória).
// - O destino é o mesmo da reconexão automática (`reconnect_target`): servidor
//   privado com os dados do launch, público com o place e o Job ID do log.
//   Nada aqui abre cliente: "Entrar novamente" é o launch normal da tela.

/// O que o monitor de quedas sabe de um cliente rastreado numa passada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientSeen {
    pub user_id: i64,
    pub pid: u32,
    pub adopted: bool,
    /// O processo terminou (o tracker ainda o lista).
    pub exited: bool,
    /// Está num jogo (log): entrou e não caiu nem saiu.
    pub in_game: bool,
    pub drop: Option<ClientDrop>,
    pub destination: Option<JoinedDestination>,
}

/// O que o book pergunta ao mundo na hora em que um cliente some (dublê nos testes).
pub trait ClosedClientContext {
    /// Foi o app que fechou este PID.
    fn terminated_by_app(&self, pid: u32) -> bool;
    /// O Auto Rejoin gerencia a conta agora.
    fn auto_rejoin(&self, user_id: i64) -> bool;
    /// Para onde o app mandou a conta no último launch.
    fn launched_target(&self, user_id: i64) -> Option<LaunchedTarget>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClosedClient {
    pid: u32,
    closed_at_ms: i64,
    drop: ClientDrop,
    place_id: Option<i64>,
    target: Option<ReconnectTarget>,
}

/// O destino de "Entrar novamente", nos termos do launch da tela (`joinServer`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejoinTargetView {
    pub place_id: i64,
    pub job_id: String,
    pub launch_data: String,
    pub join_vip: bool,
    pub link_code: String,
}

/// O que a lista de contas e a Sessão mostram de uma conta cujo cliente fechou.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedClientView {
    pub user_id: i64,
    pub closed_at_ms: i64,
    /// O motivo (`crashed` = fechou sem sair do jogo).
    pub drop: ClientDrop,
    /// O jogo em que estava.
    pub place_id: Option<i64>,
    /// `None`: nem o log nem o launch dizem para onde voltar.
    pub rejoin: Option<RejoinTargetView>,
}

#[derive(Debug, Default)]
pub struct ClosedClientBook {
    /// O último retrato de cada cliente vivo, para saber como ele estava
    /// quando sumiu (o monitor esquece o cliente junto com o tracker).
    seen: HashMap<i64, ClientSeen>,
    closed: HashMap<i64, ClosedClient>,
}

impl ClosedClientBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Uma passada. Devolve `true` quando a lista de fechados mudou.
    pub fn observe(&mut self, clients: &[ClientSeen], now_ms: i64, ctx: &dyn ClosedClientContext) -> bool {
        let mut changed = false;
        // Cliente vivo: guarda o retrato; se é um cliente novo de uma conta
        // anotada como fechada, ela abriu de novo.
        for client in clients.iter().filter(|c| !c.exited) {
            if self
                .closed
                .get(&client.user_id)
                .is_some_and(|closed| closed.pid != client.pid)
            {
                self.closed.remove(&client.user_id);
                changed = true;
            }
            self.seen.insert(client.user_id, client.clone());
        }
        // Quem tinha retrato e não tem mais cliente vivo: fechou.
        let gone: Vec<i64> = self
            .seen
            .keys()
            .copied()
            .filter(|uid| !clients.iter().any(|c| c.user_id == *uid && !c.exited))
            .collect();
        for user_id in gone {
            let Some(last) = self.seen.remove(&user_id) else {
                continue;
            };
            // O processo morto que o tracker ainda lista traz o que o monitor
            // viu na saída (o "fechou sem sair do jogo").
            let exited = clients
                .iter()
                .find(|c| c.user_id == user_id && c.pid == last.pid && c.exited);
            if let Some(closed) = Self::closed_client(&last, exited, now_ms, ctx) {
                self.closed.insert(user_id, closed);
                changed = true;
            }
        }
        changed
    }

    fn closed_client(
        last: &ClientSeen,
        exited: Option<&ClientSeen>,
        now_ms: i64,
        ctx: &dyn ClosedClientContext,
    ) -> Option<ClosedClient> {
        if last.adopted || ctx.terminated_by_app(last.pid) || ctx.auto_rejoin(last.user_id) {
            return None;
        }
        let drop = exited
            .and_then(|c| c.drop.clone())
            .or_else(|| last.drop.clone())
            .or_else(|| {
                last.in_game.then(|| ClientDrop {
                    kind: DropKind::Crashed,
                    reason: None,
                    code: None,
                    message: None,
                    since_ms: now_ms,
                })
            })?;
        let destination = exited
            .and_then(|c| c.destination.clone())
            .or_else(|| last.destination.clone());
        let launched = ctx.launched_target(last.user_id);
        let target = reconnect_target(launched.as_ref(), destination.as_ref(), drop.kind);
        let place_id = destination
            .as_ref()
            .map(|d| d.place_id)
            .or_else(|| target.as_ref().map(|t| t.place_id));
        Some(ClosedClient {
            pid: last.pid,
            closed_at_ms: now_ms,
            drop,
            place_id,
            target,
        })
    }

    /// "×" da linha: esquece o cliente fechado. `false` se já não estava.
    pub fn dismiss(&mut self, user_id: i64) -> bool {
        self.closed.remove(&user_id).is_some()
    }

    pub fn views(&self) -> Vec<ClosedClientView> {
        let mut views: Vec<ClosedClientView> = self
            .closed
            .iter()
            .map(|(user_id, closed)| ClosedClientView {
                user_id: *user_id,
                closed_at_ms: closed.closed_at_ms,
                drop: closed.drop.clone(),
                place_id: closed.place_id,
                rejoin: closed.target.as_ref().map(|t| RejoinTargetView {
                    place_id: t.place_id,
                    job_id: t.job_id.clone(),
                    launch_data: t.launch_data.clone(),
                    join_vip: t.join_vip,
                    link_code: t.link_code.clone(),
                }),
            })
            .collect();
        views.sort_by_key(|v| v.user_id);
        views
    }
}

static CLOSED_CLIENT_BOOK: std::sync::LazyLock<std::sync::Mutex<ClosedClientBook>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ClosedClientBook::new()));

fn with_closed_client_book<R>(f: impl FnOnce(&mut ClosedClientBook) -> R) -> R {
    let mut book = match CLOSED_CLIENT_BOOK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut book)
}

fn publish_closed_clients(app: &tauri::AppHandle) {
    let entries = with_closed_client_book(|book| book.views());
    let _ = app.emit("closed-clients", serde_json::json!({ "entries": entries }));
}

#[tauri::command]
fn get_closed_clients() -> serde_json::Value {
    serde_json::json!({ "entries": with_closed_client_book(|book| book.views()) })
}

/// "×" da linha (lista de contas ou Sessão). Não fecha nem abre nada.
#[tauri::command]
fn dismiss_closed_client(app: tauri::AppHandle, user_id: i64) -> bool {
    let dismissed = with_closed_client_book(|book| book.dismiss(user_id));
    if dismissed {
        publish_closed_clients(&app);
    }
    dismissed
}

#[cfg(target_os = "windows")]
struct WindowsClosedClientContext {
    botting: BottingStatusPayload,
}

#[cfg(target_os = "windows")]
impl ClosedClientContext for WindowsClosedClientContext {
    fn terminated_by_app(&self, pid: u32) -> bool {
        platform::windows::was_terminated_by_app(pid)
    }
    fn auto_rejoin(&self, user_id: i64) -> bool {
        managed_by_auto_rejoin(&self.botting, user_id)
    }
    fn launched_target(&self, user_id: i64) -> Option<LaunchedTarget> {
        launched_target_of(user_id)
    }
}

/// Logo depois da passada do monitor de quedas: confere quem fechou.
#[cfg(target_os = "windows")]
pub(crate) fn closed_clients_after_health_tick(app: &tauri::AppHandle) {
    let tracked = platform::windows::tracker().get_all();
    let idle = tracked.is_empty() && with_closed_client_book(|book| book.seen.is_empty());
    if idle {
        return;
    }
    let views: HashMap<i64, ClientHealthView> = CLIENT_HEALTH_VIEWS
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();
    let clients: Vec<ClientSeen> = tracked
        .iter()
        .map(|p| {
            let view = views.get(&p.user_id).filter(|v| v.pid == p.pid);
            ClientSeen {
                user_id: p.user_id,
                pid: p.pid,
                adopted: p.adopted,
                exited: view.is_some_and(|v| v.exited),
                in_game: view.is_some_and(|v| v.in_game),
                drop: view.and_then(|v| v.drop.clone()),
                destination: view.and_then(|v| v.destination.clone()),
            }
        })
        .collect();
    let ctx = WindowsClosedClientContext {
        botting: current_botting_status(),
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    if with_closed_client_book(|book| book.observe(&clients, now_ms, &ctx)) {
        publish_closed_clients(app);
    }
}

#[cfg(test)]
mod closed_clients_tests {
    use super::*;

    const T0: i64 = 1_791_000_000_000;

    #[derive(Default)]
    struct World {
        terminated: Vec<u32>,
        auto_rejoin: Vec<i64>,
        launched: HashMap<i64, LaunchedTarget>,
    }

    impl ClosedClientContext for World {
        fn terminated_by_app(&self, pid: u32) -> bool {
            self.terminated.contains(&pid)
        }
        fn auto_rejoin(&self, user_id: i64) -> bool {
            self.auto_rejoin.contains(&user_id)
        }
        fn launched_target(&self, user_id: i64) -> Option<LaunchedTarget> {
            self.launched.get(&user_id).cloned()
        }
    }

    fn public_launch(place_id: i64) -> LaunchedTarget {
        LaunchedTarget {
            place_id,
            job_id: String::new(),
            launch_data: "data".into(),
            join_vip: false,
            link_code: String::new(),
            private: false,
        }
    }

    fn playing(user_id: i64, pid: u32, place_id: i64, job: &str) -> ClientSeen {
        ClientSeen {
            user_id,
            pid,
            in_game: true,
            destination: Some(JoinedDestination {
                place_id,
                job_id: Some(job.into()),
            }),
            ..ClientSeen::default()
        }
    }

    fn drop_of(kind: DropKind) -> ClientDrop {
        ClientDrop {
            kind,
            reason: None,
            code: Some(267),
            message: Some("AFK".into()),
            since_ms: T0,
        }
    }

    fn world_with_launch(user_id: i64, place_id: i64) -> World {
        let mut world = World::default();
        world.launched.insert(user_id, public_launch(place_id));
        world
    }

    #[test]
    fn a_client_that_disappears_while_in_a_game_is_closed_with_the_same_server_to_rejoin() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        assert!(!book.observe(&[playing(1, 100, 10, "job-a")], T0, &world));
        assert!(book.views().is_empty());

        assert!(book.observe(&[], T0 + 2_000, &world));
        let views = book.views();
        assert_eq!(views.len(), 1);
        let closed = &views[0];
        assert_eq!(closed.user_id, 1);
        assert_eq!(closed.closed_at_ms, T0 + 2_000);
        assert_eq!(closed.drop.kind, DropKind::Crashed);
        assert_eq!(closed.place_id, Some(10));
        assert_eq!(
            closed.rejoin,
            Some(RejoinTargetView {
                place_id: 10,
                job_id: "job-a".into(),
                launch_data: "data".into(),
                join_vip: false,
                link_code: String::new(),
            })
        );
    }

    #[test]
    fn a_crash_the_monitor_saw_keeps_its_drop_and_the_lingering_client_stays_closed() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "job-a")], T0, &world);
        let exited = ClientSeen {
            exited: true,
            in_game: false,
            drop: Some(drop_of(DropKind::Crashed)),
            ..playing(1, 100, 10, "job-a")
        };
        assert!(book.observe(std::slice::from_ref(&exited), T0 + 2_000, &world));
        assert_eq!(book.views()[0].drop.kind, DropKind::Crashed);
        // O tracker ainda lista o processo morto: nada muda.
        assert!(!book.observe(&[exited], T0 + 4_000, &world));
        assert_eq!(book.views().len(), 1);
    }

    #[test]
    fn a_kicked_client_the_person_closed_keeps_the_kick_as_the_reason() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        let kicked = ClientSeen {
            in_game: false,
            drop: Some(drop_of(DropKind::Kicked)),
            ..playing(1, 100, 10, "job-a")
        };
        book.observe(&[kicked], T0, &world);
        assert!(book.observe(&[], T0 + 2_000, &world));
        let closed = &book.views()[0];
        assert_eq!(closed.drop.kind, DropKind::Kicked);
        assert_eq!(closed.drop.message.as_deref(), Some("AFK"));
        assert_eq!(closed.rejoin.as_ref().map(|t| t.job_id.as_str()), Some("job-a"));
    }

    #[test]
    fn a_server_shutdown_rejoins_the_game_without_the_dead_server() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        let shut = ClientSeen {
            in_game: false,
            drop: Some(drop_of(DropKind::ServerShutdown)),
            ..playing(1, 100, 10, "job-a")
        };
        book.observe(&[shut], T0, &world);
        book.observe(&[], T0 + 2_000, &world);
        let rejoin = book.views()[0].rejoin.clone().unwrap();
        assert_eq!((rejoin.place_id, rejoin.job_id.as_str()), (10, ""));
    }

    #[test]
    fn a_private_server_rejoins_with_the_launch_data_never_a_made_up_link() {
        let mut world = World::default();
        world.launched.insert(
            1,
            LaunchedTarget {
                place_id: 10,
                job_id: String::new(),
                launch_data: String::new(),
                join_vip: true,
                link_code: "CODE".into(),
                private: true,
            },
        );
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "reserved-job")], T0, &world);
        book.observe(&[], T0 + 2_000, &world);
        let rejoin = book.views()[0].rejoin.clone().unwrap();
        assert_eq!(rejoin.place_id, 10);
        assert_eq!(rejoin.job_id, "");
        assert!(rejoin.join_vip);
        assert_eq!(rejoin.link_code, "CODE");
    }

    #[test]
    fn what_the_app_closed_is_not_announced() {
        let mut world = world_with_launch(1, 10);
        world.terminated.push(100);
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "job-a")], T0, &world);
        assert!(!book.observe(&[], T0 + 2_000, &world));
        assert!(book.views().is_empty());
    }

    #[test]
    fn site_clients_and_auto_rejoin_accounts_are_left_alone() {
        let mut world = world_with_launch(2, 10);
        world.auto_rejoin.push(2);
        let mut book = ClosedClientBook::new();
        let site = ClientSeen {
            adopted: true,
            ..playing(1, 100, 10, "job-a")
        };
        book.observe(&[site, playing(2, 200, 10, "job-a")], T0, &world);
        assert!(!book.observe(&[], T0 + 2_000, &world));
        assert!(book.views().is_empty());
    }

    #[test]
    fn leaving_the_game_or_never_joining_one_is_a_choice_not_a_close() {
        let world = World::default();
        let mut book = ClosedClientBook::new();
        let left = ClientSeen {
            user_id: 1,
            pid: 100,
            ..ClientSeen::default()
        };
        let never = ClientSeen {
            user_id: 2,
            pid: 200,
            ..ClientSeen::default()
        };
        book.observe(&[left, never], T0, &world);
        assert!(!book.observe(&[], T0 + 2_000, &world));
        assert!(book.views().is_empty());
    }

    #[test]
    fn opening_the_account_again_clears_it_any_way_it_opens() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "job-a")], T0, &world);
        book.observe(&[], T0 + 2_000, &world);
        assert_eq!(book.views().len(), 1);
        // Um cliente novo da conta (pelo app ou pelo site) apaga o aviso.
        let reopened = ClientSeen {
            user_id: 1,
            pid: 101,
            adopted: true,
            ..ClientSeen::default()
        };
        assert!(book.observe(&[reopened], T0 + 4_000, &world));
        assert!(book.views().is_empty());
    }

    #[test]
    fn a_relaunch_that_swaps_the_client_in_one_pass_is_not_a_close() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "job-a")], T0, &world);
        assert!(!book.observe(&[playing(1, 101, 10, "job-b")], T0 + 2_000, &world));
        assert!(book.views().is_empty());
    }

    #[test]
    fn dismissing_forgets_it_once() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "job-a")], T0, &world);
        book.observe(&[], T0 + 2_000, &world);
        assert!(book.dismiss(1));
        assert!(!book.dismiss(1));
        assert!(book.views().is_empty());
        // E não volta sozinho na passada seguinte.
        assert!(!book.observe(&[], T0 + 4_000, &world));
        assert!(book.views().is_empty());
    }

    #[test]
    fn the_view_goes_to_the_screen_in_camel_case() {
        let world = world_with_launch(1, 10);
        let mut book = ClosedClientBook::new();
        book.observe(&[playing(1, 100, 10, "job-a")], T0, &world);
        book.observe(&[], T0 + 2_000, &world);
        let json = serde_json::to_value(book.views()).unwrap();
        assert_eq!(json[0]["userId"], 1);
        assert_eq!(json[0]["closedAtMs"], T0 + 2_000);
        assert_eq!(json[0]["drop"]["kind"], "crashed");
        assert_eq!(json[0]["placeId"], 10);
        assert_eq!(json[0]["rejoin"]["jobId"], "job-a");
        assert_eq!(json[0]["rejoin"]["joinVip"], false);
    }

    #[test]
    fn the_monitor_loop_feeds_the_book_every_pass() {
        // A conferência roda junto com a reconexão, a cada passada do monitor.
        let source = include_str!("client_health.rs");
        let start = source.find("pub(crate) fn start_client_health_monitor(").unwrap();
        let body = &source[start..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("closed_clients_after_health_tick(&app)"));
    }
}
