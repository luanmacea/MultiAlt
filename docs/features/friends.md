# Amigos online (aba Friends)

## Objetivo

Mostrar os amigos **online** de cada conta selecionada e, com um clique num amigo, mandar **todas** as contas selecionadas para o servidor dele — sem o usuário ter que descobrir Job ID, copiar link ou usar o Follow por nome.

## Onde fica o código

| Peça | Arquivo |
|---|---|
| Cliente da API | [api/roblox/friends.rs](../../src-tauri/src/api/roblox/friends.rs) (`get_online_friends`) |
| Comandos Tauri | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs) (`get_online_friends`, `get_online_friends_for_accounts`, `collect_online_friends`) |
| UI | [components/friends/FriendsTab.tsx](../../src/components/friends/FriendsTab.tsx) |
| Tipos no frontend | [types.ts](../../src/types.ts) (`OnlineFriend`, `AccountFriends`, `FriendsOnlineProgress`) |
| Evento | `friends-online-progress` → `{ done, total }` |
| Suíte de teste | `bun run t friends` |

## Fluxo

1. O usuário abre a aba **Friends** da tela Choose Game com N contas selecionadas.
2. O frontend chama `get_online_friends_for_accounts({ userIds, delayMs })`.
3. O backend percorre as contas **em sequência**, com pausa entre elas, e faz **uma** requisição por conta: `GET friends.roblox.com/v1/users/{userId}/friends/online` com o cookie daquela conta.
4. Como o payload dessa rota não traz mais `name`/`displayName` (mudança da Roblox em out/2024), os nomes são completados em lote por `POST users.roblox.com/v1/users` (100 ids por requisição). Quando um amigo está em jogo mas sem `gameInstanceId`, há um resgate opcional via `presence.roblox.com/v1/presence/users` **com cookie** (sem cookie a Roblox omite o `gameId`).
5. Cada conta vira um `AccountFriends { userId, friends, error }` — o erro de uma conta **não** derruba as outras. O progresso vai para a UI pelo evento `friends-online-progress`.
6. Ao clicar num amigo entrável, a aba chama `launchAll(userIds, placeId, jobId)` (o `useLauncher` da Choose Game, que usa `launch_multiple`).

## Regras de negócio

- **Uma requisição por conta.** O rate limit da API de amigos é agressivo; consultar amigo a amigo derruba a conta em segundos. A pausa entre contas existe pelo mesmo motivo.
- **Todos os online aparecem**, não só os entráveis. Quem não dá para seguir fica esmaecido **com o motivo** — senão o amigo "some" da lista e o usuário não entende por quê:

  | Situação | Motivo mostrado |
  |---|---|
  | `presenceType == 1` (online, fora de jogo) | *On the website* |
  | `presenceType == 3` | *In Studio* |
  | Em jogo, sem `gameId` | *Server not visible* (privacidade do amigo) |

- **`rootPlaceId` tem prioridade sobre `placeId`**: o Roblox devolve o place específico (que pode ser um sub-place de teleporte) e o place raiz da experiência; o launch aceita o raiz.
- **O launch passa sempre por `launch_multiple`** (`launchAll`). Um laço próprio com `launch_roblox` fura o piso anti-captcha de 8 s que o backend aplica entre contas — foi exatamente o bug corrigido no Follow (ver [multi-launch.md](multi-launch.md)).
- **Nomes de amigos nunca são mascarados.** São terceiros e o usuário precisa reconhecê-los; `hideUsernames` vale só para os nomes das contas do próprio usuário.
- O servidor do amigo é resolvido **uma vez** e todas as contas vão para aquele `jobId` — não há uma resolução por conta.

## Configurações relacionadas

- `hideUsernames` / letras de prévia — mascaramento dos nomes **das contas** (não dos amigos).
- O atraso entre contas (`delayMs`) tem padrão no backend (`FRIENDS_ONLINE_DELAY_MS`); a UI manda o seu próprio valor.

## Armadilhas / cuidados

- A rota antiga `friends/v1/my/friends/online` foi **removida** pelo Roblox e responde 404 para todo mundo — foi o que fez a aba mostrar "status 404" em todas as contas. A rota atual leva o `userId` na URL **e** o cookie da conta.
- Nessa rota a presença vem aninhada em `userPresence`, com as chaves em maiúscula e o tipo como **texto** (`"InGame"`), não número. O parser aceita as duas formas, e um nome novo vira presença desconhecida em vez de derrubar o amigo.
- `get_presence` sem cookie não devolve `gameId`; por isso o resgate de presença usa `get_presence_as(cookie, ids)`.
- Amigo em jogo com servidor escondido não vira erro — vira linha esmaecida. Transformar isso em erro esconderia o resto da lista.
- Não usar `run_with_session_retry` nessas leituras: o refresh desloga a conta em todo lugar (ver [authentication.md](authentication.md)).
