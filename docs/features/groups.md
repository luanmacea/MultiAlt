# Grupos (página Groups)

> **Escondida na versão publicada** (08/10/2026): a página fica atrás de `ENABLE_GROUPS` em [featureFlags.ts](../../src/featureFlags.ts), desligada. No teste real, o "Abrir no navegador" (Chromium automatizado) mostrava a página do grupo **sem** o botão de entrar, enquanto o site normal, na mesma conta, mostrava o botão e entrava sem pedir nada. Até entender isso, o trabalho segue na branch `feature/groups`. Para religar: `VITE_ENABLE_GROUPS=true` no build ou o padrão em `true`.

## Objetivo

Pôr várias contas num grupo (comunidade) do Roblox sem abrir o site conta por conta. A pessoa acha o grupo pelo nome, link ou id (ou escolhe um dos **Grupos populares** que aparecem com o campo vazio), marca as contas e clica **Join group**: o app entra com uma conta por vez, com uma pausa curta entre elas.

Regra do dono: **tenta primeiro; só se o Roblox pedir alguma confirmação, abre o navegador.** O app nunca tenta resolver nem contornar desafio. A conta que esbarra num desafio é marcada e o lote segue com as outras; a linha dela ganha **Open in browser** (pt "Abrir no navegador": o navegador da própria conta na página do grupo), **Try again** (a entrada de novo, só ela) e **Check again** (confere se a conta entrou).

**Nem todo desafio é captcha.** No teste do dono (08/10/2026), contas marcadas "Needs captcha" não tinham captcha nenhum ao abrir o navegador: uma só mostrava o aviso de "Termos de Uso atualizados" do Roblox (depois de aceitar, a entrada funcionou), outras mostravam a página da comunidade sem botão Entrar. Por isso o tipo do desafio (`rblx-challenge-type`) vai até a tela, e só "captcha" é chamado de captcha.

Na tela: "Groups" (pt "Grupos", es "Grupos"), ícone `UsersRound`, logo depois de Avatars na barra lateral.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| API do Roblox (busca, detalhes, entrada tipada, conferência) | [src-tauri/src/api/roblox/groups.rs](../../src-tauri/src/api/roblox/groups.rs) |
| Comandos e o lote | [src-tauri/src/commands/groups.rs](../../src-tauri/src/commands/groups.rs) |
| Navegador da conta numa página de grupo | `open_account_browser` com `groupId` em [src-tauri/src/chromium/commands.rs](../../src-tauri/src/chromium/commands.rs) |
| Página | [src/components/pages/GroupsPage.tsx](../../src/components/pages/GroupsPage.tsx) + [src/components/pages/groups/](../../src/components/pages/groups) (`GroupResults`, `GroupJoinCard`, `shared.ts`) |
| Tutorial | `groups` em [src/components/tour/tours.ts](../../src/components/tour/tours.ts) |
| Grupos populares (lista curada) | `POPULAR_GROUP_IDS` em [src-tauri/src/api/roblox/groups.rs](../../src-tauri/src/api/roblox/groups.rs) |
| Cenário do harness | `?scenario=roblox-groups&accounts=6` (o cenário `groups` é o dos grupos da lista de contas; `&popular=fail` = populares falham) em [src/dev/harness/scenarios.ts](../../src/dev/harness/scenarios.ts). O cenário `tour` também entrega os dados de grupos (para o `ui:audit`) |

## Endpoints usados

Todos pelo `endpoints::host("groups")` / `host("thumbnails")` (mockáveis nos testes).

| Para quê | Chamada | Cookie |
|---|---|---|
| Busca | `GET groups/v1/groups/search?keyword=&limit=25&prioritizeExactMatch=true[&cursor=]` | não (se o Roblox recusar, repete uma vez com o cookie de uma conta, escolhido como na presença: `pick_viewer_cookie`) |
| Grupo por id/link | `GET groups/v1/groups/<id>` | idem |
| Grupos populares | `GET groups/v1/groups/<id>` para cada id de `POPULAR_GROUP_IDS`, 4 por vez | não |
| Ícones | `/v1/batch` com tipo `GroupIcon`, 150x150, pelo mesmo cache de imagens das fotos das contas (`ImageCache::get_images_batch`) | não |
| Entrar | `POST groups/v1/groups/<id>/users` com corpo `{}` e csrf | sim, `get_cookie` (sem refresh) |
| Conferir | `GET groups/v1/users/<userId>/groups/roles`, depois `GET groups/v1/user/groups/pending` | só o segundo |

## Fluxo

1. **Busca, sem botão.** A busca roda sozinha 500 ms depois da última tecla (`SEARCH_DEBOUNCE_MS`); **Enter** busca na hora. Cada busca ganha um número e a resposta de uma busca que já não é a última é jogada fora — a resposta lenta de "pe" nunca apaga a de "pet". O campo aceita palavra (2 a 50 caracteres, limite do Roblox; com 1 caractere aparece "Type at least 2 characters to search." e nada sai), número puro ou link `roblox.com/groups/<id>/...` / `roblox.com/communities/<id>/...` (inclusive com prefixo de idioma). Link ou id (colado ou digitado) viram um resultado só, já escolhido (`parse_group_reference`). "Load more" segue o `nextPageCursor`.
1. **Campo vazio: Grupos populares** (pt/es "Grupos populares"). O Roblox não tem endpoint de "top grupos"; a lista é **curada à mão** em `POPULAR_GROUP_IDS` (24 ids, montada em 08/10/2026 com a busca pública por palavras amplas — roblox, games, simulator, adopt, tycoon, obby, brookhaven, doors...: os de mais membros, com selo de verificado, sem nome ofensivo, de piada ou com cara de golpe). O comando `groups_popular` lê os dados de agora de cada um (4 por vez, sem cookie) e devolve do maior para o menor; quem falha fica de fora; nenhum respondeu = erro e a tela volta para a dica "Search by name, or paste a group link or ID.". A página guarda a lista na sessão (`SessionCache`): voltar à página não pede de novo. O cartão de um popular é o mesmo da busca e escolhe o grupo igual.
   - **Por que não `/v2/groups?groupIds=` em lote:** conferido em 08/10/2026, ele devolve `id, name, description, owner, created, hasVerifiedBadge` — sem `memberCount` nem `publicEntryAllowed`, que o cartão precisa. O `/v1/groups/<id>` traz os dois.
   - **Para atualizar a lista:** repetir as buscas (`/v1/groups/search?keyword=<palavra>&limit=100`), trocar os ids e conferir cada um em `/v1/groups/<id>`.
2. **Cartões.** Ícone, nome, selo de verificado, membros e a regra de entrada: "Open to join" (`publicEntryAllowed`), "Approval required" ou "Locked".
3. **Contas.** Lista de marcar como as abas do Modo AFK (Select all / Clear), com o nome mascarado pelo `useAccountLabel`. Ao abrir a página, vêm marcadas as contas selecionadas na lista principal. **Clicar em qualquer lugar da caixa da conta** marca/desmarca (pedido do dono: a caixa alta, com selo e botões, só respondia na linha do nome); os botões de dentro (Open in browser, Try again, Check again) fazem só a ação deles (`stopPropagation`). A caixa de marcar continua um `role="checkbox"` de verdade, com foco visível e Espaço/Enter. Durante o lote, nada marca.
4. **Join group.** `groups_join_batch(userIds, groupId)`: relê os detalhes do grupo (para saber se pede aprovação e se está trancado), monta o retrato com todas as contas em "waiting" e entra com uma por vez. Entre uma conta e a próxima, pausa sorteada entre 2 e 4 s, em fatias de 100 ms — o Cancel vale no meio da pausa. Cada passo publica o retrato inteiro em `groups-join-state`; a página pode sair e voltar (`get_groups_join_state`).
5. **Desafio.** O selo diz "Needs captcha" só quando `challengeType == "captcha"`; qualquer outro tipo é "Needs confirmation" (pt "Precisa de confirmação"). Embaixo: "Roblox asked for a captcha" ou "Roblox asked this account to confirm something" (pt "O Roblox pediu uma confirmação para esta conta"), a dica "Open it in the browser and accept what Roblox shows (terms, verification). Then press Try again." e, se não for captcha, o tipo em letra miúda ("type: proofofwork"). Uma linha miúda de diagnóstico mostra o que o Roblox respondeu (`detail`, ex. "HTTP 403 · challenge proofofwork · code 0", mais a mensagem dele).
6. **Ações da linha** (estados `challenge`, `failed` e `notMember`):
   - **Open in browser** (era "Solve in browser") → `open_account_browser { userId, groupId }`: o navegador da conta (o mesmo do "Open in browser" do painel, logado pelo cookie dela) abre em `https://www.roblox.com/communities/<id>`.
   - **Try again** → `groups_join_retry { userId, groupId }`: a entrada de novo **só para essa conta**, com o mesmo tratamento de resultado do lote; mexe só na linha dela no retrato. Usa a mesma trava do lote (`GROUP_JOIN_RUNNING`): não roda junto de um lote nem de outro "Try again" — enquanto roda, os "Try again" e o Join group ficam desligados.
   - **Check again** → `groups_check_membership`: está na lista de grupos → "Joined"; tem pedido pendente → "Pending approval"; nenhum dos dois → "Not a member yet" (os botões continuam).

## Regras de negócio

### Resultado de cada conta

Códigos conferidos na documentação oficial do `POST /v1/groups/{groupId}/users` (`groups.roblox.com/docs/json/v1`, lida em 08/10/2026):

| Resposta | Estado na tela |
|---|---|
| 2xx, grupo aberto | Joined |
| 2xx, grupo com aprovação (`publicEntryAllowed: false`) | Pending approval — o Roblox responde igual; o pendente sai dos detalhes do grupo |
| 409 código 7 ("You have already requested to join this group.") | Pending approval |
| 409 código 8 ("You are already a member of this group.") | Already a member |
| 403 com o header `rblx-challenge-id` | Needs captcha se `rblx-challenge-type` = `captcha`; senão Needs confirmation, com o tipo (`twostepverification`, `reauthentication`, `chef`, `proofofwork`, `generic`...; ausente = `unknown`). O lote segue |
| Qualquer outra (403 código 6, limite de grupos; 403 código 14, grupo fechado; 33/34, verificação/tempo de conta; 429 código 10, tentativas demais; 400 código 1, grupo inválido...) | Failed, com a mensagem do Roblox embaixo |

O que **não** foi confirmado: que o 2xx de um grupo com aprovação seja sempre pedido pendente (a documentação não descreve o corpo). A tela trata assim porque é o que o site mostra; **Check again** confere de verdade.

### Outras regras

- **Uma conta por vez, um lote por vez.** `GROUP_JOIN_RUNNING` recusa um segundo lote ("A group join is already running"). Com o lote de outro grupo rodando, o botão diz "Another group join is running".
- **Sem refresh de sessão.** Nada aqui passa por `run_with_session_retry`/`refresh_account_session` (o refresh derruba as outras sessões da conta — ver CLAUDE.md). Cookie vencido ou ausente só falha aquela conta.
- **Nunca resolve desafio.** Nem tenta de novo sozinho depois de um desafio — o "Try again" é sempre um clique da pessoa.
- **Diagnóstico sem segredo.** Cada entrada grava no stderr (`eprintln!`, como o resto do backend) `[groups] join user=… group=… status=… challenge=… code=… message=…` — nunca cookie nem token. Numa build GUI o stderr não aparece; por isso o mesmo resumo (`detail` + `reason`) vai para a linha da conta na tela.
- **Grupo trancado** (`isLocked`) é recusado antes de qualquer entrada.
- **URL do navegador fechada no backend.** O front manda só o `groupId` numérico; `account_browser_start_url` monta a URL. Uma janela logada nunca abre um endereço escolhido pelo front.
- O painel de uma conta (DetailSidebar → Join Group) continua com o comando antigo `join_group`, agora em cima da entrada tipada: pendente e "já é membro" contam como sucesso; desafio vira erro dizendo qual ("asked for a captcha" ou "asked this account to confirm something (<tipo>)").

## Comandos

| Comando | O que faz |
|---|---|
| `groups_search(query, cursor)` | busca por palavra, ou um grupo quando a pessoa colou link/id |
| `groups_icons(groupIds)` | ícones pelo cache de imagens |
| `groups_popular()` | os grupos de `POPULAR_GROUP_IDS` com os dados de agora, maior primeiro |
| `groups_join_batch(userIds, groupId)` | o lote; responde no fim com o retrato final |
| `groups_join_retry(userId, groupId)` | "Try again" de uma conta; responde com o retrato atualizado |
| `groups_cancel_join()` | pede o cancelamento (a conta em andamento termina) |
| `get_groups_join_state()` | retrato atual |
| `groups_check_membership(userId, groupId)` | `"joined"`, `"pending"` ou `"notMember"`; atualiza a linha no retrato se for o mesmo grupo |

Evento: `groups-join-state` (o retrato inteiro, camelCase). Cada linha: `userId`, `status`, `reason` (falha, ou a mensagem do desafio), `challengeType` (só em `challenge`) e `detail` (diagnóstico curto).

## Armadilhas

- **Mock compartilhado nos testes.** Todos os testes de `api` dividem um servidor wiremock: cada teste usa grupo, usuário e cookie próprios (88xxxx, 99xxxx, 77xxxx).
- **`include!` e nomes.** `commands/groups.rs` entra na raiz do crate: não dá para repetir `use` que outro arquivo incluído já fez (`Duration`, `AtomicBool`) — por isso os caminhos completos.
- **A tela não decide o estado.** O selo de cada linha é o que o backend publicou; o cenário do harness entrega dados fixos (captcha na 2ª conta, membro na 3ª, pendente na 4ª, falha na 5ª, desafio `proofofwork` na 6ª; "Try again" devolve "joined"), nunca a lógica.
- **Pausa e limite.** 2–4 s entre contas é o mesmo cuidado do Avatars com o limite de taxa do Roblox; não tire a pausa para "ficar rápido".

## Testes

Suíte `groups` (`bun run t groups`):

- Rust: `group_reference_tests` (link/id/palavra), `group_join_http_tests` (cada resultado da entrada, tipo do desafio e mensagem do Roblox, tipo ausente = `unknown`, diagnóstico, busca, detalhes, populares ordenados e sem os que falham, lista curada sem repetidos), `group_membership_http_tests` (membro, pendente, nenhum, erro), `group_join_batch_tests` (uma por vez, segue depois do desafio, tipo e diagnóstico na linha, "Try again" só na linha da conta e em retrato novo de outro grupo, cancelamento antes e durante a pausa, conta sem cookie, pausa entre 2 e 4 s, um lote por vez), `chromium_commands_tests` (URL do navegador da conta).
- Front: [GroupsPage.test.tsx](../../src/components/pages/GroupsPage.test.tsx) — sem botão Search, busca sozinha depois da pausa (uma vez), resposta velha não apaga a nova, 2 caracteres (id puro busca), link colado, cartões da busca, erro, "Load more", grupos populares (ordem, ícones, escolha, falha → dica, uma consulta por sessão), contas pré-marcadas, Select all/Clear, clique em qualquer lugar da caixa marca (botões de dentro não), Join só com grupo e conta, estados por linha, captcha × "confirm something" com o tipo, "Open in browser" chamando `open_account_browser` com `{ userId, groupId }`, "Try again" (só a conta, trava o resto, recusa vira aviso), "Check again", progresso pelo evento, Cancel, nomes ocultos e o tutorial.
