# Ideias de outros gerenciadores

Catálogo de funcionalidades encontradas em outros gerenciadores de contas Roblox
open source que **valem a pena** trazer para o MultiAlt. Gerado pela skill
[`/competitor-scout`](../.claude/skills/competitor-scout/SKILL.md).

- **Última passada:** 09/10/2026 — 48 repositórios lidos (de 1 a 881 estrelas).
- **Como foi feito:** cada repositório foi clonado em
  `RAM-repositorios-diversos/` (fora deste repositório) e **só lido** — nada
  deles foi executado, compilado ou instalado. Triagem de segurança e de
  intenção em cada um; cada ideia foi cruzada com o código do MultiAlt.
- **Notas completas por repositório:** `RAM-repositorios-diversos/_notas/<dono>__<repo>.md`.

## Como usar este documento (para quem vai implementar)

1. **Não copiar código.** A ideia é reescrita no nosso padrão (Rust/TS, teste
   que falha primeiro, `endpoints::host`). Repositório GPL ou sem `LICENSE` =
   **só a ideia**; MIT/Apache permitem trecho com atribuição, mas mesmo assim
   prefira reescrever.
2. **Ler o original só como texto**, no link de "Onde ver". Nunca rodar.
3. A linha **Segurança** de cada ideia é requisito, não sugestão.
4. Ideia com "**confirmar**" depende de teste com cliente real antes de entrar.
5. Qualquer coisa que entre no binário (API nativa nova, COM, dependência) exige
   build + `bun run scan --release` (ver `CLAUDE.md`).

## Resumo — o que fazer primeiro

O buraco mais repetido: **6 de 48 projetos detectam a queda da conta pelo log do
cliente e reconectam sozinhos**. O MultiAlt no Windows só olha o título da
janela (`watcher.rs:76`, o log só é lido no macOS) e nunca relança — o
`AutoRelaunch` do Nexus é só gravado (`docs/features/nexus.md:109`).

| # | Ideia | Prioridade |
|---|---|---|
| 1 | [Queda detectada pelo log, com motivo](#1-queda-detectada-pelo-log-do-cliente-com-motivo) | alta |
| 2 | [Reconexão automática](#2-reconexão-automática-da-conta-que-caiu) | alta |
| 3 | [Reservar o nome `ROBLOX_singletonEvent` (teleporte)](#3-reservar-o-nome-roblox_singletonevent-teleporte-entre-places) | alta (confirmar) |
| 7 | [Gravar o cookie novo que o Roblox devolve](#7-gravar-o-cookie-novo-que-o-roblox-devolve) | alta |
| 8 | [Moderação da conta antes do launch](#8-status-de-moderação-antes-do-launch) | alta |
| 18 | [Otimização que segue o foco](#18-otimização-que-segue-o-foco) | alta |
| 24 | [AFK com teclas que se anulam](#24-afk-com-teclas-em-par-que-se-anulam) | alta |
| 19 | [Nome da conta no título da janela](#19-nome-da-conta-no-título-da-janela-do-roblox) | média-alta |

---

## A. Queda e reconexão

### 1. Queda detectada pelo log do cliente, com motivo
- **Prioridade:** alta — é o que o usuário de AFK/farm mais precisa e o que mais concorrentes já têm.
- **Situação no MultiAlt:** variação melhor do Watcher. Conferido: no Windows o `watcher.rs` decide por `windows_title_indicates_disconnect` (linha 76); a leitura incremental de log (`read_log_delta`, linhas 399–521) só roda no macOS.
- **O que faz:** acompanha o log de cada cliente e reconhece "Lost connection with reason", kick, servidor desligado, crash. Distingue **saída voluntária** e **teleporte** (não é queda) de queda real, e mostra o motivo ao usuário.
- **Onde ver:**
  - [`wokdsoul/XRM`](https://github.com/wokdsoul/XRM) — `src/Services/LogMonitorService.cs`, GPL-3.0
  - [`NotVeen/RoPilot`](https://github.com/NotVeen/RoPilot) — `src/WatchdogManager.cpp:184-275` e `:362-390` (também detecta a janela de crash), GPL-3
  - [`woogi999/pious-bootstrapper`](https://github.com/woogi999/pious-bootstrapper) — `core/clientlog.rs`, `service/watch.rs` (Rust, carência de 8 s para teleporte), sem licença
  - [`CodySimonds65/Roblox-Account-Manager`](https://github.com/CodySimonds65/Roblox-Account-Manager) — `client/Services/RobloxLogAutopsy.cs` (explica por que o cliente fechou: update obrigatório, canal…), Apache-2.0
  - [`ic3w0lf22/Roblox-Account-Manager`](https://github.com/ic3w0lf22/Roblox-Account-Manager) — `Classes/RobloxProcess.cs:106-210`. Atenção: lá a chave é lida como `" ExitIfNoConnection"` (com espaço, linha 81) e a regra nunca dispara.
- **Segurança:** limpo — só leitura de arquivo local. Os códigos de motivo divergem entre os projetos (267 kick, 273/276 "entrou em outro lugar", 277/279 conexão, 285 saiu): **confirmar** com logs reais antes de fixar tabela. Mudar o título da janela (ideia 19) quebra a regra atual por título — fazer as duas juntas.
- **Como faríamos aqui:** levar o leitor de log do macOS para o Windows dentro do `watcher.rs` (o log de cada PID já é achado pelo `external_clients.rs`), com classificador puro e testado. Emitir evento com o motivo para a Session.

### 2. Reconexão automática da conta que caiu
- **Prioridade:** alta — depende da ideia 1.
- **Situação no MultiAlt:** novo. O Watcher fecha e não relança; o Auto Rejoin (`botting.rs`) relança por relógio, não por queda; `AutoRelaunch` do Nexus não tem lógica.
- **O que faz:** quando a conta cai, relança **só aquela conta** no mesmo destino, com espera crescente (ex.: 10 s → 5 min), desiste depois de N tentativas seguidas sem ficar estável, nunca relança o que o usuário fechou, e ao reabrir o app volta **pausado**.
- **Onde ver:**
  - [`Toluwer/Fleet`](https://github.com/Toluwer/Fleet) — `src/main/keeper.js` (a referência mais completa), MIT só no README
  - [`Vaelixx/Roblox-Account-Manager`](https://github.com/Vaelixx/Roblox-Account-Manager) — `Services/WatchdogService.cs:326`, `DisconnectTrack.cs`, `RejoinBackoff.cs` (também usa a presença como reserva), sem licença
  - [`JeskoMts/Instance-Manager`](https://github.com/JeskoMts/Instance-Manager) — `Services/AutoReconnectService.cs`
  - [`TONYP7494/multi-roblox-manager`](https://github.com/TONYP7494/multi-roblox-manager) — `multi_roblox_manager.py:695-790`, `where` (`:793`, volta ao servidor onde a conta estava **de fato**, lido do log), `reopen_last` (`:5302`)
  - [`evanovar/RobloxAccountManager`](https://github.com/evanovar/RobloxAccountManager) — `src/features/auto_rejoin.py` (`_has_internet`: espera a internet voltar antes de relançar)
- **Segurança:** regras críticas — fechar só o próprio cliente; ticket novo a cada launch sem `refresh_account_session`; parar (não insistir) no motivo "conta entrou em outro lugar", senão as duas sessões se derrubam em laço.
- **Como faríamos aqui:** opção por conta (e o `AutoRelaunch` do Nexus passa a valer), reaproveitando a fila de `launch_shared.rs`.

### 3. Reservar o nome `ROBLOX_singletonEvent` (teleporte entre places)
- **Prioridade:** alta — **confirmar** com cliente real teleportando.
- **Situação no MultiAlt:** variação melhor de `platform/windows/core.rs`. Conferido: `enable_multi_roblox` (linha 110) fecha o Event antes de cada launch, mas não impede que um cliente o recrie. Quando um cliente teleporta ele refaz a checagem de instância única, encontra o Event recriado e um dos dois fecha.
- **O que faz:** logo depois de fechar os handles, o app cria um **Mutex** com o nome `ROBLOX_singletonEvent`. Como o nome passa a ser de outro tipo de objeto, nenhum cliente consegue recriar o Event.
- **Onde ver:** [`Zgoly/MultiBloxy`](https://github.com/Zgoly/MultiBloxy) — `MultiBloxy/Program.cs` (`OpenMutex` e a ação "Fix"), MIT. Mesma técnica em [`evanovar/RobloxMultiInstance`](https://github.com/evanovar/RobloxMultiInstance) — `Program.cs:56` ("TeleportService support"), GPL-3.
- **Segurança:** limpo, API pública de objeto nomeado, sem elevação. A versão do evanovar exige matar os clientes antes — **não** copiar essa parte.
- **Como faríamos aqui:** etapa extra em `enable_multi_roblox`, solta no `cleanup_multi_roblox_on_exit`, com teste em `singleton.rs`.

### 4. Cliente travado ("Não respondendo")
- **Prioridade:** média.
- **Situação no MultiAlt:** novo (nenhum uso de `IsHungAppWindow`).
- **O que faz:** se a janela de um cliente **nosso** fica "Não respondendo" por ~30 s, fecha e (com a ideia 2) relança.
- **Onde ver:** [`Toluwer/Fleet`](https://github.com/Toluwer/Fleet) — `keeper.js:151-171`, `native.js:470`; [`TONYP7494/multi-roblox-manager`](https://github.com/TONYP7494/multi-roblox-manager) — `close_frozen_games` (`:1397`).
- **Segurança:** só clientes rastreados pelo app; nunca os abertos pelo site.

### 5. Aviso no Discord por webhook
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** quando uma conta cai/volta, manda mensagem para um webhook **que o próprio usuário cola**, com o motivo da queda.
- **Onde ver:** [`wokdsoul/XRM`](https://github.com/wokdsoul/XRM) — `src/Services/WebhookService.cs`; [`NotVeen/RoPilot`](https://github.com/NotVeen/RoPilot) — `src/WebhookManager.cpp`; [`KIKISQQ/Roblox-Vault`](https://github.com/KIKISQQ/Roblox-Vault) — `Services/DiscordService.cs`.
- **Segurança:** desligado por padrão; a chamada sai do backend; a mensagem leva só alias e motivo — **nunca** cookie, senha ou Job ID de servidor privado. Validar que a URL é `discord.com/api/webhooks/…`.

### 6. Histórico de sessões e tempo de jogo
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** por conta: entrou, saiu, trocou de servidor, caiu (com motivo), foi moderada; botão "voltar a este servidor"; gráfico de 14 dias por jogo/conta; export CSV.
- **Onde ver:** [`Paryx-games/roblox-manager`](https://github.com/Paryx-games/roblox-manager) — `ram_core/src/session_history.rs`, `ram_ui/frontend/lib/historyExport.ts` (CSV protegido contra fórmula), MIT; [`Toluwer/Fleet`](https://github.com/Toluwer/Fleet) — `src/main/playtime.js` (não perde sessão se o app fechar à força); [`userinfected/altman`](https://github.com/userinfected/altman) — `src/components/history/log_parser.cpp`, MIT; [`sleaze5/RobloxAccountManager`](https://github.com/sleaze5/RobloxAccountManager) — `internal/logsexplorer/parser.go`, `visit_end.go`, MIT.
- **Segurança:** dados locais; CSV com `=`, `+`, `-`, `@` escapados.

## B. Sessão e conta

### 7. Gravar o cookie novo que o Roblox devolve
- **Prioridade:** alta — evita conta "expirando" sem motivo aparente.
- **Situação no MultiAlt:** variação melhor. Segundo o agente, hoje só lemos o `.ROBLOSECURITY` novo no sign-out e na troca de senha — **conferir** no `api/` antes de implementar.
- **O que faz:** qualquer resposta autenticada que traga `Set-Cookie: .ROBLOSECURITY=…` atualiza a conta salva.
- **Onde ver:** [`sleaze5/RobloxAccountManager`](https://github.com/sleaze5/RobloxAccountManager) — `internal/roblox/client.go` (`processSetCookie`), MIT.
- **Segurança:** o valor novo nunca vai para log; gravação pelo caminho normal criptografado.

**Relacionado, não adotar ainda:** o mesmo projeto renova o cookie com
`POST auth.roblox.com/v2/session/refresh` (`internal/roblox/services/auth.go`,
`RefreshCookie`) em vez do `signoutfromallsessionsandreauthenticate` que usamos e
que derruba as sessões. **Só depois de provar com conta de teste** que o v2 não
derruba as outras sessões.

### 8. Status de moderação antes do launch
- **Prioridade:** alta.
- **Situação no MultiAlt:** variação melhor. Conferido: não há chamada a `usermoderation`/`not-approved`; o grupo de contas moderadas só reage ao erro do launch.
- **O que faz:** mostra "banida até dd/mm", "advertida" ou "encerrada" na conta, antes de tentar abrir.
- **Onde ver:** [`Paryx-games/roblox-manager`](https://github.com/Paryx-games/roblox-manager) — `ram_core/src/api.rs` (`fetch_moderation_message`), MIT; [`userinfected/altman`](https://github.com/userinfected/altman) — `src/utils/network/roblox/auth.h` (`checkBanStatus`), MIT.
- **Segurança:** leitura — `read_without_refresh`, nunca `run_with_session_retry`. Host via `endpoints::host`. Cuidado: o altman manda o cookie para `presence.roproxy.com` (proxy de terceiro) — **não** reproduzir.

### 9. Verificar todas as contas de uma vez
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** botão que confere a validade de todos os cookies e marca as mortas, separando 401 (morto) de 429/rede (tentar depois).
- **Onde ver:** [`evanovar/RobloxAccountManager`](https://github.com/evanovar/RobloxAccountManager) — `src/features/cookie_validator.py`, GPL-3.0; [`joaoswu/RBLXManager`](https://github.com/joaoswu/RBLXManager) — `MainWindow.xaml.cs` (`VerifyAllCookies_Click`), MIT.
- **Segurança:** só leitura, sem refresh; ritmo limitado para não tomar 429.

### 10. Mensagem clara quando o Roblox pede verificação
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor — hoje o `auth.rs` devolve o 403 cru.
- **O que faz:** se o auth ticket voltar com `rblx-challenge-*`, mostrar "conclua a verificação no navegador".
- **Onde ver:** [`azertxz/roblox-account-manager`](https://github.com/azertxz/roblox-account-manager) — `Classes/Account.cs:183`, GPL-3.0 (fork do ic3). Reserva de prioridade baixa: `clientAssertion` no pedido do ticket, em [`TheFakePlayerGame/RACCM`](https://github.com/TheFakePlayerGame/RACCM) — `Classes/Account.cs:111-138`.
- **Segurança:** não tentar resolver o desafio automaticamente.

### 11. Launch com o tracker do aparelho
- **Prioridade:** média — **confirmar** medindo.
- **Situação no MultiAlt:** variação de `launch_shared.rs`/`launch.rs`.
- **O que faz:** o autor mediu o cliente travando 5–8 s várias vezes por sessão quando cada conta usa um `BrowserTrackerId` próprio; ele manda o do aparelho (de `appStorage.json`) e usa `PlaceLauncher.ashx?request=RequestGameJob` com `joinAttemptId`/`joinAttemptOrigin`.
- **Onde ver:** [`VladDerK1ng/RobloxKeeper`](https://github.com/VladDerK1ng/RobloxKeeper) — `src/RobloxAuth.cs`, MIT.
- **Segurança:** só leitura de arquivo local do Roblox. Medir antes de mudar.

### 12. Adicionar conta por Quick Login
- **Prioridade:** média.
- **Situação no MultiAlt:** novo (hoje só temos o lado de *aprovar* um código).
- **O que faz:** o app mostra um código; o usuário aprova no celular já logado e a conta entra sem digitar senha no app.
- **Onde ver:** [`woogi999/pious-bootstrapper`](https://github.com/woogi999/pious-bootstrapper) — `core/roblox.rs`, sem licença.
- **Segurança:** fluxo oficial do Roblox. **Não** trazer a importação de cookies do navegador do mesmo projeto — é o padrão que antivírus tratam como infostealer.

## C. Launch e organização

### 13. Presets de launch e agendamento
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** salvar "estas contas → este jogo/servidor" e abrir com um clique; agendar abrir/fechar por horário — o fechamento só fecha os clientes que **aquela** tarefa abriu.
- **Onde ver:** [`Vaelixx/Roblox-Account-Manager`](https://github.com/Vaelixx/Roblox-Account-Manager) — `Models/LaunchPreset.cs`, `PresetService.cs`, `SchedulerService.cs`, sem licença; [`joaoswu/RBLXManager`](https://github.com/joaoswu/RBLXManager) — `LaunchGroup`, MIT.
- **Segurança:** agendamento roda só com o app aberto — nada de tarefa agendada do Windows.

### 14. Fila que espera o cliente entrar no jogo
- **Prioridade:** média (melhor depois da ideia 1).
- **Situação no MultiAlt:** variação melhor — a fila hoje espera um tempo fixo.
- **O que faz:** passa para a próxima conta quando o log diz que a anterior entrou no jogo, com teto (~20 s).
- **Onde ver:** [`JeskoMts/Instance-Manager`](https://github.com/JeskoMts/Instance-Manager) — `Services/LaunchService.cs:34,145` (licença contraditória: só a ideia).

### 15. Conferir a assinatura do `RobloxPlayerBeta.exe`
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** antes de abrir, confere que o exe é assinado pela Roblox Corporation.
- **Onde ver:** [`JeskoMts/Instance-Manager`](https://github.com/JeskoMts/Instance-Manager) — `Services/RobloxExecutableValidator.cs`.
- **Segurança:** `WinVerifyTrust` é API nativa nova no binário — build + scan.

### 16. Diagnóstico "o launch não faz nada"
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** checklist (Roblox instalado, pasta gravável, disco, API alcançável, multi-instância) + aviso quando o handler `roblox-player` aponta para exe inexistente ou há bootstrapper de terceiro instalado + processos do Roblox sem janela há 150 s (mostrar e fechar no clique).
- **Onde ver:** [`Vaelixx/Roblox-Account-Manager`](https://github.com/Vaelixx/Roblox-Account-Manager) — `HealthCheckService.cs:120`; [`VladDerK1ng/RobloxKeeper`](https://github.com/VladDerK1ng/RobloxKeeper) — `RobloxInstall.cs`, `GhostWatch.cs`, MIT.
- **Segurança:** só leitura do registro. O RobloxKeeper também **reescreve** o handler e tem "Repair" que fecha todos os clientes — não trazer.

### 17. Configurações em três camadas (global → jogo → conta)
- **Prioridade:** baixa.
- **Onde ver:** [`CodySimonds65/Roblox-Account-Manager`](https://github.com/CodySimonds65/Roblox-Account-Manager) — `src/RobloxAccountManager.Core/Launch/LaunchSettingsResolver.cs`, Apache-2.0.

## D. Janelas e desempenho

### 18. Otimização que segue o foco
- **Prioridade:** alta.
- **Situação no MultiAlt:** variação melhor de `optimization.rs` — hoje a política é aplicada uma vez, no launch.
- **O que faz:** a janela em uso volta ao normal; as de fundo ganham prioridade baixa e teto de CPU; 35 s de carência ao abrir. Reage à troca de foco por evento, sem polling.
- **Onde ver:** [`NotVeen/RoPilot`](https://github.com/NotVeen/RoPilot) — `src/Optimizer.cpp` (`MonitorLoop`), GPL-3; [`8damon/TASX-Roblox-Optimizer`](https://github.com/8damon/TASX-Roblox-Optimizer) — `Infra/winhook.cc` (`SetWinEventHook`), MIT; [`VladDerK1ng/RobloxKeeper`](https://github.com/VladDerK1ng/RobloxKeeper) — `PerformanceManager.cs` (`EffectiveFor`, `CeilingTick`: limite de memória que pagina em vez de matar).
- **Segurança:** o `SetWinEventHook` tem que ficar **fora** dos arquivos do AFK (o `afk_input_safety_tests` proíbe ganchos lá). Não trazer do TASX: tarefa agendada com UAC, desligar turbo do processador, matar `RobloxCrashHandler`.

### 19. Nome da conta no título da janela do Roblox
- **Prioridade:** média-alta.
- **Situação no MultiAlt:** novo.
- **O que faz:** cada janela mostra o alias da conta — saber quem é quem na barra de tarefas.
- **Onde ver:** [`wokdsoul/XRM`](https://github.com/wokdsoul/XRM) — `src/Services/WindowRenamerService.cs`, GPL-3.0; [`evanovar/RobloxAccountManager`](https://github.com/evanovar/RobloxAccountManager) — `src/features/window_renamer.py`.
- **Segurança:** **conflito:** a regra de título do Watcher compara o título exato e passaria a ver todos os clientes renomeados como "caídos". Fazer junto com a ideia 1.

### 20. Volume ao vivo por cliente (fundo mudo)
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor — hoje só mudamos o volume antes do launch.
- **O que faz:** mutar os clientes de fundo e desmutar o que está em foco, pelo mixer do Windows, com o jogo aberto.
- **Onde ver:** [`Agzes/AntiAFK-RBX`](https://github.com/Agzes/AntiAFK-RBX) — `MuteProcessByPid` (`AntiAFK-RBX.cpp:1495`), MIT; [`PookiePepelsss/MultiRoblox-RAM`](https://github.com/PookiePepelsss/MultiRoblox-RAM) — `RobloxNative.cs` (`AudioControl`), PolyForm Noncommercial (só a ideia).
- **Segurança:** COM de áudio é código nativo novo — build + scan.

### 21. Devolver as configurações do Roblox ao fechar
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor — gravamos volume/qualidade/tela no `GlobalBasicSettings_13.xml` e não devolvemos o original, então o jogo aberto pelo site herda.
- **Onde ver:** [`Gaiiiaaa-GH/Gaiiiablox`](https://github.com/Gaiiiaaa-GH/Gaiiiablox) — `src/Core.cs` (`RobloxPrefs.ApplyScalar/RestoreAll`), MIT. Variação: settings por conta guardados de volta quando a janela fecha — [`woogi999/pious-bootstrapper`](https://github.com/woogi999/pious-bootstrapper) `core/robloxsettings.rs`.
- **Segurança:** não deixar o arquivo como somente-leitura (o XRM faz isso — descartado).

### 22. Grade de janelas menor que o mínimo e sem moldura
- **Prioridade:** média — **confirmar** com cliente real.
- **Situação no MultiAlt:** variação melhor — hoje a grade aumenta a célula para caber (`windowing.rs:843-848`).
- **O que faz:** `SWP_NOSENDCHANGING` faz o Roblox aceitar janela menor que ~800×600; opção de tirar a moldura e encostar as janelas.
- **Onde ver:** [`M7ilan/roblox-manager`](https://github.com/M7ilan/roblox-manager) — `Services/WindowArranger.cs:247-253`, `ApplyStyle`, MIT.

### 23. Manter o PC acordado durante AFK/Auto Rejoin
- **Prioridade:** média — barato.
- **Situação no MultiAlt:** novo.
- **Onde ver:** [`Agzes/AntiAFK-RBX`](https://github.com/Agzes/AntiAFK-RBX) — `SetThreadExecutionState` (`:10548`), MIT; [`Overdusts/roblox-multi-instance-launcher`](https://github.com/Overdusts/roblox-multi-instance-launcher) — `Form1.cs:132`, `576-581`.
- **Segurança:** soltar o estado quando o AFK/Auto Rejoin para e ao sair do app.

## E. AFK

### 24. AFK com teclas em par que se anulam
- **Prioridade:** alta.
- **Situação no MultiAlt:** variação melhor de `afk.rs`.
- **O que faz:** em vez de clique/tecla que mexe o personagem, manda pares que se anulam (câmera ← →, zoom O/I, passo W/S) — conta como atividade, o personagem não sai do lugar, sem o clique de ~1,2 s por conta. Setas enviadas como tecla estendida.
- **Onde ver:** [`VladDerK1ng/RobloxKeeper`](https://github.com/VladDerK1ng/RobloxKeeper) — `NudgeMethod.cs`, `InputSender.SendScan`, MIT; [`Agzes/AntiAFK-RBX`](https://github.com/Agzes/AntiAFK-RBX) — `AntiAFK-RBX.cpp:2144` (zoom I/O).
- **Segurança:** continua passando pelo `platform/windows/input.rs` (o único ponto de SendInput).

### 25. Adiar o AFK quando há janela em tela cheia
- **Prioridade:** média.
- **Onde ver:** [`VladDerK1ng/RobloxKeeper`](https://github.com/VladDerK1ng/RobloxKeeper) — `NudgePolicy.cs` (com prazo limite antes do kick).
- **Segurança:** só a parte de tela cheia. A parte que usa `GetLastInputInfo` é proibida pelo nosso `afk_input_safety_tests`.

## F. Proteção do app

### 26. Cookie copiado fora do histórico do Win+V
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor do aviso de copiar credencial.
- **O que faz:** ao copiar cookie, marca o conteúdo para não ir ao histórico/nuvem do clipboard e o apaga sozinho depois de alguns segundos.
- **Onde ver:** [`Vaelixx/Roblox-Account-Manager`](https://github.com/Vaelixx/Roblox-Account-Manager) — `ClipboardService.cs:34`.

### 27. Trancar o app por inatividade
- **Prioridade:** média (para quem usa senha).
- **O que faz:** tranca por inatividade ou ao minimizar; espera crescente após senhas erradas.
- **Onde ver:** [`Vaelixx/Roblox-Account-Manager`](https://github.com/Vaelixx/Roblox-Account-Manager) — `LockService.cs`.

### 28. "Reportar problema" com log anonimizado
- **Prioridade:** média.
- **O que faz:** abre um issue no GitHub já preenchido com o log, caminho do perfil e nomes de conta escondidos. O app não envia nada sozinho.
- **Onde ver:** [`M7ilan/roblox-manager`](https://github.com/M7ilan/roblox-manager) — `Windows/FeedbackWindow.xaml.cs`, MIT.
- **Segurança:** testar a anonimização (cookie nunca pode aparecer).

## G. Versões do Roblox

### 29. Catálogo e download mais robustos
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor de `versions.rs`.
- **O que faz:** catálogo pelo `DeployHistory.txt` do CDN oficial; espelhos do CDN com teste de conectividade (hoje só `setup-aws`); cache de pacotes por hash entre builds; mostrar as FastFlags que o Roblox **recusou** (linha "Denied local configuration for:" do log).
- **Onde ver:** [`N3XT3R1337/RiftStrap`](https://github.com/N3XT3R1337/RiftStrap) e [`tsukiforge/BoneFish`](https://github.com/tsukiforge/BoneFish) — `RobloxInterfaces/Deployment.cs`, `Bootstrapper.cs`, MIT; [`woogi999/pious-bootstrapper`](https://github.com/woogi999/pious-bootstrapper) — `core/clientlog.rs`.
- **Segurança:** conferir hash de tudo que é baixado (os dois bootstrappers não conferem no updater deles). Não gravar canal no registro (regra crítica).

## H. macOS

### 30. Multi-instância de verdade no Mac
- **Prioridade:** alta se o macOS for prioridade.
- **Situação no MultiAlt:** pelo código, `platform/macos/launch.rs` só faz `open <url>`; com um cliente aberto o macOS tende a entregar a URL a ele em vez de abrir outro — **confirmar num Mac**.
- **O que faz:** uma cópia **inalterada** do Roblox.app por conta (assinatura original conferida), HOME isolado (sessão, cache e settings separados) e a URL de launch enviada ao PID certo por Apple Event; antes, conferir a versão no `clientsettingscdn …/MacPlayer`.
- **Onde ver:** [`intraducine/Roblox-Account-Manager-Mac`](https://github.com/intraducine/Roblox-Account-Manager-Mac) — `RobloxLauncher.swift` (`prepareUnmodifiedCopy`, `launchUnmodified`, `sendLaunchURL`, `verifyCurrentOfficialVersion`), GPL-3.0; [`benkoppe/Multiblox`](https://github.com/benkoppe/Multiblox) — `src/manager.rs` (Rust, atualização atômica da cópia), MIT.
- **Segurança:** **nunca** alterar/reassinar o app (o `vmmie/MacOS-RobloxAccountManager` faz — risco de ToS).
- **Extras no Mac:** anti-AFK sem tirar o foco (`CGEventPostToPid`, benkoppe `src/automation.rs`); posicionar janelas pela API de Acessibilidade (intraducine `WindowLayoutController.swift`); FPS por `FramerateCap` no XML, já que a `DFIntTaskSchedulerTargetFps` estaria fora da allowlist — confirmar pelo log.

## Para decisão do dono

Ideias boas que esbarram numa regra do projeto ou trazem um host novo. Não
recomendadas por padrão; viram opção só se ele quiser.

- **Adiar o AFK enquanto o usuário mexe no PC** (Agzes) e **forçar o foco** (RoPilot): dependem de `GetLastInputInfo`/`AttachThreadInput`, proibidos de propósito no `afk.rs:2004-2029`.
- **Região dos servidores em lote pelo RoValra** ([`sleaze5`](https://github.com/sleaze5/RobloxAccountManager) `internal/integration/rovalra/client.go`): mais rápido que o `join-game-instance` por servidor, mas é host de terceiro — só desligado por padrão.
- **Seguir quem escondeu o servidor** com follow temporário e unfollow garantido por diário ([`Paryx-games`](https://github.com/Paryx-games/roblox-manager) `ram_core/src/join_user.rs`, `temporary_join`): mexe no perfil social da conta.
- **`LOCALAPPDATA` próprio por conta** ([`Overdusts`](https://github.com/Overdusts/roblox-multi-instance-launcher) `Core/RobloxLauncher.cs`, `SetupInstanceIsolation`): separaria settings/cookie local/logs de contas que rodam juntas (o isolamento atual não separa), mas muda onde o cliente procura as builds — investigar antes.

## Repositórios analisados (09/10/2026)

Veredito: **L** legítimo · **S** suspeito · **M** malicioso. Estrelas na data da busca.

| Repositório | ★ | Licença | Veredito | Aproveitável |
|---|---|---|---|---|
| [ic3w0lf22/Roblox-Account-Manager](https://github.com/ic3w0lf22/Roblox-Account-Manager) | 881 | GPL-3.0 | L (práticas antigas) | 1, 2 |
| [Agzes/AntiAFK-RBX](https://github.com/Agzes/AntiAFK-RBX) | 94 | MIT | L | 20, 23, 24 |
| [evanovar/RobloxAccountManager](https://github.com/evanovar/RobloxAccountManager) | 89 | GPL-3.0 | L | 2, 9, 19 |
| [Zgoly/MultiBloxy](https://github.com/Zgoly/MultiBloxy) | 54 | MIT | L | 3 |
| [Avaluate/MultipleRobloxInstances](https://github.com/Avaluate/MultipleRobloxInstances) | 53 | — | L (pede admin à toa) | nada |
| [PookiePepelsss/MultiRoblox-RAM](https://github.com/PookiePepelsss/MultiRoblox-RAM) | 45 | PolyForm NC | L com ressalvas (exe sem fonte, mexe em handles do cliente) | 2, 20 |
| [JunkBeat/AntiAFK-Roblox](https://github.com/JunkBeat/AntiAFK-Roblox) | 19 | MIT | L | só baixas |
| [RoRvzzz/RoRvzzz-Account-Manager](https://github.com/RoRvzzz/RoRvzzz-Account-Manager) | 13 | — | L (API local sem senha) | nada |
| [userinfected/altman](https://github.com/userinfected/altman) | 11 | MIT | L, **vaza cookie** para roproxy | 6, 8 |
| [VladDerK1ng/RobloxKeeper](https://github.com/VladDerK1ng/RobloxKeeper) | 8 | MIT | L (o melhor de AFK) | 11, 16, 18, 24, 25 |
| [tsukiforge/BoneFish](https://github.com/tsukiforge/BoneFish) | 8 | MIT | L (otimizações invasivas) | 29 |
| [KIKISQQ/Roblox-Vault](https://github.com/KIKISQQ/Roblox-Vault) | 7 | GPL-3.0 | L | 5 |
| [erikpog757/RobloxMultiInstance](https://github.com/erikpog757/RobloxMultiInstance) | 6 | — | L | nada |
| [Paryx-games/roblox-manager](https://github.com/Paryx-games/roblox-manager) | 5 | MIT | L (lê memória do cliente — descartado) | 6, 8 |
| [ex9d/sentra](https://github.com/ex9d/sentra) | 5 | GPL-3.0 | **S** (`new Function` em HTML baixado, sniper com proxy) | nada |
| [nickytvv/The-Better-Roblox-Account-Manager](https://github.com/nickytvv/The-Better-Roblox-Account-Manager) | 5 | — | **S** (updater sem hash como admin, cookies no console) | nada |
| [Tunneliahover/roblox-account-manager-launcher](https://github.com/Tunneliahover/roblox-account-manager-launcher) | 5 | — | **M** (isca: código do RAM sem projeto, download aponta para zip de outro repo) | nada |
| [8damon/TASX-Roblox-Optimizer](https://github.com/8damon/TASX-Roblox-Optimizer) | 4 | MIT | L (invasivo) | 18 |
| [aksoPX/akso-alt-roblox-alt](https://github.com/aksoPX/akso-alt-roblox-alt) | 4 | — | L (RAM original traduzido) | nada |
| [JeskoMts/Instance-Manager](https://github.com/JeskoMts/Instance-Manager) | 3 | contraditória | L | 2, 14, 15 |
| [NotVeen/RoPilot](https://github.com/NotVeen/RoPilot) | 3 | GPL-3 | L | 1, 5, 18 |
| [TheEpicFace007/roblox-alt-account-manager](https://github.com/TheEpicFace007/roblox-alt-account-manager) | 3 | — | L (abandonado) | nada |
| [Gaiiiaaa-GH/Gaiiiablox](https://github.com/Gaiiiaaa-GH/Gaiiiablox) | 2 | MIT | L | 21 |
| [N3XT3R1337/RiftStrap](https://github.com/N3XT3R1337/RiftStrap) | 2 | MIT | L arriscado (mexe na memória do cliente) | 29 |
| [Overdusts/roblox-multi-instance-launcher](https://github.com/Overdusts/roblox-multi-instance-launcher) | 2 | — | L descuidado | 23 |
| [TheFakePlayerGame/RACCM](https://github.com/TheFakePlayerGame/RACCM) | 2 | GPL-3.0 | código L, **distribuição S** (pede instalar certificado do autor) | 10 |
| [Toluwer/Fleet](https://github.com/Toluwer/Fleet) | 2 | MIT (só README) | L | 2, 4, 6 |
| [Vaelixx/Roblox-Account-Manager](https://github.com/Vaelixx/Roblox-Account-Manager) | 2 | — | L (o melhor gerenciador C#) | 2, 13, 16, 26, 27 |
| [hoanglonggg79/BloxVault](https://github.com/hoanglonggg79/BloxVault) | 2 | — | L (nem fala com o Roblox) | nada |
| [intraducine/Roblox-Account-Manager-Mac](https://github.com/intraducine/Roblox-Account-Manager-Mac) | 2 | GPL-3.0 | L | 30 |
| [joaoswu/RBLXManager](https://github.com/joaoswu/RBLXManager) | 2 | MIT | L | 9, 13 |
| [sleaze5/RobloxAccountManager](https://github.com/sleaze5/RobloxAccountManager) | 2 | MIT | L | 6, 7 |
| [vmmie/MacOS-RobloxAccountManager](https://github.com/vmmie/MacOS-RobloxAccountManager) | 2 | GPL-3.0 | L (reassina o cliente) | nada |
| [CodySimonds65/Roblox-Account-Manager](https://github.com/CodySimonds65/Roblox-Account-Manager) | 1 | Apache-2.0 | L (pede admin) | 1, 17 |
| [KujouReiketsu0501/RobloxMultiLauncher](https://github.com/KujouReiketsu0501/RobloxMultiLauncher) | 1 | MIT | L | só baixas |
| [M7ilan/roblox-manager](https://github.com/M7ilan/roblox-manager) | 1 | MIT | L | 22, 28 |
| [N3XT3R1337/RiftCE](https://github.com/N3XT3R1337/RiftCE) | 1 | — | **S** (updater executa exe de outra conta sem assinatura) | nada |
| [TONYP7494/multi-roblox-manager](https://github.com/TONYP7494/multi-roblox-manager) | 1 | — | L | 2, 4 |
| [TheGlauberShow/GigaBloxianXYZ](https://github.com/TheGlauberShow/GigaBloxianXYZ) | 1 | — | L (esqueleto) | nada |
| [Xan3vo/BSSaltManager](https://github.com/Xan3vo/BSSaltManager) | 1 | — | L, muito invasivo (RDP Wrapper, cria usuários, HKLM) | nada |
| [azertxz/roblox-account-manager](https://github.com/azertxz/roblox-account-manager) | 1 | GPL-3.0 | L | 10 |
| [benkoppe/Multiblox](https://github.com/benkoppe/Multiblox) | 1 | MIT | L | 30 |
| [bimoso/RobloxAccountManager](https://github.com/bimoso/RobloxAccountManager) | 1 | GPL-3.0 | **S** (scripts fly/noclip, status de executors, exe commitado) | nada |
| [evanovar/RobloxMultiInstance](https://github.com/evanovar/RobloxMultiInstance) | 1 | GPL-3 | L | 3 |
| [iamEvanYT/roblox-account-manager](https://github.com/iamEvanYT/roblox-account-manager) | 1 | — | L (extensão Raycast) | nada |
| [wokdsoul/XRM](https://github.com/wokdsoul/XRM) | 1 | GPL-3.0 | L | 1, 5, 19 |
| [woogi999/pious-bootstrapper](https://github.com/woogi999/pious-bootstrapper) | 0 | — | L com ressalvas (lê cookies do navegador, ganchos globais) | 1, 12, 21, 29 |
| [xfwil/roblox-accounts-manager](https://github.com/xfwil/roblox-accounts-manager) | 0 | — | L, inseguro (API local entrega cookies sem senha) | nada |

## Bandeira vermelha (não clonados)

Iscas do nicho. Não abrir, não baixar, não indicar.

- **Estrelas falsas em lote**, todos `HTML`, 68–70 ★, atualizados no mesmo dia, nomes "...-Forge/-Orbit/2026": `mehrab-6767/SessionX-Orbit`, `kshong/roblox-multi-account-orbit`, `MendAura/roblox-session-forge`, `MAHDI292-sys/rustblox-account-forge`, `ItaloFelixSantos/Roblox-Account-Manager-Python`, `AFKvipproplayer/Roblox-MultiLauncher-Next`, `rajnathyadav6/Fisch-DevOps-Forge`, `donguyenbao/place-sync-hub`, `Jimeshhacker/MultiRoblox-Orchestrator`, `HeisJeremiah1/NeuzBlox-MultiRunner`, `Ferrerodillo/Nexo-Account-Vault`, `0m4rxze/Roblox-Account-Switcher`.
- **Sem código e com descrição de marketing** ("ultimate", "premium", "pro-tier"): `kich2/Multiroblox-extension-2026` (88 ★), `lifet-dot/Roblox-Extension-Multi-Account-Manager` (76 ★), `toskaman/RBX-Manager-2`, `rapha30/RobloxAccountManagerPlus-*` ("Download …"), e dezenas de repositórios de 0–1 ★ com linguagem vazia.
- **Abuso explícito:** `katicmartin854-eng/Multi-Toll-To-Hack-Roblox-Accounts`, `zZarby/RChecker` (checker de combolist), `wanz-hub/WANZZ` (bypass de key system).
- **Clonado e confirmado como isca:** `Tunneliahover/roblox-account-manager-launcher` (ver tabela).

## Descartado em todos os projetos (padrões que não entram)

- Matar todos os clientes (ao abrir, ao fechar, no "kill switch", no "Repair").
- Ler/escrever memória do cliente, fechar handles **dentro** do processo do Roblox para outros fins, modificar CoreScripts, FFlags que mexem em física/anúncios/vantagem.
- `handle64.exe`/`handle.exe` baixado e executado; atualizador que roda exe sem conferir assinatura/hash.
- Gravar canal ou reescrever o handler `roblox-player` no registro; deixar arquivo de settings do Roblox somente-leitura.
- Importar cookies do navegador; cookie em texto puro, base64 ou XOR "criptografado"; cookie em log/console; API local sem senha.
- Macros, ganchos globais de teclado/mouse, trade bot, sniper de username, scraper de grupos, bot de visitas.
- Sessão RDP por alt (exige admin + RDP Wrapper, que antivírus marcam).
