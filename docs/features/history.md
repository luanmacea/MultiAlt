# Histórico de sessões e tempo de jogo

## Objetivo

Responder, por conta, "onde ela jogou, por quanto tempo e como terminou": entrou num jogo,
trocou de servidor, saiu, caiu (com o motivo lido do log do Roblox), o cliente fechou, foi
moderada. Com isso: um resumo de tempo de jogo dos últimos 14 dias por jogo, um botão para
voltar ao servidor de uma sessão recente e o export em CSV. É a ideia 6 de
[ideias-de-outros-gerenciadores.md](../ideias-de-outros-gerenciadores.md) (pacote Organização em
[plano-ideias.md](../plano-ideias.md)); usa os eventos do pacote Quedas
([watcher.md](watcher.md#quedas-lidas-do-log-do-cliente)).

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Formato do evento, arquivo JSONL, retenção, recuperação, montagem das sessões (`build_sessions`) | [data/session_history.rs](../../src-tauri/src/data/session_history.rs) |
| Observador (retrato do monitor → eventos), checkpoint, comandos `get_session_history` e `save_history_export`, marca de moderação | [commands/session_history.rs](../../src-tauri/src/commands/session_history.rs) |
| Retrato por cliente (`ClientSessionSnapshot`: place, Job ID, queda, saiu, terminou) e a chamada a cada passada | [commands/client_health.rs](../../src-tauri/src/commands/client_health.rs) (`session_snapshots`, `run_client_health_tick`) |
| Moderação no launch | [launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) `mark_account_moderated` → `record_moderated_history` |
| Tela (painel da conta) | [AccountHistory.tsx](../../src/components/accounts/AccountHistory.tsx), dentro do [SingleSelectSidebar](../../src/components/accounts/SingleSelectSidebar.tsx) |
| Sessão de agora na página Session (jogo, servidor, tempo em jogo) | `get_current_sessions` em [commands/session_history.rs](../../src-tauri/src/commands/session_history.rs); [CurrentSessionLine.tsx](../../src/components/session/CurrentSessionLine.tsx) e [useCurrentSessions.ts](../../src/hooks/useCurrentSessions.ts) no [SessionPanel](../../src/components/session/SessionPanel.tsx) |
| Regras puras da tela: duração, tempo de 14 dias, "Join again", texto do fim, CSV | [utils/sessionHistory.ts](../../src/utils/sessionHistory.ts) |

## Fluxo

1. O monitor de quedas (`client_health.rs`, a cada 2 s, sempre ligado no Windows) já lê o log
   de cada cliente rastreado — aberto pelo app **ou** adotado do site. A sessão do log agora
   também guarda o place e o Job ID do último `! Joining game`.
2. No fim de cada passada, `run_client_health_tick` entrega o retrato
   (`session_snapshots`) a `record_session_history`. Nada novo é lido do disco por causa disso.
3. O observador (`SessionHistoryObserver::observe`, puro) compara com a passada anterior:

   | Mudança | Evento |
   |---|---|
   | cliente com place, sem queda, sem saída | `joined` (place, Job ID) |
   | em jogo, place/Job ID mudou no mesmo processo | `teleported` (fecha uma sessão, abre outra) |
   | em jogo, o monitor marcou queda | `dropped` (`dropKind`, `reason`, `code`, `message`) |
   | em jogo, a pessoa saiu (`leaveUGCGameInternal`, `returnToLuaApp`…) | `left` |
   | em jogo, o processo terminou, ou a conta saiu do rastreamento | `closed` |
   | PID novo para a conta | `closed` do anterior + `joined` do novo |
   | caiu ou saiu e entrou de novo no mesmo cliente | `joined` |

4. Cada evento vai para o fim de `RAMSessionHistory.jsonl` **na hora** (uma linha JSON por
   evento) e o evento `session-history-changed {userIds}` avisa a tela.
5. A cada minuto com alguém em jogo, `RAMSessionHistory.open.json` guarda quem está em jogo e
   quando. Ao abrir o app (`start_session_history`, antes do monitor começar), as sessões que o
   arquivo deixou abertas e que estavam nesse checkpoint ganham `appClosed` naquele instante
   (`recovery_events`) — é assim que um fechamento à força não deixa sessão "infinita".
6. Moderação: quando o launch descobre que a conta está moderada e a move para o grupo
   `moderadas`, entra um `moderated` (só na primeira vez, como o próprio aviso).
7. A tela pede `get_session_history(userId)`: o backend monta as sessões (`build_sessions`),
   da mais nova para a mais velha. Sessão sem fim de uma conta que não está mais em jogo vira
   `unknown` (não "em jogo").

## O que a pessoa vê (painel da conta › History)

- **Time played, last 14 days:** até 5 jogos, barra proporcional ao maior, duração ao lado
  ("3h 32m"). Só o pedaço da sessão dentro da janela conta; sessão sem fim conhecido não soma.
  Sessão em jogo soma até agora.
- **Recent sessions:** as 6 mais novas (com "Show all"): nome do jogo (`useGameIdentity`),
  "Today 14:02 · 1h 12m" e como terminou — as quedas com as **mesmas frases** da Sessão
  ("Disconnected: lost connection", "Kicked: …", "The server shut down"…; código no tooltip).
- **Join again:** só para sessão com Job ID conhecido que terminou há até 3 h e que não terminou
  com o servidor fechando (`canJoinAgain`). Pergunta "conta online?" como os outros launches e
  chama `joinServer(userId, { placeId, jobId })` — o launch normal de uma conta. O servidor pode
  já não existir (o Roblox então manda para outro do mesmo jogo, ou recusa); servidor privado não
  abre só pelo Job ID.
  - O aviso de conta online leva em conta o **próprio histórico**: se uma sessão está "Playing
    now", a conta conta como "In Game" mesmo com a presença do Roblox dizendo offline (privacidade,
    atraso) ou falhando — um segundo launch tomaria essa sessão (`useJoinOnlineWarning`,
    parâmetro `knownInGame`). Quem desligou o aviso em Settings continua sem ele.
  - Enquanto abre, o botão fica "Joining..." e desabilitado (todos os "Join again" do painel: um
    launch por vez). Começou: o toast "Launching game..." do `joinServer`. Falhou: toast "Could not
    join this server again" (antes só a barra de status dizia).
- **Export CSV:** monta o CSV na tela (nomes dos jogos já resolvidos) e o backend grava em
  `<pasta de dados>/exports/multialt-history-<userId>-<data>.csv` (com BOM, para o Excel) e abre o
  Explorer com o arquivo selecionado. Colunas: início, fim, minutos, jogo, Place ID, servidor,
  como terminou, código, mensagem.
  - **Separador pelo idioma da tela** (`csvSeparatorFor`): `;` onde o decimal é vírgula (pt, es,
    de…), `,` em inglês. O Excel abre `.csv` com o separador de lista do Windows; em pt-BR é `;`,
    e com `,` tudo caía na coluna A. Os cabeçalhos já saem traduzidos, então o idioma da tela é o
    sinal certo.
  - O toast de sucesso aparece no canto de baixo à direita, por cima do botão "Choose Game" por
    alguns segundos: é a posição de **todos** os toasts do app (`App.tsx`), não algo do histórico.
- **Painel:** History é a **última** seção do painel da conta e **recolhível** (clique no título;
  a escolha fica no `localStorage` do navegador, `multialt.sidebarSection.collapsed.history`).
- **Aviso de queda no topo do painel:** a mensagem de kick quebra em até 3 linhas
  (`ClientHealthNote wrap`) em vez de ser cortada numa; o texto inteiro fica no tooltip.
- Conta sem nada: "Nothing yet. Games this account plays while MultiAlt is open show up here."

## Sessão de agora na página Session

A sessão **aberta** (a que no painel aparece como "Playing now") também organiza a lista
**In game** do Painel de Sessão (página Session). Desde 11/10/2026 a lista é **agrupada por jogo
e servidor** ([sessionGroups.ts](../../src/utils/sessionGroups.ts),
[InGameGroups.tsx](../../src/components/session/InGameGroups.tsx)):

- **Needs attention** no topo: quem está reconectando, não responde ou caiu sai do grupo do jogo
  (a sessão de quem caiu já fechou) e fica aqui, com o jogo na linha quando ainda se sabe;
- um grupo por **jogo** (place da sessão aberta), com a imagem do jogo quando o
  `useGameIdentity` já a tem, o nome e quantas contas; os com mais contas primeiro;
- dentro dele, um subgrupo por **servidor** (Job ID): "Public server" / "Private server" (com
  cadeado) / "Server" (tipo desconhecido) e o Job ID curto (8 caracteres, inteiro no tooltip),
  ou "Server unknown" sem Job ID. Contas no mesmo servidor ficam juntas;
- **Not in a game**: cliente aberto sem sessão (ainda sem jogo, ou log não achado).

Cada grupo recolhe pelo título (padrão acordeão, `aria-expanded`) e tem uma caixa que marca as
contas dele (a do servidor marca as do servidor). Sem nenhuma sessão conhecida (fora do Windows,
onde o log não é lido) a lista fica simples, como antes. Dentro do grupo, a linha leva só o
**tempo em jogo** ao lado do estado; fora dele (atenção, sem jogo, lista simples), a segunda
linha leva:

- **jogo:** nome resolvido por `useGameIdentity` (o mesmo cache das outras telas); sem nome,
  "Place <id>". Nome comprido é cortado, com o texto inteiro no tooltip;
- **servidor:** "Public server" / "Private server" (com cadeado). O `! Joining game` do log não
  diz se o servidor é privado; quem diz é o launch que o app fez (`remember_launch_target`,
  campo `private`), e só enquanto a conta continua no place para onde foi mandada
  (`private_server_of`). Conta aberta pelo site ou teleportada para outro place: o tipo não
  aparece (não se sabe);
- **tempo em jogo**, andando sozinho ("45s", "12m", "1h 05m" — `formatDuration`), contado do
  início da sessão aberta: a entrada no jogo, ou o último teleporte (é quando o histórico abre
  a sessão nova). Cada linha tem o seu relógio de 1 s; o resto da lista não redesenha.

Na primeira linha, o estado curto: **Playing** (verde), **Reconnecting** (âmbar, com a queda ao
lado), a queda com motivo ou **Not responding** (`ClientHealthNote`), ou **Not in a game yet**
(log achado, sem jogo ainda). Caiu: a sessão fecha, e jogo e tempo somem até voltar.

De onde vem: comando `get_current_sessions` (commands/session_history.rs), que lê só a memória do
observador (`SessionHistoryObserver::current_sessions`: place, Job ID e `since_ms`, gravado no
`joined`/`teleported`). A tela ([useCurrentSessions.ts](../../src/hooks/useCurrentSessions.ts),
[CurrentSessionLine.tsx](../../src/components/session/CurrentSessionLine.tsx)) lê ao montar,
quando muda quem está rodando e a cada `session-history-changed` — **sem polling novo**. Só
Windows. Testes: `the_current_session_says_where_and_since_when`,
`the_server_kind_comes_from_where_the_app_sent_the_account` e "SessionPanel — sessão atual" em
[SessionPanel.test.tsx](../../src/components/session/SessionPanel.test.tsx). Harness:
`?scenario=client-drops&accounts=9`, página Session.

### Região e tempo no ar do servidor

O cabeçalho de cada servidor (e a segunda linha de quem está fora do grupo do jogo) mostra
também **onde fica o servidor e há quanto tempo está no ar**: "Miami, US · server up for 3 h".
Tudo vem do log do cliente, lido pelo monitor de saúde
([client_health.rs](../../src-tauri/src/commands/client_health.rs)), em cada join:

| Linha (cliente 0.743) | O que dá |
|---|---|
| `[FLog::Network] UDMUX Address = 128.116.21.33, Port = … \| RCC Server Address = 10.x…` | o IP **público** do servidor (o `RCC Server Address` e o `at 10.x` do `Joining game` são internos) |
| `[FLog::Output] Server Prefix: 0.743.0.7431129_20261014T235657Z_RCC_79c67` | quando o servidor subiu (o carimbo `AAAAMMDDTHHMMSSZ`, UTC) |

- Os dois vão para o `JoinedDestination` do join (um join novo zera os dois até as linhas
  novas chegarem) e saem em `health.server` (`{region, startedAtMs}`) no `get_running_instances`.
  O IP fica no backend.
- **Região:** o IP público passa pelo `lookup_ip_region` com cache (o mesmo da aba Servers —
  ver [server-choice.md](server-choice.md#região)), em segundo plano; a passada seguinte põe o
  rótulo formatado por `General.ServerRegionFormat`. IP privado nunca é geolocalizado (o
  classificador nem o aceita). Geolocalização sem resposta: sem região (só o tempo no ar), e
  nova tentativa em 10 min. Nenhuma chamada à API do Roblox.
- **Tempo no ar:** "server up for 25 min / 3 h / 3 d" (`serverUptimeLabel`,
  [serverUptime.ts](../../src/utils/serverUptime.ts)); carimbo ausente, ilegível (o parser não
  entra em pânico com linha fora do formato) ou mais de 5 min no futuro: não aparece.
- Contas no mesmo servidor dividem a informação: o cabeçalho usa a primeira que já sabe.
- Testes: `roblox_log_line_tests`, `client_server_region_tests`, `serverUptime.test.ts` e
  "shows each server's region…" em [SessionPanel.test.tsx](../../src/components/session/SessionPanel.test.tsx).
  Harness: `?scenario=client-drops&accounts=9`.

## Regras de negócio

- **Privacidade:** o evento só tem `at`, `userId`, `kind`, `placeId`, `jobId`, `dropKind`,
  `reason`, `code` e `message` (texto do kick, sem caracteres de controle, até 200) — travado em
  `the_file_holds_only_ids_places_codes_and_the_game_message`. Nunca cookie, ticket nem nome de
  conta. O nome do arquivo exportado leva só o id da conta.
- **Names hidden:** a seção não mostra nome de conta; o nome da conta que aparecer dentro de uma
  mensagem de kick é trocado pelo nome mascarado, na tela e no CSV (`maskNamesInText`).
- **CSV contra fórmula:** célula que começa com `=`, `+`, `-`, `@`, tab ou CR ganha `'` na frente
  (`csvCell`); aspas, vírgula, ponto e vírgula e quebra de linha vão entre aspas.
- **Retenção:** 90 dias e no máximo 50 000 eventos (os mais novos). A compactação roda ao abrir o
  app e a cada 5 000 linhas novas, regravando de forma atômica com a versão anterior em
  `RAMSessionHistory.jsonl.bak`. Linha quebrada (app morto no meio de uma escrita) é ignorada.
- **Só leitura:** nada aqui fecha, abre ou mexe em cliente; "Join again" é o launch normal.
- Só Windows: no macOS não há monitor de quedas, então não há histórico.

## Arquivos

| Arquivo | O quê | Backup |
|---|---|---|
| `RAMSessionHistory.jsonl` | um evento por linha, ex.: `{"at":1760000000000,"userId":7,"kind":"dropped","dropKind":"disconnected","reason":"connectionLost","code":277}` | **sim** (`DATA_FILES`); restaurar vale na hora (lido a cada consulta, gravado por append) |
| `RAMSessionHistory.jsonl.bak` | a versão anterior à última compactação | não |
| `RAMSessionHistory.open.json` | checkpoint de quem está em jogo (some quando ninguém está) | não (é transitório) |
| `exports/*.csv` | o que a pessoa exportou | não |

Teste do backup: `restoring_a_backup_brings_the_session_history_back_without_a_restart`.

## Harness

`bun run dev:ui` e abra `?scenario=history&accounts=4`; selecione TestAccount1 e abra o painel
(ícone de painel na Toolbar). A conta 1 tem duas semanas de sessões em três jogos (em jogo agora,
quedas com motivo, teleporte, saída, app fechado, moderação); a conta 2 não tem nada.

## Testes

`bun run t history` — Rust: `session_history_store_tests`, `session_history_build_tests`,
`session_history_observer_tests`; front: [AccountHistory.test.tsx](../../src/components/accounts/AccountHistory.test.tsx),
[sessionHistory.test.ts](../../src/utils/sessionHistory.test.ts).

## Armadilhas / cuidados

- **Não verificado com cliente real** (precisa do teste do dono): as linhas de teleporte e de saída
  vêm do classificador do pacote Quedas, calibrado com os logs da máquina do dono; uma versão nova do
  Roblox que mude o `! Joining game` faz o histórico parar de ver as entradas.
- Duas mudanças dentro da mesma passada de 2 s (sair e entrar de novo no mesmo servidor) viram uma
  sessão só.
- Depois de reiniciar o app, o cliente que continuou aberto volta como sessão nova (o início é a
  hora em que o app o reconheceu de novo); o tempo entre o último checkpoint e a reabertura não conta.
- O histórico só existe enquanto o app está aberto: o que a conta jogou com o app fechado não entra.
