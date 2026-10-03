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
| Evento | `friends-online-progress` → `{ done, total, requestId, entry }` |
| Suíte de teste | `bun run t friends` |

## Fluxo

1. O usuário abre a aba **Friends** da tela Choose Game com N contas selecionadas.
2. O frontend chama `get_online_friends_for_accounts({ userIds, delayMs, requestId })`.
3. O backend percorre as contas **em sequência**, com pausa entre elas, e faz **uma** requisição por conta: `GET friends.roblox.com/v1/users/{userId}/friends/online` com o cookie daquela conta.
4. Como o payload dessa rota não traz mais `name`/`displayName` (mudança da Roblox em out/2024), os nomes são completados em lote por `POST users.roblox.com/v1/users` (100 ids por requisição). Quando um amigo está em jogo mas sem `gameInstanceId`, há um resgate opcional via `presence.roblox.com/v1/presence/users` **com cookie** (sem cookie a Roblox omite o `gameId`).
5. Cada conta vira um `AccountFriends { userId, friends, error }` — o erro de uma conta **não** derruba as outras. Assim que uma conta volta, o evento `friends-online-progress` leva a contagem **e a entrada dela** (`entry`), com o `requestId` que a tela mandou; a aba desenha aquela conta na hora, sem esperar o lote. O comando ainda devolve o lote inteiro no fim, e é essa a versão final da lista.
6. Ao clicar num amigo entrável, a aba chama `launchAll(userIds, placeId, jobId)` (o `useLauncher` da Choose Game, que usa `launch_multiple`).

### Carga progressiva e cache da aba

- **Uma conta por vez na tela.** Enquanto a rodada corre, a aba mostra todas as contas da seleção, **na ordem da seleção** (a mesma do retorno final), não na ordem em que as respostas chegam (`upsertAccountFriends`). Conta que ainda não voltou aparece como pendente (`friends-pending-<id>`: cabeçalho com spinner e *Loading friends...*). Evento com outro `requestId` (sobra de uma rodada anterior) é ignorado; evento sem `requestId` só atualiza a contagem.
- **Voltar à aba mostra o que ela tinha.** A última lista de cada seleção fica num cache de memória da sessão (`friendsCache`, ver `utils/sessionCache.ts`), assim como as miniaturas dos amigos. Ao voltar (trocando de aba ou saindo e entrando na Choose Game), a lista aparece no primeiro desenho e a consulta roda por trás: cada conta mostra um spinner no cabeçalho (`friends-updating-<id>`) até a resposta nova dela chegar e tomar o lugar.
- **Voltar no meio de uma rodada não abre outra.** A rodada em curso fica registrada por seleção (`runningLoads`); a aba que monta de novo se junta a ela em vez de chamar o backend outra vez — duas rodadas juntas dobrariam as chamadas à API de amigos.
- As miniaturas continuam saindo **num lote só**, no fim da rodada (como antes) — pedir por conta multiplicaria as chamadas.

## Regras de negócio

- **Uma requisição por conta.** O rate limit da API de amigos é agressivo; consultar amigo a amigo derruba a conta em segundos. A pausa entre contas existe pelo mesmo motivo.
- **Todos os online aparecem**, não só os entráveis. Quem não dá para seguir fica esmaecido **com o motivo** — senão o amigo "some" da lista e o usuário não entende por quê:

  | Situação | Motivo mostrado |
  |---|---|
  | `presenceType == 1` (online, fora de jogo) | *On the website* |
  | `presenceType == 3` | *In Studio* |
  | Em jogo, sem `gameId` | *Server not visible* (privacidade do amigo) |

- **`placeId` tem prioridade sobre `rootPlaceId`**: o Job ID (`gameId`) é de um servidor **do place em que o amigo está**, que pode ser um sub-place. Com o raiz, o launch pedia "o servidor X no place raiz" — que não existe ali — e o cliente abria em *This experience has ended, or the server became unavailable*. Visto no Life Sentence, que tem 6 places (o raiz distribui para "VC Only", "Pro Players"...). O raiz fica de reserva, para quando o `placeId` não vem. Até 28/09/2026 era o contrário. O Follow da Choose Game segue a mesma regra com Job ID; **sem** Job ID (servidor escondido, entrada num servidor público do jogo) ele usa o raiz, porque sub-place de teleporte pode não aceitar entrada direta.
- **O launch passa sempre por `launch_multiple`** (`launchAll`). Um laço próprio com `launch_roblox` fura o piso anti-captcha de 8 s que o backend aplica entre contas — foi exatamente o bug corrigido no Follow (ver [multi-launch.md](multi-launch.md)).
- **Nomes de amigos nunca são mascarados.** São terceiros e o usuário precisa reconhecê-los; `hideUsernames` vale só para os nomes das contas do próprio usuário.
- O servidor do amigo é resolvido **uma vez** e todas as contas vão para aquele `jobId` — não há uma resolução por conta.

## Configurações relacionadas

- `hideUsernames` / letras de prévia — mascaramento dos nomes **das contas** (não dos amigos).
- O atraso entre contas (`delayMs`) tem padrão no backend (`FRIENDS_ONLINE_DELAY_MS`); a UI manda o seu próprio valor.

## Armadilhas / cuidados

- A rota antiga `friends/v1/my/friends/online` foi **removida** pelo Roblox e responde 404 para todo mundo — foi o que fez a aba mostrar "status 404" em todas as contas. A rota atual leva o `userId` na URL **e** o cookie da conta.
- Nessa rota a presença vem aninhada em `userPresence`, com as chaves em maiúscula e o tipo como **texto** (`"InGame"`), não número. O parser aceita as duas formas, e um nome novo vira presença desconhecida em vez de derrubar o amigo.
- A presença **sem** cookie (`api::roblox::get_presence`) não devolve `gameId`; por isso o resgate de presença usa `get_presence_as(cookie, ids)`. O **comando** `get_presence` também manda cookie: o da conta pedida em `viewerUserId` ou, sem ela, o da primeira conta válida (`pick_viewer_cookie`); se a chamada autenticada falhar, repete sem cookie, para uma sessão morta não apagar a presença de todo mundo. Nunca passa por `run_with_session_retry`. **Ressalva:** o `gameId` que volta para as outras contas depende da privacidade/amizade de quem foi escolhida como viewer — é assim que a API funciona. O Follow da Choose Game depende disso para achar o servidor do alvo.
- Amigo em jogo com servidor escondido não vira erro — vira linha esmaecida. Transformar isso em erro esconderia o resto da lista.
- Não usar `run_with_session_retry` nessas leituras: o refresh desloga a conta em todo lugar (ver [authentication.md](authentication.md)).
