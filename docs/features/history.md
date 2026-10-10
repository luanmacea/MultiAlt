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
- **Export CSV:** monta o CSV na tela (nomes dos jogos já resolvidos) e o backend grava em
  `<pasta de dados>/exports/multialt-history-<userId>-<data>.csv` (com BOM, para o Excel) e abre o
  Explorer com o arquivo selecionado. Colunas: início, fim, minutos, jogo, Place ID, servidor,
  como terminou, código, mensagem.
- Conta sem nada: "Nothing yet. Games this account plays while MultiAlt is open show up here."

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
