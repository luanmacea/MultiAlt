// Reconexão automática: quando o monitor de quedas (client_health.rs) diz que
// a conta caiu, relança **só aquela conta**, no mesmo destino, com espera
// crescente. Ver docs/features/watcher.md ("Reconexão automática").
//
// Regras que não podem se perder:
// - Só cliente que o app abriu. Cliente do site (adotado) nunca é fechado nem
//   relançado; o app também nunca fecha cliente de outra conta: o único
//   cliente fechado aqui é o da própria conta que caiu, e só se ele não
//   estiver em jogo.
// - Para (e não tenta de novo) quando a conta entrou em outro lugar (as duas
//   sessões se derrubariam em laço), quando a pessoa fechou o cliente, quando
//   a conta está banida e quando a sessão expirou.
// - Cada tentativa pega um ticket novo pelo launch normal, **sem** renovar a
//   sessão (`allow_session_refresh = false`): o refresh desloga a conta de
//   todo lugar, e aqui ninguém está olhando.
// - Auto Rejoin manda na conta que ele gerencia: a reconexão fica de fora.
// - Nada é gravado em disco: ao reabrir o app, nada é retomado sozinho.
//
// A máquina de estados (`ReconnectBook`) é pura e testada abaixo; o laço do
// Windows só junta as peças, a cada passada do monitor de quedas.

/// Espera antes de cada tentativa: a 1ª depois de 10 s, a 5ª (e o teto) 5 min.
pub const RECONNECT_BACKOFF_SECS: [i64; 5] = [10, 30, 60, 120, 300];
/// Tentativas seguidas que não ficam no jogo antes de desistir.
pub const RECONNECT_MAX_ATTEMPTS: u32 = 5;
/// Tempo no jogo para a reconexão contar como bem-sucedida (zera as tentativas).
pub const RECONNECT_STABLE_MS: i64 = 120_000;
/// Relançou e não entrou no jogo neste tempo (com o log achado): tentativa falhou.
pub const RECONNECT_JOIN_TIMEOUT_MS: i64 = 120_000;
/// Sem internet: confere de novo depois disto, sem gastar tentativa.
pub const RECONNECT_OFFLINE_RETRY_MS: i64 = 10_000;
/// A fila de launch está ocupada (outro launch rodando): tenta de novo depois disto.
pub const RECONNECT_BUSY_RETRY_MS: i64 = 5_000;

// O erro de uma tentativa, como vai para a tela (`error` da entrada). Em inglês
// porque a tradução é do front (`reconnectErrorText` em
// src/utils/autoReconnect.ts, que lê esta lista no teste): frase nova aqui
// precisa entrar lá, senão aparece crua na tela em português.
/// O cliente relançado foi fechado pelo próprio app antes de ficar no jogo.
pub const RECONNECT_ERROR_CLIENT_CLOSED: &str = "The client was closed";
/// Relançado, não entrou no jogo em `RECONNECT_JOIN_TIMEOUT_MS`.
pub const RECONNECT_ERROR_NOT_IN_GAME: &str = "It did not get into the game in 2 minutes";
/// O cliente caído da própria conta não fechou antes do relaunch.
#[allow(dead_code)]
pub const RECONNECT_ERROR_OLD_CLIENT_OPEN: &str = "The old client did not close";
/// O launch terminou sem cliente do app aberto.
#[allow(dead_code)]
pub const RECONNECT_ERROR_DID_NOT_START: &str = "The Roblox client did not start";

/// Espera antes da tentativa `attempt` (1, 2, ...).
pub fn reconnect_backoff_ms(attempt: u32) -> i64 {
    let index = (attempt.max(1) - 1).min(RECONNECT_BACKOFF_SECS.len() as u32 - 1) as usize;
    RECONNECT_BACKOFF_SECS[index] * 1_000
}

/// Por que a reconexão parou de vez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReconnectStopReason {
    /// A mesma conta entrou em outro lugar: relançar faria as duas sessões se
    /// derrubarem em laço.
    JoinedElsewhere,
    /// A pessoa fechou o cliente (ou saiu do jogo) por conta própria.
    ClosedByUser,
    /// O Roblox baniu ou encerrou a conta.
    Banned,
    /// O Roblox invalidou a sessão: precisa entrar de novo na conta.
    SessionExpired,
    /// Nem o log nem o launch dizem para onde voltar.
    NoDestination,
    /// A conta está aberta num cliente de fora do app (site): o app não o fecha.
    OpenedOutsideApp,
}

/// Para onde o app mandou a conta no último launch (launch.rs grava).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedTarget {
    pub place_id: i64,
    /// O Job ID como foi pedido (pode ser `vip:<código>` ou um link privado).
    pub job_id: String,
    pub launch_data: String,
    pub join_vip: bool,
    pub link_code: String,
    /// O launch foi para um servidor privado/VIP.
    pub private: bool,
}

/// O destino da reconexão, nos termos do launch normal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectTarget {
    pub place_id: i64,
    pub job_id: String,
    pub launch_data: String,
    pub join_vip: bool,
    pub link_code: String,
}

/// Para onde voltar. Servidor privado: os mesmos dados que o app usou no
/// launch (nunca um link inventado). Público: o place e o Job ID do último
/// "Joining game" do log — sem o Job ID quando o servidor fechou (ele não
/// existe mais). Sem log, o que o app pediu no launch.
pub fn reconnect_target(
    launched: Option<&LaunchedTarget>,
    seen: Option<&JoinedDestination>,
    kind: DropKind,
) -> Option<ReconnectTarget> {
    let server_gone = kind == DropKind::ServerShutdown;
    if let Some(l) = launched.filter(|l| l.private) {
        return Some(ReconnectTarget {
            place_id: l.place_id,
            job_id: l.job_id.clone(),
            launch_data: l.launch_data.clone(),
            join_vip: l.join_vip,
            link_code: l.link_code.clone(),
        });
    }
    if let Some(seen) = seen {
        // O launch data é do jogo que o app abriu: só vale no mesmo place.
        let launch_data = launched
            .filter(|l| l.place_id == seen.place_id)
            .map(|l| l.launch_data.clone())
            .unwrap_or_default();
        return Some(ReconnectTarget {
            place_id: seen.place_id,
            job_id: if server_gone {
                String::new()
            } else {
                seen.job_id.clone().unwrap_or_default()
            },
            launch_data,
            join_vip: false,
            link_code: String::new(),
        });
    }
    let l = launched?;
    Some(ReconnectTarget {
        place_id: l.place_id,
        job_id: if server_gone { String::new() } else { l.job_id.clone() },
        launch_data: l.launch_data.clone(),
        join_vip: false,
        link_code: String::new(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconnectPhase {
    /// Esperando a hora da próxima tentativa.
    Waiting { at_ms: i64 },
    /// Sem internet: confere de novo em `at_ms`, sem gastar tentativa.
    WaitingForInternet { at_ms: i64 },
    /// Tentativa em andamento (checagens + launch).
    Launching,
    /// Relançou: conferindo se fica no jogo por 2 min.
    Checking {
        launched_at_ms: i64,
        in_game_since_ms: Option<i64>,
    },
    GaveUp,
    Stopped(ReconnectStopReason),
}

impl ReconnectPhase {
    fn is_final(&self) -> bool {
        matches!(self, Self::GaveUp | Self::Stopped(_))
    }
}

#[derive(Debug, Clone)]
struct ReconnectEntry {
    phase: ReconnectPhase,
    /// A tentativa que espera, roda ou está sendo conferida (1, 2, ...).
    attempt: u32,
    /// A queda que começou (ou recomeçou) a reconexão.
    drop: ClientDrop,
    /// O cliente que o estado acompanha: o que caiu, enquanto espera; o
    /// relançado, enquanto confere.
    watched_pid: Option<u32>,
    /// O cliente que caiu continuou aberto (queda de rede, kick): se a pessoa
    /// o fechar enquanto espera, a reconexão para.
    watched_was_open: bool,
    target: Option<ReconnectTarget>,
    error: Option<String>,
}

/// Uma queda que o monitor reportou, com o que o laço sabe da conta.
#[derive(Debug, Clone)]
pub struct ReconnectDrop {
    pub user_id: i64,
    pub pid: Option<u32>,
    pub adopted: bool,
    pub drop: ClientDrop,
    /// A opção está ligada para esta conta.
    pub enabled: bool,
    /// O Auto Rejoin gerencia esta conta agora.
    pub auto_rejoin: bool,
    pub target: Option<ReconnectTarget>,
}

/// O que o laço vê da conta numa passada (dublê nos testes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconnectObservation {
    pub enabled: bool,
    pub auto_rejoin: bool,
    /// O cliente que o tracker tem para a conta agora.
    pub tracked_pid: Option<u32>,
    pub tracked_adopted: bool,
    /// Esse cliente está num jogo (log).
    pub tracked_in_game: bool,
    /// O cliente acompanhado terminou.
    pub watched_exited: bool,
    /// Foi o app que o fechou (Fechar, Watcher, a própria reconexão…).
    pub watched_closed_by_app: bool,
    /// O cliente acompanhado está num jogo (log).
    pub watched_in_game: bool,
    /// O log do cliente acompanhado foi achado.
    pub watched_log_found: bool,
    /// A queda que o monitor mostra agora para o cliente acompanhado (reserva
    /// para um aviso que chegou enquanto a tentativa rodava).
    pub watched_drop: Option<ClientDrop>,
}

/// O que fazer agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectAction {
    Attempt {
        user_id: i64,
        attempt: u32,
        target: ReconnectTarget,
    },
}

/// Como terminou uma tentativa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// O cliente novo abriu (PID detectado).
    Launched { pid: u32 },
    /// Sem internet: espera ela voltar sem gastar tentativa.
    NoInternet,
    /// Outro launch está rodando: tenta logo depois.
    Busy,
    /// O cliente já voltou ao jogo sozinho (o Roblox reconectou).
    AlreadyInGame,
    Failed(String),
    Stop(ReconnectStopReason, Option<String>),
}

/// Mudança que vira linha no Console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectNotice {
    Scheduled { user_id: i64, attempt: u32, in_ms: i64 },
    Offline { user_id: i64 },
    Relaunched { user_id: i64, attempt: u32 },
    Reconnected { user_id: i64 },
    BackByItself { user_id: i64 },
    GaveUp { user_id: i64, attempts: u32 },
    Stopped { user_id: i64, reason: ReconnectStopReason },
    /// O Auto Rejoin assumiu a conta, ou a opção foi desligada.
    Cancelled { user_id: i64 },
}

/// O que a Sessão mostra de cada conta.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectEntryView {
    pub user_id: i64,
    /// `waiting`, `waitingForInternet`, `launching`, `checking`, `gaveUp`, `stopped`.
    pub phase: &'static str,
    pub attempt: u32,
    pub max_attempts: u32,
    pub next_attempt_at_ms: Option<i64>,
    pub reason: Option<ReconnectStopReason>,
    pub error: Option<String>,
    pub drop: ClientDrop,
}

#[derive(Debug, Default)]
pub struct ReconnectBook {
    entries: HashMap<i64, ReconnectEntry>,
}

impl ReconnectBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// O monitor disse que a conta caiu.
    pub fn on_drop(&mut self, input: ReconnectDrop, now_ms: i64) -> Option<ReconnectNotice> {
        let user_id = input.user_id;
        // Auto Rejoin gerencia a conta: ele relança, a reconexão fica de fora.
        // Cliente do site: o app não fecha nem relança.
        if input.auto_rejoin || input.adopted {
            return None;
        }
        let joined_elsewhere = input.drop.reason == Some(DropReason::JoinedElsewhere);
        let watched_was_open = input.drop.kind != DropKind::Crashed;

        if let Some(entry) = self.entries.get_mut(&user_id) {
            let same_client = entry.watched_pid == input.pid;
            match entry.phase.clone() {
                // O cliente relançado caiu de novo: a tentativa não ficou.
                ReconnectPhase::Checking { .. } if same_client => {
                    entry.drop = input.drop.clone();
                    entry.watched_was_open = watched_was_open;
                    if joined_elsewhere {
                        entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::JoinedElsewhere);
                        return Some(ReconnectNotice::Stopped {
                            user_id,
                            reason: ReconnectStopReason::JoinedElsewhere,
                        });
                    }
                    return Some(Self::fail_attempt(entry, user_id, None, now_ms));
                }
                // Já está reconectando esta queda: só a "entrou em outro lugar"
                // (que pode chegar depois de outra linha) muda alguma coisa.
                phase if !phase.is_final() => {
                    if joined_elsewhere {
                        entry.drop = input.drop.clone();
                        entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::JoinedElsewhere);
                        return Some(ReconnectNotice::Stopped {
                            user_id,
                            reason: ReconnectStopReason::JoinedElsewhere,
                        });
                    }
                    return None;
                }
                // Estado final do mesmo cliente: nada muda até a pessoa agir.
                _ if same_client => return None,
                // Estado final de um cliente antigo: esta é uma queda nova.
                _ => {}
            }
        }

        if !input.enabled {
            self.entries.remove(&user_id);
            return None;
        }
        let stop = if joined_elsewhere {
            Some(ReconnectStopReason::JoinedElsewhere)
        } else if input.target.is_none() {
            Some(ReconnectStopReason::NoDestination)
        } else {
            None
        };
        let phase = match stop {
            Some(reason) => ReconnectPhase::Stopped(reason),
            None => ReconnectPhase::Waiting {
                at_ms: now_ms + reconnect_backoff_ms(1),
            },
        };
        self.entries.insert(
            user_id,
            ReconnectEntry {
                phase,
                attempt: 1,
                drop: input.drop,
                watched_pid: input.pid,
                watched_was_open,
                target: input.target,
                error: None,
            },
        );
        Some(match stop {
            Some(reason) => ReconnectNotice::Stopped { user_id, reason },
            None => ReconnectNotice::Scheduled {
                user_id,
                attempt: 1,
                in_ms: reconnect_backoff_ms(1),
            },
        })
    }

    /// A tentativa não ficou: agenda a próxima, ou desiste.
    fn fail_attempt(
        entry: &mut ReconnectEntry,
        user_id: i64,
        error: Option<String>,
        now_ms: i64,
    ) -> ReconnectNotice {
        if error.is_some() {
            entry.error = error;
        }
        if entry.attempt >= RECONNECT_MAX_ATTEMPTS {
            entry.phase = ReconnectPhase::GaveUp;
            return ReconnectNotice::GaveUp {
                user_id,
                attempts: entry.attempt,
            };
        }
        entry.attempt += 1;
        let in_ms = reconnect_backoff_ms(entry.attempt);
        entry.phase = ReconnectPhase::Waiting { at_ms: now_ms + in_ms };
        ReconnectNotice::Scheduled {
            user_id,
            attempt: entry.attempt,
            in_ms,
        }
    }

    /// Uma passada: confere cada conta e devolve as tentativas a disparar.
    pub fn tick(
        &mut self,
        now_ms: i64,
        observe: impl Fn(i64, Option<u32>) -> ReconnectObservation,
    ) -> (Vec<ReconnectAction>, Vec<ReconnectNotice>) {
        let mut actions = Vec::new();
        let mut notices = Vec::new();
        let mut remove = Vec::new();
        let mut ids: Vec<i64> = self.entries.keys().copied().collect();
        ids.sort_unstable();
        for user_id in ids {
            let Some(entry) = self.entries.get_mut(&user_id) else {
                continue;
            };
            if entry.phase.is_final() {
                continue;
            }
            let obs = observe(user_id, entry.watched_pid);
            // Auto Rejoin assumiu, ou a pessoa desligou a opção.
            if obs.auto_rejoin || !obs.enabled {
                if entry.phase != ReconnectPhase::Launching {
                    remove.push(user_id);
                    notices.push(ReconnectNotice::Cancelled { user_id });
                }
                continue;
            }
            match entry.phase.clone() {
                ReconnectPhase::Waiting { at_ms } | ReconnectPhase::WaitingForInternet { at_ms } => {
                    if obs.tracked_in_game {
                        // Voltou ao jogo sozinho (o Roblox reconectou), ou a
                        // pessoa abriu a conta de novo.
                        remove.push(user_id);
                        notices.push(ReconnectNotice::BackByItself { user_id });
                    } else if entry.watched_was_open && obs.watched_exited && !obs.watched_closed_by_app {
                        entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::ClosedByUser);
                        notices.push(ReconnectNotice::Stopped {
                            user_id,
                            reason: ReconnectStopReason::ClosedByUser,
                        });
                    } else if obs.tracked_adopted {
                        entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::OpenedOutsideApp);
                        notices.push(ReconnectNotice::Stopped {
                            user_id,
                            reason: ReconnectStopReason::OpenedOutsideApp,
                        });
                    } else if now_ms >= at_ms {
                        match entry.target.clone() {
                            Some(target) => {
                                entry.phase = ReconnectPhase::Launching;
                                actions.push(ReconnectAction::Attempt {
                                    user_id,
                                    attempt: entry.attempt,
                                    target,
                                });
                            }
                            None => {
                                entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::NoDestination);
                                notices.push(ReconnectNotice::Stopped {
                                    user_id,
                                    reason: ReconnectStopReason::NoDestination,
                                });
                            }
                        }
                    }
                }
                ReconnectPhase::Launching => {}
                ReconnectPhase::Checking {
                    launched_at_ms,
                    in_game_since_ms,
                } => {
                    let since = match (in_game_since_ms, obs.watched_in_game) {
                        (Some(t), _) => Some(t),
                        (None, true) => Some(now_ms),
                        (None, false) => None,
                    };
                    entry.phase = ReconnectPhase::Checking {
                        launched_at_ms,
                        in_game_since_ms: since,
                    };
                    if let Some(drop) = obs.watched_drop.clone() {
                        // O relançado caiu e o aviso passou enquanto a
                        // tentativa rodava: vale como se tivesse chegado agora.
                        entry.drop = drop.clone();
                        if drop.reason == Some(DropReason::JoinedElsewhere) {
                            entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::JoinedElsewhere);
                            notices.push(ReconnectNotice::Stopped {
                                user_id,
                                reason: ReconnectStopReason::JoinedElsewhere,
                            });
                        } else {
                            entry.watched_was_open = drop.kind != DropKind::Crashed;
                            notices.push(Self::fail_attempt(entry, user_id, None, now_ms));
                        }
                    } else if since.is_some_and(|t| now_ms - t >= RECONNECT_STABLE_MS) {
                        remove.push(user_id);
                        notices.push(ReconnectNotice::Reconnected { user_id });
                    } else if obs.watched_exited {
                        // A queda (inclusive "fechou sozinho") já chegou por
                        // `on_drop`; terminar sem queda é a pessoa fechando.
                        if obs.watched_closed_by_app {
                            let notice = Self::fail_attempt(
                                entry,
                                user_id,
                                Some(RECONNECT_ERROR_CLIENT_CLOSED.to_string()),
                                now_ms,
                            );
                            notices.push(notice);
                        } else {
                            entry.phase = ReconnectPhase::Stopped(ReconnectStopReason::ClosedByUser);
                            notices.push(ReconnectNotice::Stopped {
                                user_id,
                                reason: ReconnectStopReason::ClosedByUser,
                            });
                        }
                    } else if since.is_none() && now_ms - launched_at_ms >= RECONNECT_JOIN_TIMEOUT_MS {
                        if obs.watched_log_found {
                            let notice = Self::fail_attempt(
                                entry,
                                user_id,
                                Some(RECONNECT_ERROR_NOT_IN_GAME.to_string()),
                                now_ms,
                            );
                            notices.push(notice);
                        } else {
                            // Sem log não dá para ver o jogo: o cliente está
                            // aberto há 2 min, então conta como reconectado.
                            remove.push(user_id);
                            notices.push(ReconnectNotice::Reconnected { user_id });
                        }
                    }
                }
                ReconnectPhase::GaveUp | ReconnectPhase::Stopped(_) => {}
            }
        }
        for user_id in remove {
            self.entries.remove(&user_id);
        }
        (actions, notices)
    }

    /// O resultado de uma tentativa. Ignorado se a pessoa parou a reconexão
    /// enquanto ela rodava.
    pub fn attempt_finished(
        &mut self,
        user_id: i64,
        outcome: AttemptOutcome,
        now_ms: i64,
    ) -> Option<ReconnectNotice> {
        let entry = self.entries.get_mut(&user_id)?;
        if entry.phase != ReconnectPhase::Launching {
            return None;
        }
        match outcome {
            AttemptOutcome::Launched { pid } => {
                entry.watched_pid = Some(pid);
                entry.watched_was_open = true;
                entry.phase = ReconnectPhase::Checking {
                    launched_at_ms: now_ms,
                    in_game_since_ms: None,
                };
                Some(ReconnectNotice::Relaunched {
                    user_id,
                    attempt: entry.attempt,
                })
            }
            AttemptOutcome::NoInternet => {
                entry.phase = ReconnectPhase::WaitingForInternet {
                    at_ms: now_ms + RECONNECT_OFFLINE_RETRY_MS,
                };
                Some(ReconnectNotice::Offline { user_id })
            }
            AttemptOutcome::Busy => {
                entry.phase = ReconnectPhase::Waiting {
                    at_ms: now_ms + RECONNECT_BUSY_RETRY_MS,
                };
                None
            }
            AttemptOutcome::AlreadyInGame => {
                self.entries.remove(&user_id);
                Some(ReconnectNotice::BackByItself { user_id })
            }
            AttemptOutcome::Failed(error) => Some(Self::fail_attempt(entry, user_id, Some(error), now_ms)),
            AttemptOutcome::Stop(reason, error) => {
                entry.error = error;
                entry.phase = ReconnectPhase::Stopped(reason);
                Some(ReconnectNotice::Stopped { user_id, reason })
            }
        }
    }

    /// O monitor viu a conta voltar a um jogo depois da queda (o Roblox
    /// reconectou sozinho, ou a pessoa clicou em "Reconnect").
    pub fn on_recovered(&mut self, user_id: i64) -> Option<ReconnectNotice> {
        let entry = self.entries.get(&user_id)?;
        if matches!(
            entry.phase,
            ReconnectPhase::Waiting { .. } | ReconnectPhase::WaitingForInternet { .. }
        ) {
            self.entries.remove(&user_id);
            return Some(ReconnectNotice::BackByItself { user_id });
        }
        None
    }

    /// "Parar": esquece a reconexão desta conta (o cliente fica como está).
    pub fn stop(&mut self, user_id: i64) -> bool {
        match self.entries.get(&user_id) {
            // A tentativa em andamento termina; o resultado é ignorado.
            Some(_) => {
                self.entries.remove(&user_id);
                true
            }
            None => false,
        }
    }

    /// "Tentar agora" (e "Tentar de novo" depois de desistir). Conta banida
    /// não tem tentativa: o launch nem passaria.
    pub fn try_now(&mut self, user_id: i64, now_ms: i64) -> bool {
        let Some(entry) = self.entries.get_mut(&user_id) else {
            return false;
        };
        match entry.phase {
            ReconnectPhase::Waiting { .. } | ReconnectPhase::WaitingForInternet { .. } => {
                entry.phase = ReconnectPhase::Waiting { at_ms: now_ms };
                true
            }
            ReconnectPhase::GaveUp | ReconnectPhase::Stopped(_) => {
                if entry.phase == ReconnectPhase::Stopped(ReconnectStopReason::Banned) || entry.target.is_none() {
                    return false;
                }
                entry.attempt = 1;
                entry.error = None;
                entry.phase = ReconnectPhase::Waiting { at_ms: now_ms };
                true
            }
            ReconnectPhase::Launching | ReconnectPhase::Checking { .. } => false,
        }
    }

    /// Alguma reconexão em andamento (para manter o PC acordado).
    pub fn is_active(&self) -> bool {
        self.entries.values().any(|e| !e.phase.is_final())
    }

    pub fn views(&self) -> Vec<ReconnectEntryView> {
        let mut views: Vec<ReconnectEntryView> = self
            .entries
            .iter()
            .map(|(user_id, e)| {
                let (phase, next, reason) = match &e.phase {
                    ReconnectPhase::Waiting { at_ms } => ("waiting", Some(*at_ms), None),
                    ReconnectPhase::WaitingForInternet { at_ms } => ("waitingForInternet", Some(*at_ms), None),
                    ReconnectPhase::Launching => ("launching", None, None),
                    ReconnectPhase::Checking { .. } => ("checking", None, None),
                    ReconnectPhase::GaveUp => ("gaveUp", None, None),
                    ReconnectPhase::Stopped(reason) => ("stopped", None, Some(*reason)),
                };
                ReconnectEntryView {
                    user_id: *user_id,
                    phase,
                    attempt: e.attempt,
                    max_attempts: RECONNECT_MAX_ATTEMPTS,
                    next_attempt_at_ms: next,
                    reason,
                    error: e.error.clone(),
                    drop: e.drop.clone(),
                }
            })
            .collect();
        views.sort_by_key(|v| v.user_id);
        views
    }
}

/// O erro do launch, nos termos da reconexão: ban e sessão expirada param de
/// vez; o resto é uma tentativa que falhou.
pub fn attempt_outcome_for_launch_error(error: &str) -> AttemptOutcome {
    if error == LAUNCH_ALREADY_ACTIVE {
        return AttemptOutcome::Busy;
    }
    // "Skipped: …" é a frase do launch pulado por moderação (moderation.rs).
    if is_moderated_error(error) || error.starts_with("Skipped:") {
        return AttemptOutcome::Stop(ReconnectStopReason::Banned, Some(error.to_string()));
    }
    if is_auth_session_error(error) || error == session_relogin_error() {
        return AttemptOutcome::Stop(ReconnectStopReason::SessionExpired, Some(error.to_string()));
    }
    AttemptOutcome::Failed(error.to_string())
}

/// A conta tem a reconexão ligada? O campo da conta (`AutoReconnect`) manda;
/// sem ele, vale o padrão `General.AutoReconnect`. O `AutoRelaunch` do Nexus
/// liga também.
pub fn reconnect_enabled(account_field: Option<&str>, global_default: bool, nexus_auto_relaunch: bool) -> bool {
    let own = match account_field.map(str::trim) {
        Some("true") => true,
        Some("false") => false,
        _ => global_default,
    };
    own || nexus_auto_relaunch
}

/// Linha do Console (traduzível, como as do monitor de quedas).
fn reconnect_console_line(notice: &ReconnectNotice) -> (&'static str, LogMsg) {
    let max = RECONNECT_MAX_ATTEMPTS;
    match notice {
        ReconnectNotice::Scheduled { attempt, in_ms, .. } => (
            "info",
            log_msg!(
                "Automatic reconnection: attempt {{attempt}}/{{max}} in {{seconds}} s",
                attempt = *attempt,
                max = max,
                seconds = in_ms / 1_000
            ),
        ),
        ReconnectNotice::Offline { .. } => (
            "warn",
            log_msg!("No internet: the reconnection waits for it to come back"),
        ),
        ReconnectNotice::Relaunched { attempt, .. } => (
            "info",
            log_msg!(
                "Reconnecting (attempt {{attempt}}/{{max}}): checking that it stays in the game",
                attempt = *attempt,
                max = max
            ),
        ),
        ReconnectNotice::Reconnected { .. } => ("success", log_msg!("Reconnected: it stayed in the game")),
        ReconnectNotice::BackByItself { .. } => (
            "info",
            log_msg!("Back in the game on its own: reconnection not needed"),
        ),
        ReconnectNotice::GaveUp { attempts, .. } => (
            "error",
            log_msg!("Reconnection gave up after {{attempts}} attempts", attempts = *attempts),
        ),
        ReconnectNotice::Stopped { reason, .. } => (
            "warn",
            match reason {
                ReconnectStopReason::JoinedElsewhere => {
                    log_msg!("No reconnection: the account joined somewhere else")
                }
                ReconnectStopReason::ClosedByUser => log_msg!("No reconnection: the client was closed"),
                ReconnectStopReason::Banned => log_msg!("No reconnection: the account is banned"),
                ReconnectStopReason::SessionExpired => {
                    log_msg!("No reconnection: the session expired, sign in to the account again")
                }
                ReconnectStopReason::NoDestination => {
                    log_msg!("No reconnection: it is not known where to go back to")
                }
                ReconnectStopReason::OpenedOutsideApp => {
                    log_msg!("No reconnection: the account is open outside the app")
                }
            },
        ),
        ReconnectNotice::Cancelled { .. } => ("info", log_msg!("Reconnection cancelled")),
    }
}

fn reconnect_notice_user(notice: &ReconnectNotice) -> i64 {
    match notice {
        ReconnectNotice::Scheduled { user_id, .. }
        | ReconnectNotice::Offline { user_id }
        | ReconnectNotice::Relaunched { user_id, .. }
        | ReconnectNotice::Reconnected { user_id }
        | ReconnectNotice::BackByItself { user_id }
        | ReconnectNotice::GaveUp { user_id, .. }
        | ReconnectNotice::Stopped { user_id, .. }
        | ReconnectNotice::Cancelled { user_id } => *user_id,
    }
}

/// Onde o app mandou cada conta no último launch (só em memória).
static LAUNCHED_TARGETS: std::sync::LazyLock<std::sync::Mutex<HashMap<i64, LaunchedTarget>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// launch.rs chama isto quando o cliente da conta subiu.
pub(crate) fn remember_launch_target(user_id: i64, target: LaunchedTarget) {
    if let Ok(mut targets) = LAUNCHED_TARGETS.lock() {
        targets.insert(user_id, target);
    }
}

fn launched_target_of(user_id: i64) -> Option<LaunchedTarget> {
    LAUNCHED_TARGETS.lock().ok()?.get(&user_id).cloned()
}

/// Há internet até o Roblox? Qualquer resposta HTTP conta (até um 404): só
/// erro de rede é "sem internet".
async fn reachable_at(client: &reqwest::Client, url: &str) -> bool {
    client.head(url).send().await.is_ok()
}

async fn roblox_reachable() -> bool {
    let Ok(client) = api::http_client::builder_with(
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(8),
    )
    .build() else {
        return true;
    };
    reachable_at(&client, &format!("{}/", api::endpoints::host("www"))).await
}

static RECONNECT_BOOK: std::sync::LazyLock<std::sync::Mutex<ReconnectBook>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ReconnectBook::new()));

fn with_reconnect_book<R>(f: impl FnOnce(&mut ReconnectBook) -> R) -> R {
    let mut book = match RECONNECT_BOOK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut book)
}

/// Alguma reconexão em andamento (keep_awake.rs).
pub(crate) fn reconnect_is_active() -> bool {
    with_reconnect_book(|book| book.is_active())
}

/// Último retrato enviado à tela (para só emitir quando muda).
static RECONNECT_LAST_VIEWS: std::sync::LazyLock<std::sync::Mutex<Vec<ReconnectEntryView>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

fn publish_reconnect(app: &tauri::AppHandle, notices: &[ReconnectNotice]) {
    // Aviso no Discord: reconectou / desistiu (desligado por padrão; só fila).
    webhook_after_reconnect(app, notices);
    for notice in notices {
        // A conta relançada toca a gravação dela quando ficar o tempo
        // configurado no jogo (commands/recordings.rs).
        if let ReconnectNotice::Relaunched { user_id, .. } = notice {
            arm_recording_after_reconnect(*user_id, chrono::Utc::now().timestamp_millis());
        }
        let (level, line) = reconnect_console_line(notice);
        emit_launch_log(app, reconnect_notice_user(notice), level, "reconnect", line);
    }
    let views = with_reconnect_book(|book| book.views());
    let changed = match RECONNECT_LAST_VIEWS.lock() {
        Ok(mut last) => {
            let changed = *last != views;
            if changed {
                *last = views.clone();
            }
            changed
        }
        Err(_) => true,
    };
    let reconnected: Vec<i64> = notices
        .iter()
        .filter_map(|n| match n {
            ReconnectNotice::Reconnected { user_id } => Some(*user_id),
            _ => None,
        })
        .collect();
    if changed || !reconnected.is_empty() {
        let _ = app.emit(
            "auto-reconnect",
            serde_json::json!({ "entries": views, "reconnected": reconnected }),
        );
    }
}

#[tauri::command]
fn get_auto_reconnect_status() -> serde_json::Value {
    serde_json::json!({ "entries": with_reconnect_book(|book| book.views()) })
}

/// "Parar": a conta sai da reconexão; nenhum cliente é fechado.
#[tauri::command]
fn stop_auto_reconnect(app: tauri::AppHandle, user_id: i64) -> bool {
    let stopped = with_reconnect_book(|book| book.stop(user_id));
    if stopped {
        emit_launch_log(&app, user_id, "info", "reconnect", "Reconnection stopped by the user");
        publish_reconnect(&app, &[]);
    }
    stopped
}

/// "Tentar agora" / "Tentar de novo": a próxima passada (até 2 s) tenta.
#[tauri::command]
fn retry_auto_reconnect(app: tauri::AppHandle, user_id: i64) -> bool {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let ok = with_reconnect_book(|book| book.try_now(user_id, now_ms));
    if ok {
        publish_reconnect(&app, &[]);
    }
    ok
}

/// A conta é gerenciada pelo Auto Rejoin agora?
#[cfg(target_os = "windows")]
fn managed_by_auto_rejoin(status: &BottingStatusPayload, user_id: i64) -> bool {
    status.active && status.user_ids.contains(&user_id)
}

/// O que decide a opção de cada conta, lido uma vez por passada.
#[cfg(target_os = "windows")]
struct ReconnectSettings {
    global_default: bool,
    /// `AutoReconnect` de cada conta, quando a conta tem o campo.
    fields: HashMap<i64, String>,
    usernames: HashMap<i64, String>,
    nexus_auto_relaunch: HashSet<String>,
}

#[cfg(target_os = "windows")]
impl ReconnectSettings {
    fn load(app: &tauri::AppHandle) -> Self {
        let global_default = app.state::<SettingsStore>().get_bool("General", "AutoReconnect");
        let accounts = app.state::<AccountStore>().get_all().unwrap_or_default();
        let mut fields = HashMap::new();
        let mut usernames = HashMap::new();
        for account in accounts {
            if let Some(value) = account.fields.get("AutoReconnect") {
                fields.insert(account.user_id, value.clone());
            }
            usernames.insert(account.user_id, account.username.to_ascii_lowercase());
        }
        Self {
            global_default,
            fields,
            usernames,
            nexus_auto_relaunch: nexus_auto_relaunch_usernames(),
        }
    }

    fn enabled(&self, user_id: i64) -> bool {
        let nexus = self
            .usernames
            .get(&user_id)
            .is_some_and(|name| self.nexus_auto_relaunch.contains(name));
        reconnect_enabled(self.fields.get(&user_id).map(String::as_str), self.global_default, nexus)
    }

    /// Alguma conta com a opção ligada tem um cliente do app aberto: a
    /// reconexão está de guarda (para manter o PC acordado).
    fn guarding(&self, tracked: &[platform::windows::TrackedProcess]) -> bool {
        tracked.iter().any(|p| !p.adopted && self.enabled(p.user_id))
    }
}

/// Contas com `AutoRelaunch` ligado no Nexus (nome em minúsculas).
#[cfg(all(target_os = "windows", feature = "nexus"))]
fn nexus_auto_relaunch_usernames() -> HashSet<String> {
    nexus::websocket::nexus()
        .get_accounts()
        .into_iter()
        .filter(|a| a.auto_relaunch)
        .map(|a| a.username.to_ascii_lowercase())
        .collect()
}

#[cfg(all(target_os = "windows", not(feature = "nexus")))]
fn nexus_auto_relaunch_usernames() -> HashSet<String> {
    HashSet::new()
}

/// A reconexão está de guarda ou em andamento? (keep_awake.rs)
#[cfg(target_os = "windows")]
static RECONNECT_GUARDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "windows")]
pub(crate) fn reconnect_is_guarding() -> bool {
    RECONNECT_GUARDING.load(std::sync::atomic::Ordering::Relaxed) || reconnect_is_active()
}

/// Uma passada da reconexão, logo depois da do monitor de quedas: as quedas e
/// as voltas que ele avisou, e a conferência de cada conta.
#[cfg(target_os = "windows")]
pub(crate) fn reconnect_after_health_tick(app: &tauri::AppHandle, notices: &[HealthNotice]) {
    // Antes das saídas cedo abaixo: a gravação espera a conta ficar no jogo
    // mesmo depois de a reconexão já ter dado a conta por reconectada.
    recording_after_join_tick(app);
    let tracked = platform::windows::tracker().get_all();
    let has_work = with_reconnect_book(|book| !book.entries.is_empty())
        || notices.iter().any(|n| matches!(n, HealthNotice::Dropped { .. }));
    let any_ours = tracked.iter().any(|p| !p.adopted);
    if !has_work && !any_ours {
        RECONNECT_GUARDING.store(false, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    let settings = ReconnectSettings::load(app);
    RECONNECT_GUARDING.store(settings.guarding(&tracked), std::sync::atomic::Ordering::Relaxed);
    if !has_work {
        return;
    }
    let botting = current_botting_status();
    let views: HashMap<i64, ClientHealthView> = CLIENT_HEALTH_VIEWS
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut out = Vec::new();

    for notice in notices {
        match notice {
            HealthNotice::Dropped {
                user_id,
                drop,
                adopted,
            } => {
                let view = views.get(user_id);
                let target = reconnect_target(
                    launched_target_of(*user_id).as_ref(),
                    view.and_then(|v| v.destination.as_ref()),
                    drop.kind,
                );
                let input = ReconnectDrop {
                    user_id: *user_id,
                    pid: view.map(|v| v.pid),
                    adopted: *adopted,
                    drop: drop.clone(),
                    enabled: settings.enabled(*user_id),
                    auto_rejoin: managed_by_auto_rejoin(&botting, *user_id),
                    target,
                };
                if let Some(n) = with_reconnect_book(|book| book.on_drop(input, now_ms)) {
                    out.push(n);
                }
            }
            HealthNotice::Recovered { user_id } => {
                if let Some(n) = with_reconnect_book(|book| book.on_recovered(*user_id)) {
                    out.push(n);
                }
            }
            HealthNotice::NotResponding { .. } | HealthNotice::WrongAccount { .. } | HealthNotice::Challenge { .. } => {}
        }
    }

    let (actions, tick_notices) = with_reconnect_book(|book| {
        book.tick(now_ms, |user_id, watched| {
            let tracked_now = tracked.iter().find(|p| p.user_id == user_id);
            let view = views.get(&user_id);
            let tracked_view = tracked_now.and_then(|p| view.filter(|v| v.pid == p.pid));
            let watched_view = watched.and_then(|pid| view.filter(|v| v.pid == pid));
            let watched_exited = match (watched, watched_view) {
                (None, _) => false,
                (Some(_), Some(v)) => v.exited,
                // Sem retrato: só ainda não visto se for o cliente que o
                // tracker tem agora (acabou de abrir); senão ele já se foi.
                (Some(pid), None) => tracked_now.map(|p| p.pid) != Some(pid),
            };
            ReconnectObservation {
                enabled: settings.enabled(user_id),
                auto_rejoin: managed_by_auto_rejoin(&botting, user_id),
                tracked_pid: tracked_now.map(|p| p.pid),
                tracked_adopted: tracked_now.is_some_and(|p| p.adopted),
                tracked_in_game: tracked_view.is_some_and(|v| v.in_game),
                watched_exited,
                watched_closed_by_app: watched.is_some_and(platform::windows::was_terminated_by_app),
                watched_in_game: watched_view.is_some_and(|v| v.in_game),
                watched_log_found: watched_view.is_some_and(|v| v.log_found),
                watched_drop: watched_view.and_then(|v| v.drop.clone()),
            }
        })
    });
    out.extend(tick_notices);
    publish_reconnect(app, &out);

    for action in actions {
        let ReconnectAction::Attempt {
            user_id,
            attempt,
            target,
        } = action;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let outcome = run_reconnect_attempt(&app, user_id, attempt, &target).await;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let notice = with_reconnect_book(|book| book.attempt_finished(user_id, outcome, now_ms));
            publish_reconnect(&app, &notice.into_iter().collect::<Vec<_>>());
        });
    }
}

/// Uma tentativa: internet, ban, o cliente velho da própria conta e o launch
/// normal (pela fila, sem renovar a sessão).
#[cfg(target_os = "windows")]
async fn run_reconnect_attempt(
    app: &tauri::AppHandle,
    user_id: i64,
    attempt: u32,
    target: &ReconnectTarget,
) -> AttemptOutcome {
    if !roblox_reachable().await {
        return AttemptOutcome::NoInternet;
    }

    // Ban novo costuma vir logo depois de um kick: a 1ª tentativa pergunta de
    // novo ao Roblox (leitura, sem refresh). Falha da consulta não bloqueia.
    if attempt == 1 {
        forget_moderation(user_id);
    }
    let status = match cached_moderation(user_id, std::time::Instant::now()) {
        Some(status) => Some(status),
        None => fetch_moderation(
            app.state::<AccountStore>().inner(),
            Some(app),
            user_id,
            api::http_client::client(),
        )
        .await
        .ok(),
    };
    if let Some(message) = status.and_then(|s| moderation_block_message(&s, chrono::Utc::now())) {
        return AttemptOutcome::Stop(ReconnectStopReason::Banned, Some(message));
    }

    let tracker = platform::windows::tracker();
    if let Some(current) = tracker.get_all().into_iter().find(|p| p.user_id == user_id) {
        // Cliente do site nunca é fechado; o que voltou ao jogo sozinho fica.
        if current.adopted {
            return AttemptOutcome::Stop(ReconnectStopReason::OpenedOutsideApp, None);
        }
        if client_health_of(user_id, current.pid).is_some_and(|v| v.in_game) {
            return AttemptOutcome::AlreadyInGame;
        }
    }

    // A fila primeiro: com outro launch rodando, nada é fechado e a tentativa
    // volta logo depois, sem gastar a vez.
    let sequence = match launch_queue_start(app, &[user_id], target.place_id, &target.job_id) {
        Ok(sequence) => sequence,
        Err(_) => return AttemptOutcome::Busy,
    };
    emit_launch_log(
        app,
        user_id,
        "info",
        "reconnect",
        log_msg!(
            "Automatic reconnection: attempt {{attempt}}/{{max}}",
            attempt = attempt,
            max = RECONNECT_MAX_ATTEMPTS
        ),
    );

    // O cliente velho é da própria conta e do app (conferido acima): fecha só
    // ele, para o novo não brigar com a sessão caída.
    if let Some(current) = tracker.get_all().into_iter().find(|p| p.user_id == user_id) {
        if current.adopted {
            sequence.finish();
            return AttemptOutcome::Stop(ReconnectStopReason::OpenedOutsideApp, None);
        }
        if !tracker.kill_for_user_graceful_async(user_id, 4500).await {
            let error = RECONNECT_ERROR_OLD_CLIENT_OPEN.to_string();
            sequence.mark(user_id, LaunchQueueState::Failed, Some(error.clone()));
            sequence.finish();
            return AttemptOutcome::Failed(error);
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let result = launch_roblox_windows(
        app.clone(),
        &sequence,
        app.state::<AccountStore>(),
        app.state::<SettingsStore>(),
        app.state::<data::versions::VersionsCatalogStore>(),
        user_id,
        target.place_id,
        target.job_id.clone(),
        target.launch_data.clone(),
        false,
        target.join_vip,
        target.link_code.clone(),
        None,
        false,
    )
    .await;
    if let Err(error) = &result {
        sequence.mark(user_id, LaunchQueueState::Failed, Some(error.clone()));
    }
    sequence.finish();
    drop(sequence);
    match result {
        Ok(()) => match tracker.get_all().into_iter().find(|p| p.user_id == user_id) {
            Some(p) if !p.adopted => AttemptOutcome::Launched { pid: p.pid },
            _ => AttemptOutcome::Failed(RECONNECT_ERROR_DID_NOT_START.to_string()),
        },
        Err(error) => attempt_outcome_for_launch_error(&error),
    }
}
