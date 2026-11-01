# Launch múltiplo

## Objetivo

Lançar várias contas, **uma por vez e em sequência**, no mesmo place/Job ID, espaçando os launches para não disparar o captcha do Roblox, respeitando a versão de cada conta e permitindo cancelar a fila no meio.

## Onde fica o código

| Arquivo | Papel |
|---|---|
| [launch.rs](../../src-tauri/src/commands/launch.rs) | `launch_multiple` (Windows e macOS), `cancel_launch`, `next_account`, `cmd_kill_all_roblox` |
| [launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) | Helpers reutilizados do launch único (moderação, ticket, private join, PID, logs) |
| [platform/windows/tracker.rs](../../src-tauri/src/platform/windows/tracker.rs) | Flags `launcher_cancelled` e `next_account`. Clientes abertos fora do app são adotados no mesmo tracker — ver [external-clients.md](external-clients.md) |
| [platform/windows/launch.rs](../../src-tauri/src/platform/windows/launch.rs) | Spawn: protocolo (`launch_url`, build de produção) ou old join (pasta do catálogo, ou `default_player_dir` — build do canal lido do registro) |
| [store.tsx](../../src/store.tsx) | `launchMultiple`, `killAllRobloxProcesses`, listeners `launch-progress` / `launch-complete` / `launch-log` |
| [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) | Chamador (1 conta → `joinServer`; várias → `launchMultiple`) |
| [launch_presets.rs](../../src-tauri/src/commands/launch_presets.rs) | Chamador: presets de launch, à mão ou pelo horário, chamam `launch_multiple` direto do Rust — ver [presets.md](presets.md) |

## Fluxo

1. Frontend: `launchMultiple(userIds, target?)` → `invoke("launch_multiple", {userIds, placeId, jobId, launchData, shuffleJob})`. Se `placeId` não for número, usa `5315046213`. O alvo é passado explicitamente para evitar ler o place/job anterior do estado React. `shuffleJob` é **opcional** no backend (`Option<bool>`): quem não mandar o campo cai em `false`.
2. Backend lê `AccountJoinDelay` (default 8) e aplica o piso `MIN_JOIN_GAP_SECS = 8`. Valor negativo (INI editado à mão) é tratado como ausente — antes o cast `i64 -> u64` virava `u64::MAX` e a fila travava entre duas contas.
2.1. **Entrada na fila** (`launch_queue_admit`): fila livre, este comando reserva a sequência e segue; com um launch em andamento, o lote vai **para o fim da fila que já roda** e o comando volta na hora com `{ appended: true, added }` (ver [Uma sequência de launch por vez](#uma-sequência-de-launch-por-vez)). Vem antes de mexer no tracker, senão um lote que entra no fim apagaria o cancelamento do lote que está rodando.
3. `tracker.reset_launch_cancelled()`.
4. Isolamento pré-launch roda **uma vez**, antes do loop.
5. O laço da fila (`run_launch_queue_windows`; no macOS `run_launch_queue_macos`) pede a **próxima conta `queued` da fila compartilhada** (`next_or_finish`) — não itera a lista que recebeu, então quem entrou no fim durante o lote também é lançado, com o place/job/launch data/VIP/sorteio **do lote dele**. Para cada conta:
   0. Se houve uma conta antes: espera (ver regras de espaçamento) e volta ao topo — durante a espera a conta pode ter sido cancelada ou outro lote pode ter entrado.
   1. Se `is_launch_cancelled()` → sai do loop.
   2. `launch-log` `start` ("Account i/N — place …", frase em inglês traduzida pela tela).
   3. Resolve versão da conta (`RobloxVersion` → `DefaultVersion` → catálogo → sistema). Falha → `launch-progress` com `error: "version-resolve-failed"`, espera 2 s, próxima conta.
   4. Guarda de versão (`version_guard_blocks`, com o toggle `Versions.AllowLaunchOnOpenVersion` lido a cada conta — ver [launch.md](launch.md)): se houver cliente/pendente em outra versão → `launch-progress` com `error: "version-conflict"`, próxima conta (sem espera). O código fica no evento; a entrada da fila é marcada `Failed` com a **frase** de `version_conflict_message` (com as versões que impedem aquela conta), porque o painel de sessão desenha `entry.error` cru.
   5. Emite `launch-progress {userId, index, total}`.
   5.1. `shuffleJob` ligado e Job ID vazio: **esta conta** busca a lista de servidores públicos do place e sorteia o seu (`pick_shuffled_public_job`), logando `launch-log` `target` com o Job ID escolhido. Cada conta sorteia o seu, e um servidor já dado a outra conta do mesmo jogo sai do sorteio — o lote se espalha em vez de entrar todo no mesmo servidor.
   6. Multi Roblox + `refresh_production_version().await` + resolução da pasta de onde o cliente vai abrir (`client_dir`, ver [launch.md](launch.md#onde-o-clientappsettingsjson-é-gravado)).
   7. `AutoCloseLastProcess`: se não conseguir fechar o cliente anterior, espera 2 s e pula.
   8. Auth ticket; erro → loga, marca moderada se for o caso, espera 2 s, pula.
   9. `resolve_private_join`; erro → espera 2 s, pula.
   10. Se `is_launch_cancelled()` → sai do loop (checagem logo antes do spawn: "Close All Roblox" clicado durante o auth/resolução não abre mais um cliente).
   10.1. Patch do `ClientAppSettings.json` (na pasta do passo 6) e do `GlobalBasicSettings_13.xml` — **o último passo antes do spawn**. Até 03/10/2026 ele vinha no passo 6, e os clientes que o lote já tinha aberto reescreviam o XML durante o fechamento/ticket: a alt abria com o tamanho de janela da conta principal. Ver [launch.md](launch.md#tamanho-da-janela-conferido-pelo-pid).
   11. Spawn (old join — pasta do catálogo ou `default_player_dir` quando sem versão do catálogo — ou protocolo), espera PID (12 s ou 180 s), rastreia, aplica perfil pós-launch, dispara a conferência da janela pelo PID (`spawn_client_window_enforcement`, numa task à parte — a fila não espera; com `AutoArrangeGrid`, ela também põe a janela na grade) e a minimização.
   12. Guarda esta conta como a anterior; a espera acontece no topo da próxima volta, só se ainda houver conta `queued`.
6. Sem nenhuma conta `queued`, `next_or_finish` fecha a fila **sob o mesmo lock** em que viu a fila vazia (deixa de ser ativa e de aceitar lote): nenhum lote entra no fim de um laço que já decidiu sair. Ao fim (ou cancelamento) emite `launch-complete`.

```mermaid
sequenceDiagram
    participant UI as Frontend
    participant M as launch_multiple
    participant T as ProcessTracker
    UI->>M: invoke("launch_multiple", ids)
    M->>T: reset_launch_cancelled()
    loop cada conta
        M->>T: is_launch_cancelled()?
        M-->>UI: launch-progress {userId, index, total}
        M->>M: ticket → private join
        M->>T: is_launch_cancelled()? (antes do spawn)
        M->>M: spawn → PID
        M-->>UI: launch-log (auth/spawn/pid/wait)
        M->>M: sleep(max(delay - elapsed, 5 s) + jitter)
    end
    UI->>M: cmd_kill_all_roblox (Close All)
    M->>T: cancel_launch()
    M-->>UI: launch-complete
```

## Regras de negócio

- **Ordem:** exatamente a ordem do array `userIds` enviado pelo frontend (ordem de seleção/lista). O backend não reordena. Lotes que entram durante a fila vão para o fim, na ordem em que chegaram.
- **Mesmo destino para o lote:** place e job são os escolhidos na UI para aquele lote; overrides por conta de "jogo salvo" (`SavedPlaceId`/`SavedJobId`) foram removidos de propósito. Cada entrada da fila guarda o pedido do seu lote (`LaunchRequest`), então 3 contas no jogo A e 2 no jogo B, na mesma fila, entram cada uma no seu jogo.
- **VIP:** `launch_multiple` não recebe `joinVip`/`linkCode`; chama `resolve_launch_job(job, false, "")`. VIP só funciona se o Job ID vier como `vip:<código>` ou como link de servidor privado/share.
- **Follow user:** não suportado no multi (sempre `false`).
- **Shuffle (`shuffleJob`):** o sorteio é **por conta**, não por lote. Cada iteração chama `pick_shuffled_public_job(accounts, uid, placeId)`, que busca `/games/<place>/servers/Public` com o cookie daquela conta só com servidores com vaga, e escolhe pelo relógio (`pick_shuffle_server`) entre os que **nenhuma outra conta recebeu** nos últimos 15 min (`SHUFFLE_ASSIGNMENTS`, por place). Antes cada conta sorteava sem saber das outras e duas caíam juntas com frequência. Só repete servidor quando todos com vaga já foram dados. Só vale quando o Job ID está **vazio** (`should_shuffle_server`): um Job ID explícito — inclusive `vip:<código>` — manda. Se a listagem falhar ou vier vazia, a conta segue com o Job ID vazio (servidor público qualquer), sem erro. O argumento é opcional (`Option<bool>`), então `invoke` sem o campo continua funcionando; `None` = não sortear.
- **Espaçamento síncrono (`AsyncJoin = false`):**
  - `delay = max(AccountJoinDelay, 8)` segundos, medido a partir do **início** da iteração da conta (desconta auth + espera de PID);
  - `wait = max(delay - elapsed, 5 s) + jitter`, com `jitter = 300 + (subsec_millis % 1200)` ms (300–1499 ms);
  - loga `launch-log` `wait` ("Waiting Ns before the next account (anti-captcha)", traduzida pela tela);
  - a espera é **fatiada** (250 ms) e termina antes da hora quando não sobra conta `queued` desta sequência ou quando o cancelamento global chega. Dormir o intervalo inteiro deixava o app preso depois de o usuário parar a fila — com o painel mostrando "0 na fila" e todo launch novo recusado.
  - Motivo: o Roblox pede "verify you're not a robot" quando resgates de auth ticket do mesmo IP chegam próximos demais.
- **Espera pelo jogo (`WaitForGameJoin = true`, padrão, só Windows, só com `AsyncJoin = false`):** em vez da espera fixa, a fila segue **assim que o log do cliente diz que a conta entrou no jogo** (`in_game` do monitor de quedas, [watcher.md](watcher.md#quedas-lidas-do-log-do-cliente)). Três prazos, todos medidos como a espera fixa (do início da iteração, com o resíduo de 5 s e o jitter): piso `8 s` (o anti-captcha, nunca antes dele, nem com a conta já em jogo), espera fixa `max(AccountJoinDelay, 8)` e teto `max(20 s, AccountJoinDelay)` (`JOIN_WAIT_CAP_SECS`). Regra (`join_wait_done`, testes `launch_join_wait_tests`):
  - log achado e a conta **em jogo** → segue (depois do piso). Com `AccountJoinDelay` 30 e a conta em jogo aos 12 s, segue aos 12 s;
  - log achado e ainda **carregando** → espera até o teto;
  - **sem log** (o monitor ainda não achou, ou não acha) → a espera fixa de antes. Queda ou cliente fechado antes de entrar também caem aqui.
  - Sem PID detectado, ou com a opção desligada, é a espera fixa de antes. Logs: "Esperando a conta entrar no jogo antes da próxima (entre Xs e Ys)" e "Entrou no jogo: próxima conta".
- **Modo `AsyncJoin = true`:** após cada conta, espera o sinal `next_account()` (comando Tauri), o cancelamento ou a fila ficar sem conta esperando a vez, com teto de 120 s (polling de 500 ms). Sem espaçamento anti-captcha nesse modo. Nenhuma tela nem script embutido chama `next_account` hoje, então na prática a espera é sempre os 120 s (menos na última conta) — a descrição em Settings diz "2 minutos entre contas".
- **Cancelamento:** `cmd_kill_all_roblox` (botão "Close All Roblox") e `cancel_launch` setam `launcher_cancelled`. O loop verifica no início de cada iteração, **logo antes do spawn** de cada conta, no loop de espera do AsyncJoin e em cada fatia da espera entre contas. `stop_launch_queue` não seta esse flag — ele só cancela as contas `queued`, e é por isso que a espera também olha a fila. `reset_launch_cancelled` só é chamado no início de um novo `launch_multiple`.
- **Versão por conta:** cada conta resolve sua própria versão; mas uma conta cuja versão diverge dos clientes já abertos é pulada (`version-conflict`). Na prática todas as contas de um lote devem estar na mesma versão.
- **Contas moderadas:** não são filtradas antes; a falha no auth ticket as move para o grupo `moderadas`, emite `account-moderated` (o frontend recarrega a lista e mostra toast) e a fila segue.
- **Erro de Multi Roblox** (`ensure_multi_roblox_enabled`) **aborta a fila inteira** (retorna `Err`), diferente dos demais erros por conta.
- **Eventos:** `launch-progress {userId, index, total[, error, message]}` (UI mostra "Launching account i/N"), `launch-log`, `launch-complete` (UI limpa estado após 1,5 s).
- **Restart de clientes** (`restartRobloxClients` no store): fecha cada cliente lançado pelo app com `cmd_kill_roblox`, espera 250 ms e relança via `launchMultiple` (ou `joinServer` se for 1).

## Fila observável e cancelamento

O lote deixou de ser uma caixa-preta: o backend mantém a fila como estado e emite o evento `launch-queue` a cada transição, com uma entrada por conta.

| Estado | Quando |
|---|---|
| `queued` | ainda não chegou a vez |
| `launching` | entre o início do trabalho da conta e o spawn |
| `done` | PID do cliente confirmado |
| `failed` | erro (mensagem em `error`), inclusive "PID não detectado no tempo esperado" |
| `cancelled` | pulada por cancelamento |

Comandos: `get_launch_queue()` (snapshot para montar a UI), `cancel_account_launch(userId)` e `stop_launch_queue()`.

**Regras (decididas com o usuário):**

- **Cancelar nunca fecha cliente.** Conta já lançada (`done`) não é afetada; cancelar devolve `false`.
- `stop_launch_queue` cancela só quem está `queued`. Quem está `launching` termina — não dá para abortar no meio do auth — e quem já entrou continua jogando.
- Isso é **diferente** do "Close All Roblox" (`cancel_launch` + matar clientes). São ações distintas na UI de propósito. O `cancel_launch` também esvazia a fila na hora, para o painel não ficar mostrando contas que não vão mais entrar.
- Um lote novo substitui a fila anterior **quando a anterior acabou**; com ela rodando, entra no fim (ver abaixo). `launch_roblox` (conta única) alimenta a mesma fila com uma entrada, para a UI ser uniforme.
- Cada entrada leva o seu `placeId`/`jobId` (o resto do pedido — launch data, código VIP — fica no backend). Quando a fila mistura jogos, o Painel de Sessão mostra o jogo em cada linha; com um jogo só, não repete.
- Cancelar uma conta e parar a fila valem igual para as contas que entraram no fim.
- **Conta errada pergunta antes de parar a fila:** quando o log de um cliente que o app abriu mostra que ele entrou como outra conta do Roblox, a fila espera a pessoa dizer se continua (`General.WrongAccountQueue`: `ask` padrão, `continue` ou `stop`, que faz o mesmo que o Stop) — sem fechar cliente nenhum. Ver [watcher.md](watcher.md#conta-errada-no-cliente).

A interface disso é o **Painel de Sessão** — ver [ui-layout.md](ui-layout.md#painel-de-sessão-sessionpanel).

## Uma sequência de launch por vez

Duas sequências ao mesmo tempo (dois cliques no botão, ou um launch de uma conta disparado durante uma fila) disputavam o mutex do Multi Roblox, o registro e o `ClientAppSettings.json` — que é global por pasta de versão — e a UI mostrava só a segunda, deixando a primeira invisível.

**Lote durante a fila entra no fim dela (11/10/2026).** Até aqui o segundo launch era recusado: o dono lançou 3 contas no jogo A e, com a fila ainda entrando, mais 2 no jogo B — a Choose Game já tinha ido para a página Session, o aviso "Já existe um launch em andamento" era um toast fácil de perder, e as 2 não abriam. Agora `LaunchQueue::admit` põe o lote **no fim da fila que roda**, e o laço do dono o lança na vez dele. Continua **uma sequência só** — os motivos acima seguem valendo; o que mudou é que a segunda leva espera em vez de ser descartada:

- cada entrada guarda o pedido do lote dela (`LaunchRequest`: place, Job ID cru, launch data, follow user, VIP/link, sorteio), e o laço lança cada conta com o seu — o lote B abre exatamente como abriria sozinho;
- conta que já espera a vez (`queued`/`launching`) **não entra duas vezes**; uma que já terminou neste lote (`done`/`failed`/`cancelled`) volta para o fim, sem linha duplicada;
- o comando que entrou no fim volta na hora com `{ appended: true, added }`, e a tela avisa "Added N accounts to the launch queue" (ou "Already waiting in the launch queue", com `added = 0`) — sem o aviso de recusa. Com a fila rodando, o store também não troca o progresso/"entrando" da tela pelos do lote novo;
- o laço do dono só fecha a fila quando não sobra ninguém `queued`, e fecha **sob o mesmo lock** em que viu a fila vazia (`next_or_finish`): a partir daí a fila não aceita lote (`accepting = false`) até a reserva cair. Quem chega nesse intervalo (milissegundos) espera a reserva cair e abre a sua sequência (`Admission::Closing`, teto `LAUNCH_CLOSING_GRACE` = 3 s, depois recusa);
- "Close All Roblox" (`cancel_launch` / `cmd_kill_all_roblox`) também fecha a fila para lotes novos: o laço vai cancelar o resto quando acordar, e um lote que entrasse nesse meio seria cancelado sem nunca ser tentado;
- o launch de **uma** conta que abriu a fila, depois do próprio launch, entrega quem entrou no meio ao mesmo laço (`run_launch_queue_windows`) numa task à parte, com a espera contada do início dele;
- o **preset** cujo lote entrou no fim espera as contas dele saírem da fila (`wait_until_launched`) antes de contar quais clientes abriu — senão contaria zero;
- a **reconexão automática** reserva com `try_start`, que não aceita lote no fim (ela não tem laço para lançar mais ninguém): um launch durante ela continua recusado com `launch-already-active`, e ela, do lado dela, continua tentando de novo quando a fila está ocupada.

Testes: `launch_queue_tests` (`a_second_batch_is_appended_to_the_running_sequence`, `appended_entries_keep_their_own_target`, `an_account_already_waiting_is_not_added_twice`, `the_owner_takes_the_next_entry_from_the_shared_queue`, `the_sequence_finishes_only_when_nothing_is_left_queued`, `stopping_the_queue_also_cancels_the_appended_entries`…) e, no frontend, `store.test.ts` ("um launch durante a fila entra no fim dela…") e `SessionPanel.test.tsx` ("a queue that mixes games…").

`launch_queue_start` passou a **reservar** a sequência, e a reserva tem **dono**. Vale para os quatro chamadores (launch de uma conta e fila, no Windows e no macOS):

- a reserva cobre as contas do lote **inteiro** antes de qualquer launch — reservar conta a conta dentro do laço deixaria a segunda sequência entrar no meio da primeira;
- com uma sequência reservada que **não** aceita lote no fim (a da reconexão automática, ou uma que não sai dentro de `LAUNCH_CLOSING_GRACE`), o comando devolve o código `launch-already-active` sem tocar na fila existente, e o frontend mostra "Já existe um launch em andamento" (aviso, não erro — `isLaunchAlreadyActiveError` em [utils/robloxErrors.ts](../../src/utils/robloxErrors.ts)). No launch de **uma** conta o store devolve o resultado da tentativa (`started` / `refused` / `failed`, o tipo `LaunchAttempt`) em vez de deixar quem chamou adivinhar — a tela anunciava "seguindo com 1 conta..." em cima do aviso de recusa e também em cima da faixa de erro de uma falha comum. A recusa aparece em **dois** lugares, não em três: o **toast** do store (vale para qualquer origem do launch, inclusive as que não têm linha inline) e a **linha inline** da tela de Choose Game (fica onde o clique aconteceu). A linha de status do rodapé foi tirada — ela e o toast são os dois globais e apareciam juntos, somando três cópias simultâneas de uma frase de uma linha. E o "Launching X…" que o launch escreve no rodapé **antes** do `invoke` (5 s) sai quando a recusa volta (`withdrawActionStatus`, só se ainda for a linha que está lá) — senão o rodapé passava 5 s dizendo que lançava a conta recusada;
- **reservada** é um estado próprio da fila (`reserved`), não algo inferido das entradas. Uma fila momentaneamente toda em estado final **não** significa que o laço acabou: o comando de uma conta ainda passa pelo perfil pós-launch (1,5 s por padrão) depois do `done`, e um lote parado pelo usuário ainda tem o laço vivo dormindo o intervalo anti-captcha. Enquanto o dono não sair, nenhuma sequência nova entra;
- cada reserva aceita ganha uma **geração**. Toda escrita na fila feita de dentro do launch é assinada por ela (`LaunchSequenceGuard::mark` / `cancel_remaining` / `finish` / `abort`) e é ignorada quando a fila já é de outro lote. Sem isso, o laço de um lote antigo saindo apagava a fila do lote novo (a primeira conta dele virava `failed`, o resto `cancelled`) e o `is_cancelled` dele respondia "não fui cancelado" para contas que não eram suas — lançando em paralelo com o lote novo;
- a liberação é no `Drop` da reserva, não em cada `return`: vale para o lote que terminou, para o erro por conta, para o erro que aborta o lote, para o `?` no meio, para o cancelamento do usuário e para um `panic` (o perfil de build não usa `panic = abort`). O `Drop` ainda fecha o que ficou pendente — `queued` vira `cancelled`, `launching` vira `failed` — e é o **único** ponto que solta a reserva;
- cancelar (`stop_launch_queue`, `cancel_launch`, "Close All Roblox") **não** libera a reserva: cancelar só marca as contas que ainda não foram lançadas. O laço ainda vai acordar, e liberar antes dele sair era exatamente como dois lotes rodavam juntos. O que o usuário espera até poder lançar de novo é só o que falta do trabalho da conta em voo — a espera entre contas termina sozinha quando não há mais nada `queued` (ver as regras de espaçamento acima);
- lote **sem conta nenhuma** não é sequência: o comando sai antes de reservar (e a fila também recusa), senão ele rodaria o isolamento pré-launch e seguraria a fila para não abrir nada;
- a fila vive no processo, então um fechamento do app (ou uma queda) começa com a fila livre — não existe reserva presa em disco;
- o **Auto Rejoin** não passa por aqui: o ciclo dele não é uma sequência de launch da UI, e continua como era. O mesmo vale para o launch do servidor HTTP local, que tem caminho próprio e nunca alimentou a fila.

## Configurações relacionadas

| Seção | Chave | Default | Efeito |
|---|---|---|---|
| General | `AccountJoinDelay` | `8` | Espaçamento alvo entre contas (piso 8 s; valor negativo = default) |
| General | `AsyncJoin` | `false` | Espera sinal `next_account` em vez de tempo |
| General | `WaitForGameJoin` | `true` | Só Windows, com `AsyncJoin` desligado: segue quando o log diz que a conta entrou no jogo (piso 8 s, teto 20 s ou o delay, se maior); sem log, a espera fixa |
| General | `EnableMultiRbx` | — | Obrigatório para vários clientes simultâneos |
| General | `AutoCloseLastProcess` | `false` | Fecha cliente anterior da conta antes de relançar |
| General | `AutoCloseRobloxForMultiRbx` | `false` | Mata clientes se o mutex não puder ser adquirido |
| General | `StartRobloxMinimized` | `false` | Minimiza janelas novas |
| General | `AutoArrangeGrid` | `true` | Cada janela nova vai para a primeira célula livre da grade (lido a cada conta) — ver [ui-layout.md](ui-layout.md#grade-de-janelas) |
| Developer | `UseOldJoin`, `IsTeleport` | `false` | Iguais ao launch único |
| Isolation | `Mode` | `Off` | Roda uma vez antes da fila |

## Armadilhas / cuidados

- O que a conta em voo pode demorar é **limitado** desde que as chamadas HTTP do launch ganharam teto (ver [launch.md](launch.md#teto-de-tempo-das-chamadas-http-do-launch)); antes um endpoint do Roblox pendurado segurava a reserva por tempo indeterminado e todo launch novo era recusado.
- O cancelamento **interrompe** a espera de espaçamento (ela é fatiada em 250 ms e olha a fila e o flag do tracker), mas **não** interrompe a espera de PID de um cliente já spawnado nem o trabalho da conta em voo; a conta em andamento não abre cliente se o cancelamento chegar antes do spawn (checagem após auth ticket/private join). Ou seja: depois de parar a fila, o usuário espera no máximo o que falta da conta em voo — não mais o `AccountJoinDelay` inteiro.
- `launch_multiple` não restaura posição de janela salva (só o launch único faz isso); com `AutoArrangeGrid` ligado, as contas sem janela própria vão para a grade, uma célula livre por janela, sem mexer nas que já estão abertas.
- Diminuir o piso de 8 s / residual de 5 s volta a provocar captcha — o histórico de commits ("diminuindo delay", "ajuste de tempo no join") mostra que esse valor foi calibrado.
- Isolamento só é aplicado se nenhum Roblox estiver aberto quando a fila começa; com clientes abertos ele é pulado (`skipped`) e nada é fechado.
- No macOS o delay é `max(AccountJoinDelay, 12)` só quando `EnableMultiRbx`; não há jitter nem piso de 8 s (a sanitização de valor negativo vale nos dois, via `configured_join_delay_seconds`).
- O sorteio de servidor custa uma chamada HTTP por conta **dentro** da janela de espaçamento: ela entra no `elapsed` da iteração, então não soma tempo ao delay alvo, mas pode comer o residual e deixar o gap no piso de 5 s + jitter.
