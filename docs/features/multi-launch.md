# Launch múltiplo

## Objetivo

Lançar várias contas, **uma por vez e em sequência**, no mesmo place/Job ID, espaçando os launches para não disparar o captcha do Roblox, respeitando a versão de cada conta e permitindo cancelar a fila no meio.

## Onde fica o código

| Arquivo | Papel |
|---|---|
| [launch.rs](../../src-tauri/src/commands/launch.rs) | `launch_multiple` (Windows e macOS), `cancel_launch`, `next_account`, `cmd_kill_all_roblox` |
| [launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) | Helpers reutilizados do launch único (moderação, ticket, private join, PID, logs) |
| [platform/windows/tracker.rs](../../src-tauri/src/platform/windows/tracker.rs) | Flags `launcher_cancelled` e `next_account` |
| [platform/windows/launch.rs](../../src-tauri/src/platform/windows/launch.rs) | Spawn (protocolo com canal fixado ou old join) |
| [store.tsx](../../src/store.tsx) | `launchMultiple`, `killAllRobloxProcesses`, listeners `launch-progress` / `launch-complete` / `launch-log` |
| [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) | Chamador (1 conta → `joinServer`; várias → `launchMultiple`) |

## Fluxo

1. Frontend: `launchMultiple(userIds, target?)` → `invoke("launch_multiple", {userIds, placeId, jobId, launchData, shuffleJob})`. Se `placeId` não for número, usa `5315046213`. O alvo é passado explicitamente para evitar ler o place/job anterior do estado React. `shuffleJob` é **opcional** no backend (`Option<bool>`): quem não mandar o campo cai em `false`.
2. Backend lê `AccountJoinDelay` (default 8) e aplica o piso `MIN_JOIN_GAP_SECS = 8`. Valor negativo (INI editado à mão) é tratado como ausente — antes o cast `i64 -> u64` virava `u64::MAX` e a fila travava entre duas contas.
2.1. **Reserva da sequência** (`launch_queue_start`): se já houver um launch em andamento, o comando devolve o código `launch-already-active` e **nada é lançado** (ver [Uma sequência de launch por vez](#uma-sequência-de-launch-por-vez)). Vem antes de mexer no tracker, senão um lote recusado apagaria o cancelamento do lote que está rodando.
3. `tracker.reset_launch_cancelled()`.
4. Isolamento pré-launch roda **uma vez**, antes do loop.
5. Para cada `uid` na ordem recebida:
   1. Se `is_launch_cancelled()` → sai do loop.
   2. `launch-log` `start` ("Conta i/N — place …").
   3. Resolve versão da conta (`RobloxVersion` → `DefaultVersion` → catálogo → sistema). Falha → `launch-progress` com `error: "version-resolve-failed"`, espera 2 s, próxima conta.
   4. Guarda de versão: se houver cliente/pendente em outra versão → `launch-progress` com `error: "version-conflict"`, próxima conta (sem espera). O código fica no evento; a entrada da fila é marcada `Failed` com a **frase** de `version_conflict_message` (com as versões abertas), porque o painel de sessão desenha `entry.error` cru.
   5. Emite `launch-progress {userId, index, total}`.
   5.1. `shuffleJob` ligado e Job ID vazio: **esta conta** busca a lista de servidores públicos do place e sorteia o seu (`pick_shuffled_public_job`), logando `launch-log` `target` com o Job ID escolhido. Cada conta sorteia o seu — o lote se espalha em vez de entrar todo no mesmo servidor.
   6. Multi Roblox + `refresh_production_version().await` + patch do `ClientAppSettings.json` (na pasta da build production, ver [launch.md](launch.md#canal-do-roblox-e-a-tela-de-atualização-causa-raiz-e-fix)).
   7. `AutoCloseLastProcess`: se não conseguir fechar o cliente anterior, espera 2 s e pula.
   8. Auth ticket; erro → loga, marca moderada se for o caso, espera 2 s, pula.
   9. `resolve_private_join`; erro → espera 2 s, pula.
   10. Se `is_launch_cancelled()` → sai do loop (checagem logo antes do spawn: "Close All Roblox" clicado durante o auth/resolução não abre mais um cliente).
   11. Spawn (old join — pasta do catálogo ou `default_player_dir` quando sem versão do catálogo — ou protocolo), espera PID (12 s ou 180 s), rastreia, aplica perfil pós-launch e minimização.
   12. Se não é a última conta: espera (ver regras de espaçamento).
6. Ao fim (ou cancelamento) emite `launch-complete`.

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

- **Ordem:** exatamente a ordem do array `userIds` enviado pelo frontend (ordem de seleção/lista). O backend não reordena.
- **Mesmo destino para todos:** place e job são os escolhidos na UI; overrides por conta de "jogo salvo" (`SavedPlaceId`/`SavedJobId`) foram removidos de propósito.
- **VIP:** `launch_multiple` não recebe `joinVip`/`linkCode`; chama `resolve_launch_job(job, false, "")`. VIP só funciona se o Job ID vier como `vip:<código>` ou como link de servidor privado/share.
- **Follow user:** não suportado no multi (sempre `false`).
- **Shuffle (`shuffleJob`):** o sorteio é **por conta**, não por lote. Cada iteração chama `pick_shuffled_public_job(accounts, uid, placeId)`, que busca `/games/<place>/servers/Public` com o cookie daquela conta e escolhe o índice por relógio (`shuffle_server_index(nanos, n)`); duas contas quase sempre caem em servidores diferentes, e nada garante que fiquem juntas. Só vale quando o Job ID está **vazio** (`should_shuffle_server`): um Job ID explícito — inclusive `vip:<código>` — manda. Se a listagem falhar ou vier vazia, a conta segue com o Job ID vazio (servidor público qualquer), sem erro. O argumento é opcional (`Option<bool>`), então `invoke` sem o campo continua funcionando; `None` = não sortear.
- **Espaçamento síncrono (`AsyncJoin = false`):**
  - `delay = max(AccountJoinDelay, 8)` segundos, medido a partir do **início** da iteração da conta (desconta auth + espera de PID);
  - `wait = max(delay - elapsed, 5 s) + jitter`, com `jitter = 300 + (subsec_millis % 1200)` ms (300–1499 ms);
  - loga `launch-log` `wait` ("Aguardando Ns antes da próxima conta (anti-captcha)");
  - a espera é **fatiada** (250 ms) e termina antes da hora quando não sobra conta `queued` desta sequência ou quando o cancelamento global chega. Dormir o intervalo inteiro deixava o app preso depois de o usuário parar a fila — com o painel mostrando "0 na fila" e todo launch novo recusado.
  - Motivo: o Roblox pede "verify you're not a robot" quando resgates de auth ticket do mesmo IP chegam próximos demais.
- **Modo `AsyncJoin = true`:** após cada conta, espera o sinal `next_account()` (comando Tauri), o cancelamento ou a fila ficar sem conta esperando a vez, com teto de 120 s (polling de 500 ms). Sem espaçamento anti-captcha nesse modo.
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
- Um lote novo substitui a fila anterior **quando a anterior acabou** (ver abaixo); `launch_roblox` (conta única) alimenta a mesma fila com uma entrada, para a UI ser uniforme.

A interface disso é o **Painel de Sessão** — ver [ui-layout.md](ui-layout.md#painel-de-sessão).

## Uma sequência de launch por vez

Duas sequências ao mesmo tempo (dois cliques no botão, ou um launch de uma conta disparado durante uma fila) disputavam o mutex do Multi Roblox, o registro e o `ClientAppSettings.json` — que é global por pasta de versão — e a UI mostrava só a segunda, deixando a primeira invisível.

`launch_queue_start` passou a **reservar** a sequência, e a reserva tem **dono**. Vale para os quatro chamadores (launch de uma conta e fila, no Windows e no macOS):

- a reserva cobre as contas do lote **inteiro** antes de qualquer launch — reservar conta a conta dentro do laço deixaria a segunda sequência entrar no meio da primeira;
- com uma sequência reservada, o comando devolve o código `launch-already-active` sem tocar na fila existente, e o frontend mostra "Já existe um launch em andamento" (aviso, não erro — `isLaunchAlreadyActiveError` em [utils/robloxErrors.ts](../../src/utils/robloxErrors.ts)). No launch de **uma** conta o store devolve o resultado da tentativa (`started` / `refused` / `failed`, o tipo `LaunchAttempt`) em vez de deixar quem chamou adivinhar — a tela anunciava "seguindo com 1 conta..." em cima do aviso de recusa e também em cima da faixa de erro de uma falha comum;
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
| General | `EnableMultiRbx` | — | Obrigatório para vários clientes simultâneos |
| General | `AutoCloseLastProcess` | `false` | Fecha cliente anterior da conta antes de relançar |
| General | `AutoCloseRobloxForMultiRbx` | `false` | Mata clientes se o mutex não puder ser adquirido |
| General | `StartRobloxMinimized` | `false` | Minimiza janelas novas |
| Developer | `UseOldJoin`, `IsTeleport` | `false` | Iguais ao launch único |
| Isolation | `Mode` | `Off` | Roda uma vez antes da fila |

## Armadilhas / cuidados

- O que a conta em voo pode demorar é **limitado** desde que as chamadas HTTP do launch ganharam teto (ver [launch.md](launch.md#teto-de-tempo-das-chamadas-http-do-launch)); antes um endpoint do Roblox pendurado segurava a reserva por tempo indeterminado e todo launch novo era recusado.
- O cancelamento **interrompe** a espera de espaçamento (ela é fatiada em 250 ms e olha a fila e o flag do tracker), mas **não** interrompe a espera de PID de um cliente já spawnado nem o trabalho da conta em voo; a conta em andamento não abre cliente se o cancelamento chegar antes do spawn (checagem após auth ticket/private join). Ou seja: depois de parar a fila, o usuário espera no máximo o que falta da conta em voo — não mais o `AccountJoinDelay` inteiro.
- `launch_multiple` não restaura posição de janela salva (só o launch único faz isso).
- Diminuir o piso de 8 s / residual de 5 s volta a provocar captcha — o histórico de commits ("diminuindo delay", "ajuste de tempo no join") mostra que esse valor foi calibrado.
- Isolamento só é aplicado se nenhum Roblox estiver aberto quando a fila começa; com clientes abertos ele é pulado (`skipped`) e nada é fechado.
- No macOS o delay é `max(AccountJoinDelay, 12)` só quando `EnableMultiRbx`; não há jitter nem piso de 8 s (a sanitização de valor negativo vale nos dois, via `configured_join_delay_seconds`).
- O sorteio de servidor custa uma chamada HTTP por conta **dentro** da janela de espaçamento: ela entra no `elapsed` da iteração, então não soma tempo ao delay alvo, mas pode comer o residual e deixar o gap no piso de 5 s + jitter.
