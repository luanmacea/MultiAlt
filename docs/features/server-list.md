# Server List, favoritos, recentes e servidores VIP

## Objetivo

Permitir que o usuário encontre um jogo (busca/descoberta), veja os servidores públicos de um Place, descubra em que servidor um jogador está, veja a região de um servidor, e entre com uma ou várias contas — inclusive em servidores VIP/privados salvos como favoritos.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Diálogo "Server List" (abas) | [ServerListDialog.tsx](../../src/components/server-list/ServerListDialog.tsx), [TabBar.tsx](../../src/components/server-list/TabBar.tsx) |
| Lista de servidores / busca de jogador / região | [ServersTab.tsx](../../src/components/server-list/ServersTab.tsx), [ServerContextMenu.tsx](../../src/components/server-list/ServerContextMenu.tsx) |
| Busca de jogos | [GamesTab.tsx](../../src/components/server-list/GamesTab.tsx), [GameContextMenu.tsx](../../src/components/server-list/GameContextMenu.tsx) |
| Favoritos + VIPs | [FavoritesTab.tsx](../../src/components/server-list/FavoritesTab.tsx), [FavoriteContextMenu.tsx](../../src/components/server-list/FavoriteContextMenu.tsx) |
| Recentes | [RecentTab.tsx](../../src/components/server-list/RecentTab.tsx), [RecentGamesList.tsx](../../src/components/server-list/RecentGamesList.tsx), [RecentGamesPopover.tsx](../../src/components/server-list/RecentGamesPopover.tsx), [RecentJobsList.tsx](../../src/components/server-list/RecentJobsList.tsx) |
| Tipos + persistência local (favoritos/recentes) | [server-list/types.ts](../../src/components/server-list/types.ts) |
| Reuso na tela de launch em lote | [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) |
| API de jogos/servidores | [api/roblox/avatar_games.rs](../../src-tauri/src/api/roblox/avatar_games.rs) (`get_servers`, `get_place_details`, `search_games`, `join_game_instance`, `get_universe_places`) |
| Links privados / share links | [api/roblox/private_links.rs](../../src-tauri/src/api/roblox/private_links.rs) |
| Resolução do alvo VIP no launch | [commands/launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) (`resolve_launch_job`, `resolve_private_join`) |
| Comandos Tauri | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs) (`get_servers`, `get_place_details`, `search_games`, `join_game_instance`, `parse_private_server_link_code`) |
| Ícones em lote e identidade do jogo | [commands/image_cache.rs](../../src-tauri/src/commands/image_cache.rs), [api/batch.rs](../../src-tauri/src/api/batch.rs) (`batched_get_game_icon`, `batched_get_game_info`) |

## Fluxo

### Abrir o Server List

1. Aberto pelo sidebar de conta ou por "Other Batch Tools" na tela Choose Game (`store.setServerListOpen(true)`).
2. Ao abrir, copia `store.placeId` para o estado local e dispara `refreshOnOpenSignal` → a aba Servers recarrega.
3. A conta usada para cookies/joins é `store.selectedAccount` (só existe com **exatamente uma** conta selecionada).

### Aba Servers

1. `get_place_details([placeId])` para exibir o nome.
2. Pagina `get_servers(placeId, "Public", cursor, userId)` até `nextPageCursor` acabar (ou o usuário cancelar), mostrando progressivamente.
3. Duplo clique / "Join Server": se o servidor tem `accessCode`, entra com `VIP:<accessCode>`; senão com `server.id` (Job ID). O handler grava `jobId`/`placeId` na store, confirma se a conta está online (`useJoinOnlineWarning`) e chama `store.joinServer(userId)`.
4. Campo manual "Job ID or private server link (optional)" aceita Job ID, `vip:<código>`, link com `privateServerLinkCode`, share link etc.
5. **Load Region**: `invoke("get_server_regions", { userId, placeId: teleportPlaceId || placeId, jobIds: [id] })` — o **backend** faz o `join-game-instance` para pegar o IP da máquina, geolocaliza esse IP e guarda em cache (memória + `ServerRegionCache.json`) — ver [server-choice.md](server-choice.md); a tela mostra o `label` devolvido, ou o `error`. Era um `fetch` do frontend direto ao `ipapi.co`, que quebrou (o frontend não fala com a rede, e o serviço passou a exigir desafio do Cloudflare).
6. **Find player**: `lookup_user` → headshot 48x48 do alvo → percorre páginas de servidores públicos pedindo headshots dos `playerTokens` via `batch_thumbnails` → compara URLs; ao achar, filtra a tabela para aquele servidor.

### Aba Games

1. Sem termo: `search_games("")` usa `explore-api/v1/get-sorts` (sorts de descoberta). Com termo: `search-api/omni-search`, filtrando `contentType === "Game"`. Debounce de 400 ms.
2. Remove duplicados por `universeId || placeId`; `likeRatio` = upvotes/(up+down).
3. Busca ícones com `batched_get_game_icon` para os 20 primeiros.
4. Selecionar um jogo → vira o Place atual e volta para a aba Servers; "Join Game" chama `launch_roblox` direto (sem Job ID); "Favorite" pede um nome e adiciona aos favoritos.

### Favoritos e VIPs

1. Guardados em `localStorage["ram_favorite_games"]` como `FavoriteGame { placeId, name, iconUrl, addedAt, vipServers[] }`.
   - O ícone é gravado junto quando o favorito é salvo. Favorito salvo **sem** ícone é completado ao abrir a aba (`loadGameIdentity`, uma passada por montagem, resultado gravado de volta): antes ficava com o quadrado vazio para sempre, ao lado de uma aba Games que mostra o ícone de todos. A releitura na hora de gravar evita ressuscitar favorito removido enquanto os ícones vinham.
2. "Add VIP Server" pede o link/código e um rótulo (default `VIP <n>`), gera `id` com `crypto.randomUUID()` (fallback timestamp+random).
3. Clicar num VIP → `onSelectGame(placeId, vip.link)`: no Server List pré-preenche o campo de Job ID; na Choose Game lança **todas as contas selecionadas** direto com `jobId = vip.link`.
4. Remover VIP filtra pelo `id`; renomear/remover favorito pelo menu de contexto.

### Recentes

1. `recordRecentGame(placeId, userId, maxCount)` é chamado ao selecionar jogo no Server List ([ServerListDialog.tsx](../../src/components/server-list/ServerListDialog.tsx)) e pela store ([store.tsx](../../src/store.tsx)) após um `joinServer`/`launchMultiple` **bem-sucedido** (qualquer origem, incl. Choose Game). Launch que falha não entra nos recentes.
2. Insere otimisticamente no topo (nome = placeId se desconhecido), remove duplicata do mesmo placeId e corta em `MaxRecentGames`.
3. Em seguida resolve nome e ícone (`batched_get_game_info`, uma chamada) e atualiza a entrada. `RecentGamesList` também completa entradas antigas sem nome/ícone ao exibir.
4. Persistência em `localStorage["ram_recent_games"]`.
5. Na linha (e no menu de contexto), **clicar no card abre os servidores** do jogo e **"Join Game" entra** com as contas selecionadas — as mesmas ações da aba Games, nas duas telas (Choose Game e Server List). Até 27/09/2026 o "Join Game" dos Recentes chamava o mesmo `onSelect` do card e só abria os servidores, embora a dica da aba prometesse entrar direto. No popover de escolha de jogo (`RecentGamesPopover`, sem `onJoinGame`), "Join Game" continua sendo escolher.

#### Servidores recentes (Job IDs)

Ao lado dos jogos recentes, a aba Recent do Server List mostra os **servidores** em que as contas entraram — voltar ao mesmo servidor exigia ter copiado o Job ID antes.

1. `addRecentJob(raw, placeId, maxCount, userIds)` ([types.ts](../../src/components/server-list/types.ts)) é chamado pela store depois de um `joinServer`/`launchMultiple` **bem-sucedido**, junto com `recordRecentGame`. Launch que falha não entra. A chamada é **envolvida em `try/catch`**: ela grava no `localStorage`, que o WebView recusa com a cota cheia ou o perfil sem storage, e isso acontece depois de o cliente já ter subido — sem a guarda, a exceção cairia no `catch` do launch e a tela diria "Launch failed" sobre um launch que deu certo (no lote, o `catch` ainda **relança**). Guardar recentes é conveniência; não derruba launch.
2. Guarda o alvo **como o launch o usou** (`raw`), no vocabulário que o campo de Job ID aceita de volta: Job ID público cru, ou `vip:<código>` quando o alvo é privado. Não é o texto colado: link privado é **normalizado** para `vip:<código>` pela store antes de gravar — num alvo VIP o Job ID vai vazio e o código viaja em `linkCode`, então guardar o Job ID cru perderia o servidor. O `resolve_launch_job` chega ao mesmo `link_code` pelas duas formas, e a forma normalizada ainda classifica como privada, que é o lado seguro do erro. Por isso o `kind: "link"` quase não é alcançado pela store: ele cobre o que já está gravado e quem grave o link cru.
3. `classifyJobInput` marca cada entrada como `job` (público), `vip` ou `link` — para o rótulo da linha e para a regra de visibilidade abaixo. Ele procura o código do link **também no texto decodificado**: o link curto do AppsFlyer carrega a query de verdade dentro do `af_dp` (`…?af_dp=roblox%3A%2F%2F…%3Fcode%3DDEADBEEF`), e sem decodificar ele passaria por `job` — ou seja, um alvo privado visível para todas as contas, com o código do dono à mostra. Errar para "público" é o erro caro.
4. Sem duplicata (a chave é o `raw`), mais recente no topo, cortada em `General.MaxRecentJobs`. `userIds` **soma** as contas que já usaram aquele alvo (um launch em lote registra todas).
5. Clicar preenche o Place e o Job ID e volta para a aba Servers — **não entra**. Entrar é o gesto seguinte, com o aviso de conta online.
6. Persistência em `localStorage["ram_recent_jobs"]`.
7. "Clear all" apaga **só o que aquela conta vê** (ver a regra de visibilidade): apagar entrada que não está na tela é surpresa, não limpeza.

### Ações do jogo pelo clique direito

Toda lista de jogos (Games, Favoritos, Recentes) abre um menu com **o que se pode fazer com aquele jogo**. O motivo: funcionalidades como o Auto Rejoin só podiam ser usadas abrindo a tela delas e **colando o Place ID à mão** — a ação agora já sabe de que jogo se trata.

| Item | O que faz | Onde aparece |
|---|---|---|
| Join Game | Lança o jogo (comportamento antigo) | sempre |
| Browse servers | Vai para a aba Servers com o place preenchido | quando a tela dona passa `onBrowseServers` |
| Favorite / Rename / Remove | Gerência do favorito | Games (favoritar) e Favoritos |
| Auto Rejoin | Abre o Auto Rejoin **com aquele jogo** | Choose Game e Server List |
| Scripts | Abre a página Scripts com aquele place como place atual (é o que `ram.window` expõe) | Choose Game e Server List |
| Copy Place ID | Copia o número | sempre |

Regras:

- **Ação sem callback não aparece** — item morto é pior que item ausente. As duas telas donas passam o menu **inteiro**: o Server List já foi a metade sem Browse servers/Auto Rejoin/Scripts (e com o Favorite dos Recentes morto), enquanto a Choose Game tinha o menu e não a coluna de servidores recentes. No Server List, o Auto Rejoin abre por cima do diálogo (z-[70]) e fechar volta a ele; Scripts é uma página, então o Server List fecha para ela aparecer; Browse servers faz o mesmo que o clique no card (aba Servers com o place, Job ID limpo), sem gravar o jogo nos recentes, como na Choose Game.
- A lista de **Recentes** não tinha clique direito nenhum — ganhou o mesmo `GameContextMenu` das outras.
- Abrir uma tela sobre o jogo **não entra no jogo**: nenhuma dessas ações lança cliente (travado por teste nas três listas).
- Para o Auto Rejoin o place vai **explícito na abertura** (`openBottingDialog(placeId)`), e não só por `store.placeId`: o rascunho salvo (`General.BottingDraftPlaceId`) vence a store, então sem isso o usuário escolhia um jogo e via outro. Abrir o Auto Rejoin **sem** jogo (barra de ações, toolbar) limpa o jogo da abertura anterior.

### Identificação do jogo pelo Place ID

Padrão do app: **toda tela que trabalha com um Place ID mostra qual jogo é aquele**, sempre que der para descobrir. Um número de 10 dígitos não informa nada, e telas de lote (aba Servers, barra de launch, Auto Rejoin) agem sobre várias contas de uma vez — entrar no jogo errado por um número copiado torto é caro.

1. O caminho único é o hook [useGameIdentity.ts](../../src/hooks/useGameIdentity.ts): recebe o texto do campo (número **ou** link do jogo colado) e devolve `{ placeId, name, iconUrl, loading }`.
2. Ele resolve nome e ícone num **comando só** (`batched_get_game_info`), com **cache de módulo por place** lido de forma síncrona — a segunda tela que abre o mesmo jogo já nasce com o nome, sem piscar — e **dedupe** das chamadas em voo. Do outro lado, o backend também guarda: o corpo de `multiget-place-details` traz nome e `universeId` juntos e o nome era descartado, então quem queria o nome pagava `get_place_details`, que não tem cache nenhum. Nome ausente é guardado como string vazia de propósito — é o registro de "já perguntei", e sem ele a tela perguntaria de novo a cada abertura.
3. Espera 400 ms de digitação parada antes de perguntar (`6`, `60`, `606`… não são places), descarta resposta que chega depois de o usuário trocar de place, e marca como "não sei" o place que falhou (nova tentativa só depois de 30 s, para queda de rede não virar laço de requisições).
4. Quem desenha é [GameBadge.tsx](../../src/components/ui/GameBadge.tsx), puramente visual: **sem nome e sem ícone não desenha nada** — "Place 606849621" não informa mais que o número já visível no campo ao lado.
5. Telas ligadas hoje: aba Servers da Choose Game, Auto Rejoin (os dois layouts) e Nexus. Games/Favoritos/Recentes já mostravam nome e ícone pelo caminho próprio das listas.

### Resolução do alvo VIP/privado no launch (backend)

`resolve_launch_job(job_id, join_vip, link_code)` ([launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs)):

1. Extrai link code do parâmetro explícito `link_code` (aceita `vip:`, `privateServerLinkCode=`, `linkCode=`, `code=` em share links; decodifica URL).
2. Se o Job ID começa com `vip:` (case-insensitive) → `join_vip = true` e o resto vira o Job ID.
3. Sem link code ainda → tenta extrair do próprio Job ID.
4. `join_vip` sem link code: se Job ID vazio → desliga VIP; senão o Job ID decodificado vira o link code.

`resolve_private_join(cookie, place_id, launch)`:

1. Se há link code e o Job ID é uma URL com `/games/<id>`, usa esse Place ID.
2. Se o valor parece **share link** (`/share?`, `/share-links`, `navigation/share_links`, `type=server`, `pid=server`) ou **código de share** (32 hex com pelo menos uma letra) → `resolve_share_server_link` (`POST apis.roblox.com/sharelinks/v1/resolve-link`) devolve `linkCode` e Place ID (via `placeId` ou `rootPlaceId` do `universeId`).
3. Se o valor parece **access code** (5 blocos separados por `-`, alfanuméricos/`_`) → é tratado como access code e o link code é limpo.
4. `use_private_join = join_vip || link_code != "" || access_code != ""`.

O comando `parse_private_server_link_code(userId, placeId, linkCode)` ([private_links.rs](../../src-tauri/src/api/roblox/private_links.rs)) tenta converter link code em access code baixando páginas candidatas (`/games/<id>?privateServerLinkCode=`, `/share-links?code=`, `/share?code=`, em `www` e `web.roblox.com`) e procurando `Roblox.GameLauncher.joinPrivateGame(`, `"accessCode":"` e variantes.

## Regras de negócio

- Aceitos como alvo privado: `vip:<código>` (ou `VIP:`), URL com `privateServerLinkCode=`, `linkCode=`, share link Roblox (`share?code=...&type=Server`), código de share de 32 hex, ou access code no formato `xxxx-xxxx-xxxx-xxxx-xxxx`.
- `get_servers` usa `limit=100` para públicos e `25` para `VIP`, sempre `sortOrder=Asc`; o cookie só é enviado se `userId` for passado.
- A aba Servers só lista servidores **públicos** (`serverType: "Public"`).
- Load Region e Find Player exigem Place ID; Load Region também exige uma conta selecionada.
- Favoritos: um favorito por `placeId` ("Already in favorites"); nome customizado é obrigatório.
- Migração: favorito antigo com `privateServer` (string única) é convertido em `vipServers: [{ name: "VIP", link }]` ao carregar; ao adicionar VIP o campo antigo é removido.
- Recentes: no máximo `General.MaxRecentGames` (default 8), mais recente primeiro, sem duplicatas.
- **Servidor privado recente não aparece para outra conta.** `visibleRecentJobs(entries, userId)` mostra Job ID **público** para qualquer conta (é o mesmo servidor que a aba Servers lista para todo mundo), mas alvo `vip`/`link` só para as contas que já entraram por ele. Um link VIP vale para quem o tem: mostrá-lo na lista de outra conta entregaria o servidor privado de uma conta a outra sem o dono pedir. Sem conta selecionada, só os públicos aparecem.
- A coluna de servidores recentes **só aparece onde há campo de Job ID** para preencher (hoje o Server List). A aba Recent da Choose Game não passa `onSelectJob` e continua mostrando só os jogos — ação sem destino é pior que ação ausente. Conferido de novo no checkup: a Choose Game não tem onde o valor cair — a aba Servers dela só tem Place ID (o clique num servidor já **entra**), o campo de link da aba Follow passa por `resolve_join_link`, que recusa Job ID solto e `vip:<código>` sem place, e o campo de Job ID da sidebar não está na tela enquanto a Choose Game está aberta.
- Tela nova que aceite Place ID usa `useGameIdentity` + `GameBadge` em vez de resolver nome/ícone por conta: era assim antes (cada tela com seu jeito, sem cache) e a maioria simplesmente não mostrava jogo nenhum.
- Na Choose Game, o alvo (`placeId`/`jobId`) é passado **explicitamente** para `joinServer`/`launchMultiple` — o comentário no código explica que ler da store causava entrar no VIP do jogo anterior.

## Configurações relacionadas

| Seção.Chave | Default | Efeito |
|---|---|---|
| `General.MaxRecentGames` | `8` | Tamanho da lista de jogos recentes. |
| `General.MaxRecentJobs` | `12` | Tamanho da lista de servidores recentes (Job IDs). |
| `General.WarnOnOnlineJoin` | `true` | Confirmação antes de entrar com conta já online. |
| `General.SavedPlaceId`, `SavedJobId`, `SavedLaunchData` | — | Últimos valores digitados, restaurados no startup. |
| `General.ServerRegionFormat` | `<city>, <countryCode>` | Exibido/editável em Settings → General (ver armadilhas). |
| `General.ShuffleJobId` | `false` | Opção "Shuffle Job ID" (Misc). |

## Armadilhas / cuidados

- **A lista de servidores recentes guarda código de link privado em texto puro** no `localStorage` do WebView, como os favoritos VIP já fazem. Não é exportada, não é vista pelo backend, e não é lugar para tratar o link como segredo forte — a regra de visibilidade por conta evita mostrá-lo para quem não o usou, não o esconde de quem abrir o perfil do WebView.
- **Favoritos e recentes não são arquivos do app**: vivem no `localStorage` do WebView. Não são exportados com `AccountData.json`, não são vistos pelo backend/webserver e podem sumir se o perfil do WebView for limpo.
- `General.ServerRegionFormat` **é usado**: [account_api.rs](../../src-tauri/src/commands/account_api.rs) lê o template em `server_region_template` e [server_regions.rs](../../src-tauri/src/api/roblox/server_regions.rs) o aplica em `format_region`. Os tokens substituídos são `<city>`, `<region>`, `<country>`, `<countryCode>` e `<ip>` — o resto do texto passa intacto, e template que resolve vazio cai no IP cru. O comentário do default gravado no INI lista exatamente esses tokens (antes apontava `ip-api.com`, que não tem relação com eles).
- "Load Region" chama `join-game-instance` com o cookie da conta — é uma requisição real de entrada (não abre o cliente, mas consome a API do Roblox).
- Find Player compara URLs de headshot; pode dar falso negativo se o CDN devolver URLs diferentes, e varre todas as páginas (lento em jogos grandes).
- `search_games` recebe o **cookie** da conta selecionada como argumento vindo do frontend (`securityToken`), diferente dos demais comandos que recebem `userId`.
- As funções de parsing de link existem duplicadas em [launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) e [private_links.rs](../../src-tauri/src/api/roblox/private_links.rs); mudanças de formato precisam ser feitas nos dois.
