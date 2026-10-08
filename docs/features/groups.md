# Grupos (página Groups)

## Objetivo

Pôr várias contas num grupo (comunidade) do Roblox sem abrir o site conta por conta. A pessoa acha o grupo pelo nome, link ou id, marca as contas e clica **Join group**: o app entra com uma conta por vez, com uma pausa curta entre elas.

Regra do dono: **tenta primeiro; só se o Roblox pedir captcha, abre o navegador.** O app nunca tenta resolver nem contornar captcha. A conta que esbarra num desafio é marcada "Needs captcha" e o lote segue com as outras; a linha dela ganha **Solve in browser** (abre o navegador da própria conta na página do grupo, onde a pessoa resolve e clica Entrar) e **Check again** (confere se a conta entrou).

Na tela: "Groups" (pt "Grupos", es "Grupos"), ícone `UsersRound`, logo depois de Avatars na barra lateral.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| API do Roblox (busca, detalhes, entrada tipada, conferência) | [src-tauri/src/api/roblox/groups.rs](../../src-tauri/src/api/roblox/groups.rs) |
| Comandos e o lote | [src-tauri/src/commands/groups.rs](../../src-tauri/src/commands/groups.rs) |
| Navegador da conta numa página de grupo | `open_account_browser` com `groupId` em [src-tauri/src/chromium/commands.rs](../../src-tauri/src/chromium/commands.rs) |
| Página | [src/components/pages/GroupsPage.tsx](../../src/components/pages/GroupsPage.tsx) + [src/components/pages/groups/](../../src/components/pages/groups) (`GroupResults`, `GroupJoinCard`, `shared.ts`) |
| Tutorial | `groups` em [src/components/tour/tours.ts](../../src/components/tour/tours.ts) |
| Cenário do harness | `?scenario=roblox-groups&accounts=6` (o cenário `groups` é o dos grupos da lista de contas) em [src/dev/harness/scenarios.ts](../../src/dev/harness/scenarios.ts) |

## Endpoints usados

Todos pelo `endpoints::host("groups")` / `host("thumbnails")` (mockáveis nos testes).

| Para quê | Chamada | Cookie |
|---|---|---|
| Busca | `GET groups/v1/groups/search?keyword=&limit=25&prioritizeExactMatch=true[&cursor=]` | não (se o Roblox recusar, repete uma vez com o cookie de uma conta, escolhido como na presença: `pick_viewer_cookie`) |
| Grupo por id/link | `GET groups/v1/groups/<id>` | idem |
| Ícones | `/v1/batch` com tipo `GroupIcon`, 150x150, pelo mesmo cache de imagens das fotos das contas (`ImageCache::get_images_batch`) | não |
| Entrar | `POST groups/v1/groups/<id>/users` com corpo `{}` e csrf | sim, `get_cookie` (sem refresh) |
| Conferir | `GET groups/v1/users/<userId>/groups/roles`, depois `GET groups/v1/user/groups/pending` | só o segundo |

## Fluxo

1. **Busca.** O campo aceita palavra (2 a 50 caracteres, limite do Roblox), número puro ou link `roblox.com/groups/<id>/...` / `roblox.com/communities/<id>/...` (inclusive com prefixo de idioma). Link ou id viram um resultado só, já escolhido (`parse_group_reference`). "Load more" segue o `nextPageCursor`.
2. **Cartões.** Ícone, nome, selo de verificado, membros e a regra de entrada: "Open to join" (`publicEntryAllowed`), "Approval required" ou "Locked".
3. **Contas.** Lista de marcar como as abas do Modo AFK (Select all / Clear), com o nome mascarado pelo `useAccountLabel`. Ao abrir a página, vêm marcadas as contas selecionadas na lista principal.
4. **Join group.** `groups_join_batch(userIds, groupId)`: relê os detalhes do grupo (para saber se pede aprovação e se está trancado), monta o retrato com todas as contas em "waiting" e entra com uma por vez. Entre uma conta e a próxima, pausa sorteada entre 2 e 4 s, em fatias de 100 ms — o Cancel vale no meio da pausa. Cada passo publica o retrato inteiro em `groups-join-state`; a página pode sair e voltar (`get_groups_join_state`).
5. **Captcha.** A linha "Needs captcha" mostra **Solve in browser** → `open_account_browser { userId, groupId }`: o navegador da conta (o mesmo do "Open in browser", logado pelo cookie dela) abre em `https://www.roblox.com/communities/<id>`. A pessoa resolve o captcha e clica Entrar lá. **Check again** → `groups_check_membership`: está na lista de grupos → "Joined"; tem pedido pendente → "Pending approval"; nenhum dos dois → "Not a member yet" (os botões continuam).

## Regras de negócio

### Resultado de cada conta

Códigos conferidos na documentação oficial do `POST /v1/groups/{groupId}/users` (`groups.roblox.com/docs/json/v1`, lida em 08/10/2026):

| Resposta | Estado na tela |
|---|---|
| 2xx, grupo aberto | Joined |
| 2xx, grupo com aprovação (`publicEntryAllowed: false`) | Pending approval — o Roblox responde igual; o pendente sai dos detalhes do grupo |
| 409 código 7 ("You have already requested to join this group.") | Pending approval |
| 409 código 8 ("You are already a member of this group.") | Already a member |
| 403 com o header `rblx-challenge-id` | Needs captcha (o lote segue) |
| Qualquer outra (403 código 6, limite de grupos; 403 código 14, grupo fechado; 33/34, verificação/tempo de conta; 429 código 10, tentativas demais; 400 código 1, grupo inválido...) | Failed, com a mensagem do Roblox embaixo |

O que **não** foi confirmado: que o 2xx de um grupo com aprovação seja sempre pedido pendente (a documentação não descreve o corpo). A tela trata assim porque é o que o site mostra; **Check again** confere de verdade.

### Outras regras

- **Uma conta por vez, um lote por vez.** `GROUP_JOIN_RUNNING` recusa um segundo lote ("A group join is already running"). Com o lote de outro grupo rodando, o botão diz "Another group join is running".
- **Sem refresh de sessão.** Nada aqui passa por `run_with_session_retry`/`refresh_account_session` (o refresh derruba as outras sessões da conta — ver CLAUDE.md). Cookie vencido ou ausente só falha aquela conta.
- **Nunca resolve captcha.** Nem tenta de novo sozinho depois de um desafio.
- **Grupo trancado** (`isLocked`) é recusado antes de qualquer entrada.
- **URL do navegador fechada no backend.** O front manda só o `groupId` numérico; `account_browser_start_url` monta a URL. Uma janela logada nunca abre um endereço escolhido pelo front.
- O painel de uma conta (DetailSidebar → Join Group) continua com o comando antigo `join_group`, agora em cima da entrada tipada: pendente e "já é membro" contam como sucesso; captcha vira erro dizendo isso.

## Comandos

| Comando | O que faz |
|---|---|
| `groups_search(query, cursor)` | busca por palavra, ou um grupo quando a pessoa colou link/id |
| `groups_icons(groupIds)` | ícones pelo cache de imagens |
| `groups_join_batch(userIds, groupId)` | o lote; responde no fim com o retrato final |
| `groups_cancel_join()` | pede o cancelamento (a conta em andamento termina) |
| `get_groups_join_state()` | retrato atual |
| `groups_check_membership(userId, groupId)` | `"joined"`, `"pending"` ou `"notMember"`; atualiza a linha no retrato se for o mesmo grupo |

Evento: `groups-join-state` (o retrato inteiro, camelCase).

## Armadilhas

- **Mock compartilhado nos testes.** Todos os testes de `api` dividem um servidor wiremock: cada teste usa grupo, usuário e cookie próprios (88xxxx, 99xxxx, 77xxxx).
- **`include!` e nomes.** `commands/groups.rs` entra na raiz do crate: não dá para repetir `use` que outro arquivo incluído já fez (`Duration`, `AtomicBool`) — por isso os caminhos completos.
- **A tela não decide o estado.** O selo de cada linha é o que o backend publicou; o cenário do harness entrega dados fixos (captcha na 2ª conta, membro na 3ª, pendente na 4ª, falha na 5ª), nunca a lógica.
- **Pausa e limite.** 2–4 s entre contas é o mesmo cuidado do Avatars com o limite de taxa do Roblox; não tire a pausa para "ficar rápido".

## Testes

Suíte `groups` (`bun run t groups`):

- Rust: `group_reference_tests` (link/id/palavra), `group_join_http_tests` (cada resultado da entrada, busca, detalhes), `group_membership_http_tests` (membro, pendente, nenhum, erro), `group_join_batch_tests` (uma por vez, segue depois do captcha, cancelamento antes e durante a pausa, conta sem cookie, pausa entre 2 e 4 s, um lote por vez), `chromium_commands_tests` (URL do navegador da conta).
- Front: [GroupsPage.test.tsx](../../src/components/pages/GroupsPage.test.tsx) — cartões da busca, erro, "Load more", link colado, contas pré-marcadas, Select all/Clear, Join só com grupo e conta, estados por linha, "Solve in browser" chamando `open_account_browser` com `{ userId, groupId }`, "Check again", progresso pelo evento, Cancel, nomes ocultos e o tutorial.
