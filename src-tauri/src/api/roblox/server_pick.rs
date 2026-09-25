// Escolha automática do servidor público de um lote.
//
// Antes só existia o `shuffleJob` (servidor aleatório). Agora o usuário escolhe
// uma preferência e ela vale para o **lote inteiro**: o servidor é resolvido
// UMA vez e todas as contas selecionadas entram nele — senão cada conta cairia
// num servidor diferente e o lote deixaria de jogar junto.
//
// A ordenação vem da própria API (confirmado contra ela):
// `sortOrder=Asc` devolve os servidores com MENOS jogadores primeiro e `Desc`
// com mais. `excludeFullGames=true` tira os lotados.

/// Folga deixada depois que o lote entra.
///
/// "Mais cheio que caiba" sem folga significa entrar num servidor que fica
/// lotado no instante seguinte — qualquer jogador que tente entrar depois
/// (inclusive uma conta que caiu e voltou) não consegue. Uma vaga de sobra
/// resolve isso sem esvaziar o servidor.
pub const FREE_SEAT_BUFFER: i32 = 1;

/// O que o usuário escolheu na UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPreference {
    /// Não escolhe nada: Job ID vazio e o Roblox decide (comportamento antigo).
    None,
    /// O mais cheio que ainda caiba o lote **com uma vaga de folga**. É o
    /// padrão: joga junto de outras pessoas sem arriscar deixar conta de fora.
    BestFit,
    /// Qualquer servidor com vaga (comportamento antigo do `shuffleJob`).
    Random,
    /// Menos jogadores primeiro.
    Emptiest,
    /// Mais jogadores primeiro (sem contar os lotados).
    Fullest,
}

impl ServerPreference {
    /// `sortOrder` que a API deve usar para já vir na ordem certa.
    ///
    /// `BestFit` pede `Desc` porque a busca desce dos mais cheios até achar a
    /// faixa que cabe o lote; pedir `Asc` traria milhares de servidores vazios
    /// antes de chegar lá.
    pub fn sort_order(self) -> &'static str {
        match self {
            ServerPreference::Fullest | ServerPreference::BestFit => "Desc",
            _ => "Asc",
        }
    }
}

/// Texto da UI → preferência. Desconhecido (e vazio) vira `BestFit`, que é o
/// padrão — nenhuma preferência inválida pode impedir o launch.
///
/// `"default"` era o nome antigo de "não escolhe nada" e hoje significa o
/// padrão novo; quem quer o comportamento antigo escolhe `"none"`.
pub fn parse_server_preference(value: &str) -> ServerPreference {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" | "off" => ServerPreference::None,
        "random" | "shuffle" => ServerPreference::Random,
        "emptiest" | "empty" | "fewest" => ServerPreference::Emptiest,
        "fullest" | "full" | "most" => ServerPreference::Fullest,
        _ => ServerPreference::BestFit,
    }
}

/// Quantos jogadores um servidor pode ter para o lote caber com folga.
///
/// Nunca negativo: um servidor de 4 lugares com um lote de 8 daria `-5` e
/// qualquer comparação com isso viraria bug.
pub fn best_fit_cap(max_players: i32, accounts: usize) -> i32 {
    (max_players - accounts.max(1) as i32 - FREE_SEAT_BUFFER).max(0)
}

/// O servidor cabe o lote **e** deixa a folga?
pub fn fits_with_buffer(server: &ServerData, accounts: usize) -> bool {
    server.max_players > 0 && server.playing <= best_fit_cap(server.max_players, accounts)
}

/// Chave de ordenação do "melhor encaixe" — menor é melhor.
///
/// A folga ideal depois do lote entrar é **uma vaga** ([`FREE_SEAT_BUFFER`]):
/// entra num servidor com gente, sem lotá-lo no instante seguinte. A partir
/// daí a nota piora conforme a distância dessa folga, para os dois lados —
/// encher o servidor é ruim, e um servidor quase vazio também.
///
/// Empate vai para o mais cheio, que é o servidor mais vivo.
pub fn best_fit_score(server: &ServerData, accounts: usize) -> (i32, i32, i32) {
    if server.max_players <= 0 {
        return (i32::MAX, 0, 0);
    }
    let free = (server.max_players - server.playing).max(0);
    let slack = free - accounts.max(1) as i32;
    if slack >= 0 {
        // Cabe: distância da folga ideal, depois o mais cheio.
        (0, (slack - FREE_SEAT_BUFFER).abs(), slack)
    } else {
        // Não cabe: quem leva mais contas primeiro, depois o mais cheio.
        (1, -free, -server.playing)
    }
}

/// Servidores que cabem o lote inteiro, na ordem da preferência.
///
/// Regras:
/// - só entra servidor com vaga para **todas** as contas (`playing + contas <=
///   maxPlayers`) — mandar 8 contas para um servidor com 2 vagas deixaria 6 de
///   fora, cada uma num servidor aleatório;
/// - se nenhum couber inteiro, cai para os que têm **alguma** vaga, senão o
///   launch ficaria travado num jogo cheio;
/// - a ordenação é refeita aqui e não é assumida da API: a resposta vem
///   ordenada, mas paginação e cache do Roblox já devolveram listas fora de
///   ordem.
pub fn rank_servers(
    servers: &[ServerData],
    preference: ServerPreference,
    accounts: usize,
) -> Vec<&ServerData> {
    let needed = accounts.max(1) as i32;
    let fits_all: Vec<&ServerData> = servers
        .iter()
        .filter(|s| s.max_players > 0 && s.playing + needed <= s.max_players)
        .collect();

    if fits_all.is_empty() {
        // Nenhum servidor cabe o lote inteiro. Aqui "mais cheio" e "mais
        // vazio" não querem dizer nada: o que importa é **quantas contas
        // cabem**, então o topo é quem tem mais vagas. Sem isso a lista
        // mostrava primeiro os de 12/13 (uma vaga) e o usuário tinha que
        // descer a rolagem para achar os que levavam quatro contas.
        let mut partial: Vec<&ServerData> = servers
            .iter()
            .filter(|s| s.max_players > 0 && s.playing < s.max_players)
            .collect();
        partial.sort_by_key(|s| best_fit_score(s, accounts));
        return partial;
    }

    let mut candidates = fits_all;

    match preference {
        ServerPreference::Emptiest => candidates.sort_by_key(|s| s.playing),
        ServerPreference::Fullest => candidates.sort_by_key(|s| -s.playing),
        // Pontuação: a vaga de folga ideal é **uma**. Quanto mais longe disso
        // (encher o servidor ou entrar num quase vazio), pior; empate vai para
        // o mais cheio, que é o servidor mais vivo.
        ServerPreference::BestFit => candidates.sort_by_key(|s| best_fit_score(s, accounts)),
        // Aleatório e "não escolhe" mantêm a ordem da API; quem sorteia é
        // `pick_from_list`.
        ServerPreference::Random | ServerPreference::None => {}
    }
    candidates
}

/// Escolhe um servidor da lista. Puro (o `nanos` é a fonte de aleatoriedade),
/// para poder ser testado sem rede nem relógio.
pub fn pick_from_list(
    servers: &[ServerData],
    preference: ServerPreference,
    accounts: usize,
    nanos: u128,
) -> Option<String> {
    let ranked = rank_servers(servers, preference, accounts);
    let chosen = match preference {
        ServerPreference::Random => ranked.get((nanos as usize) % ranked.len().max(1))?,
        _ => ranked.first()?,
    };
    Some(chosen.id.clone())
}

/// O servidor escolhido, com o que a UI precisa dizer ao usuário.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedServer {
    pub job_id: String,
    pub playing: i32,
    pub max_players: i32,
    /// Preenchido só quando houve filtro de região.
    pub region: Option<ServerRegion>,
    /// `true` quando o filtro de região não achou ninguém e a escolha caiu no
    /// melhor servidor disponível. A UI pergunta antes de entrar.
    pub region_fallback: bool,
}

fn picked_from(server: &ServerData, region: Option<ServerRegion>, fallback: bool) -> PickedServer {
    PickedServer {
        job_id: server.id.clone(),
        playing: server.playing,
        max_players: server.max_players,
        region,
        region_fallback: fallback,
    }
}

/// Escolhe o servidor do lote, opcionalmente exigindo um país.
///
/// Sem filtro de região é uma única chamada à lista de servidores. Com filtro,
/// percorre os candidatos **na ordem da preferência** resolvendo a região de um
/// por vez (cada um custa um `join-game-instance`) e para no primeiro que bate
/// — achar o servidor certo cedo é o caso comum, e o teto de `max_lookups`
/// impede que uma região inexistente vire uma varredura infinita.
pub async fn pick_server(
    security_token: &str,
    place_id: i64,
    preference: ServerPreference,
    accounts: usize,
    country_code: &str,
    region_template: &str,
    max_lookups: usize,
) -> Result<PickedServer, String> {
    let servers = collect_servers_for(
        place_id,
        Some(security_token),
        preference,
        accounts,
        BEST_FIT_MAX_PAGES,
    )
    .await?;
    let ranked = rank_servers(&servers, preference, accounts);
    if ranked.is_empty() {
        return Err("No public server with room was found".to_string());
    }

    let wanted = country_code.trim();
    if wanted.is_empty() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let chosen = match preference {
            ServerPreference::Random | ServerPreference::None => {
                ranked[(nanos as usize) % ranked.len()]
            }
            _ => ranked[0],
        };
        return Ok(picked_from(chosen, None, false));
    }

    let budget = max_lookups.min(REGION_LOOKUP_MAX).max(1);
    for server in ranked.iter().take(budget) {
        let region =
            resolve_server_region(security_token, place_id, &server.id, region_template).await;
        if region_matches(&region, wanted) {
            return Ok(picked_from(server, Some(region), false));
        }
        tokio::time::sleep(std::time::Duration::from_millis(REGION_LOOKUP_DELAY_MS)).await;
    }

    // Nada na região pedida dentro do teto: devolve o melhor disponível
    // marcado como fallback. Quem decide se entra assim é o usuário.
    Ok(picked_from(ranked[0], None, true))
}

/// Páginas que a busca do "melhor encaixe" percorre antes de desistir.
pub const BEST_FIT_MAX_PAGES: usize = 6;

/// Junta servidores até achar os que cabem o lote com folga.
///
/// Com `Desc` a primeira página traz os mais cheios, e num jogo popular eles
/// estão todos com uma ou duas vagas. Descer as páginas chega na faixa em que o
/// lote cabe. Sem isso, "melhor encaixe" nunca via um servidor utilizável num
/// jogo grande — e cair no `Asc` traria os servidores vazios, que é justamente
/// o que o usuário não quer.
///
/// As outras preferências pedem uma página só: a ordem da API já entrega o que
/// elas querem no topo.
pub async fn collect_servers_for(
    place_id: i64,
    security_token: Option<&str>,
    preference: ServerPreference,
    accounts: usize,
    max_pages: usize,
) -> Result<Vec<ServerData>, String> {
    let pages = if preference == ServerPreference::BestFit {
        max_pages.clamp(1, BEST_FIT_MAX_PAGES)
    } else {
        1
    };

    let mut all: Vec<ServerData> = Vec::new();
    let mut cursor: Option<String> = None;

    for _ in 0..pages {
        let page = get_servers_page(
            place_id,
            "Public",
            cursor.as_deref(),
            security_token,
            preference.sort_order(),
            true,
        )
        .await?;
        let next = page.next_page_cursor.clone();
        all.extend(page.data);

        if preference != ServerPreference::BestFit {
            break;
        }
        if all.iter().any(|s| fits_with_buffer(s, accounts)) {
            break;
        }
        match next {
            Some(c) if !c.trim().is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all)
}

/// Atalho para a lista pública já ordenada e sem servidores lotados.
pub async fn get_servers_sorted(
    place_id: i64,
    sort_order: &str,
    security_token: Option<&str>,
) -> Result<ServersResponse, String> {
    get_servers_page(place_id, "Public", None, security_token, sort_order, true).await
}
