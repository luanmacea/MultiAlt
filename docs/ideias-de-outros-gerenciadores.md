# Ideias de outros gerenciadores

Catálogo de funcionalidades encontradas em outros gerenciadores de contas Roblox
open source que **valem a pena** trazer para o MultiAlt. Gerado pela skill
[`/competitor-scout`](../.claude/skills/competitor-scout/SKILL.md).

- **Passadas:** 09/10/2026 — 48 repositórios (de 1 a 881 estrelas); **14/10/2026** — mais 35 novos, 5 forks do RAM original e 6 da primeira passada que receberam commits (89 no total). Ideias novas da segunda: [seção I](#i-segunda-passada-14102026).
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

## Situação de cada ideia (atualizado em 15/10/2026)

**Leia isto antes de propor uma ideia nova:** o que já saiu, o que foi
descartado (e por quê) e o que ainda está aberto. Uma passada nova do
`/competitor-scout` só deve trazer ideia que **não** esteja aqui como feita
ou descartada — ou trazer um motivo novo para rever um descarte. Ordem de
trabalho e pacotes: [plano-ideias.md](plano-ideias.md).

### Feitas

| # | Ideia | Saiu em | Observação |
|---|---|---|---|
| 1 | Queda detectada pelo log, com motivo | 1.3.0 | Motivo na Session e no resumo "While you were away" |
| 2 | Reconexão automática | 1.3.0 | Mesmo jogo e mesmo servidor privado; por conta ou para todas |
| 3 | Reservar o `ROBLOX_singletonEvent` (teleporte) | 2.0.0 | **Experimental, desligada.** O método antigo continua o padrão; falta teste do dono teleportando com 2+ contas. **14/10:** três projetos usam a reserva como padrão e um diz que o método antigo parou nas builds de outubro — ver [ideia 32](#32-motivo-novo-para-ligar-a-reserva-do-roblox_singletonevent-ideia-3) |
| 4 | Cliente travado ("Não respondendo") | 1.3.0 | Aviso depois de ~30 s; fechar é opção |
| 6 | Histórico de sessões e tempo de jogo | 2.0.0 | Painel da conta |
| 7 | Gravar o cookie novo que o Roblox devolve | 1.2.0 | |
| 8 | Moderação da conta antes do launch | 1.2.0 | Conta moderada também pode ser adicionada |
| 9 | Verificar todas as contas de uma vez | 1.2.0 | |
| 10 | Mensagem clara quando o Roblox pede verificação | 1.2.0 | |
| 12 | Adicionar conta por Quick Login | 2.0.0 | |
| 13 | Presets de launch e agendamento | 2.0.0 | |
| 14 | Fila que espera o cliente entrar no jogo | 1.3.0 | Desde a 2.0.0 a fila também aceita lotes novos no fim |
| 16 | Diagnóstico "o launch não faz nada" | 2.0.0 | Settings › Launch |
| 18 | Otimização que segue o foco | 2.0.0 | **Mudou:** a versão que desacelerava as janelas de fundo (prioridade baixa, EcoQoS, timer grosso) fazia o jogo atrasar e a conta "teleportar" para os outros jogadores. Hoje o fundo roda normal e só devolve memória ("Free memory from the windows you're not using") — ver [performance.md](features/performance.md) |
| 19 | Nome da conta no título da janela | 1.3.0 | "conta — Roblox" |
| 20 | Volume ao vivo por cliente (fundo mudo) | 2.0.0 | Nas duas edições (feature `live-audio`) |
| 21 | Devolver as configurações do Roblox ao fechar | 2.0.0 | Desligada por padrão |
| 22 | Grade de janelas menor e sem moldura | 2.0.0 | Controles da grade na página Console |
| 23 | Manter o PC acordado | 1.3.0 | |
| 25 | Adiar o AFK com janela em tela cheia | 2.0.0 | Ligada por padrão |
| 26 | Cookie copiado fora do histórico do Win+V | 1.2.0 | |
| 27 | Trancar o app por inatividade | 2.0.0 | Desligada por padrão |
| 28 | "Reportar problema" com log anonimizado | 2.0.0 | Resumo anônimo no "Send feedback" |
| 5 | Aviso no Discord por webhook | `develop` (15/10) | Desligado por padrão; o usuário escolhe cada evento (caiu, reconexão desistiu, não responde, reconectou, launch falhou, abriu em outra conta). Ver [webhook.md](features/webhook.md) |
| 24 | AFK com teclas em par que se anulam | `develop` (15/10) | → ← e ↑ ↓ |
| 29 / 40 | Download da build sem cair no instalador | `develop` (15/10) | **Era bug:** uma consulta de versão lenta logo depois de abrir o app levava o launch ao handler `roblox-player:` mesmo com a build no disco. Hoje: host de reserva, novas tentativas, última build gravada em disco, 4 tentativas por pacote |
| 33 | Não lançar com outro cliente segurando o `ROBLOX_singletonEvent` | `develop` (15/10) | **Era bug:** o launch seguia e o cliente antigo fechava. Hoje confere de novo depois de fechar e pula o launch com o motivo |
| 35 | Conferir que o cliente entrou na conta certa | `develop` (15/10) | Pelo `userid` do log; avisa e para a fila, nunca fecha o cliente. `challengedByGcs` ficou de fora (nunca visto num log real) |
| 36 | Região e tempo no ar do servidor na Session | `develop` (15/10) | Pelas linhas `UDMUX Address` e `Server Prefix` do log, sem chamada ao Roblox |
| 38 | Exportar as contas com senha própria | `develop` (15/10) | Settings › Backups; abre em qualquer PC pelo import que já existia |
| 42 | Preset que aponta para um grupo | `develop` (15/10) | O grupo é resolvido a cada launch. Server hop ficou de fora; reinício a cada N horas já existia no Auto Rejoin |
| 44 | AFK devolve cada janela como estava | `develop` (15/10) | Re-minimiza sem ativar outra janela e devolve a ordem das janelas. O foco do usuário já voltava antes; isto tira as piscadas no meio do ciclo |
| 46 | Esconder os nomes no título da janela do Roblox | 1.3.0 | Já tinha saído junto com a ideia 19 (`hiding_names_swaps_the_title_for_the_masked_one`); estava aberta por engano |
| 48 | Esperar o jogador ficar disponível e entrar | `develop` (15/10) | Aba Follow: confere a cada 30 s, recua em erro/429, desiste em 2 h |
| 50 | Servidores privados da conta na aba Servers | `develop` (15/10) | `games/v1/games/{place}/private-servers`, entra pelo caminho de VIP que já existia |
| 52 | Pequenas: preset pela bandeja, gravação depois de qualquer launch, desfazer remoção de contas, renomear grupo | `develop` (15/10) | |

Além do catálogo, a 2.0.0 trouxe o que não veio de concorrente: página
Session agrupada por jogo/servidor, estado ao vivo na lista de contas,
"Closed" com "Rejoin", resumo "While you were away", gravações (importar do
TinyTask), parada de emergência por chacoalhar o mouse, teto de memória por
cliente, configurações divididas em seções e tamanho automático para tela
grande.

### Descartadas (com o motivo)

| # / pedido | Motivo |
|---|---|
| 15 — conferir a assinatura do `RobloxPlayerBeta.exe` | Ganho pequeno (as builds já vêm do CDN oficial, baixadas pelo app) e exige API nativa nova (`WinVerifyTrust`), risco de antivírus na edição padrão. Se voltar, só na edição completa |
| 17 — configurações em três camadas (global → jogo → conta) | Complica a tela mais do que ajuda; não vale agora |
| Adiar o AFK enquanto o usuário mexe no PC / forçar o foco | Dependem de `GetLastInputInfo`/`AttachThreadInput`, proibidos de propósito (`afk_input_safety_tests`). O foco depois do ciclo foi resolvido sem eles (1.2.1, issue #23) |
| Gravar macros ao vivo dentro do app | Abandonado pelo dono em 11/10/2026: o TinyTask já grava, e o app **importa** o `.rec` |
| Executor / injetor de scripts no Roblox (pedido no Discord, 12/10/2026) | Viola as regras do Roblox, contorna o anti-cheat e acaba com o "0 alertas" de antivírus. Quem usa executor próprio já tem o **Nexus** (edição completa) |
| Seguir em massa para inflar seguidores (issue #27, 13/10/2026) | Inflar seguidores é contra as regras do Roblox, o "seguir" automatizado esbarra em captcha (que o app não contorna) e expõe o projeto. Um "Seguir" de **uma conta por vez**, como no site, foi oferecido como alternativa |
| Portátil (`.exe` solto) e setup NSIS | Saem de 1/75 no VirusTotal; a release publica só o MSI (`PUBLISH_PORTABLE: "false"`) |
| 31 — "o Roblox fechou porque outra janela abriu" | **Interpretação errada** (avaliação de 15/10/2026 nos logs reais 0.743 do dono): `EventWaitResultUpdate` aparece em **todo** fechamento normal e `mutex result is 0X000001` não apareceu em nenhum dos 15 logs. Marcaria toda saída como erro; e a saída de propósito já não dispara a reconexão (`Tearing down`/`leaveUGCGameInternal`) |
| 39 — versão velha não grava por cima | Risco pequeno no nosso formato (contas com campos livres, settings em INI). Só reavaliar se um JSON tipado ganhar campo importante |
| 45 — miniaturas ao vivo e posição em fração do monitor | Miniatura DWM não desenha dentro do WebView e congela com o cliente minimizado; a fração resolve mais que o problema. O que sobra de útil: descartar posição salva que caiu fora da tela |
| 49 — identidade do aparelho por conta | **Leitura errada do concorrente** (a conta nova não recebe cópia limpa) e só funciona com nenhum cliente aberto; briga com a trava do `RobloxCookies.dat` (correção do 773) |
| `LOCALAPPDATA` por conta | Sem prova de que o cliente respeite; quebraria detecção de queda, Session e isolamento |
| Seguir temporário para entrar em servidor escondido | Uma janela de CAPTCHA por conta e o outro jogador vê o follow. O "entrar seguindo" das abas Friends/Follow é outra coisa |
| Da 52: Ctrl+K, ícone com o avatar, código curto de 8 letras, voltar ao servidor privado, Discord Rich Presence | Pouco valor para quem deixa alts em AFK; o ícone do Froststrap nem é o avatar; o código curto não é do Roblox (só acha em jogo pequeno); o "voltar ao privado" do original fecha todos os clientes |

### Ainda abertas

Avaliadas em 15/10/2026 (código dos concorrentes lido, nosso código e os logs
reais do dono conferidos). Nenhuma é bug; todas dependem de um teste ou de uma
decisão do dono.

| # | Ideia | Situação |
|---|---|---|
| 11 | Formato da URL de launch / tracker do aparelho | **Medir:** um concorrente usa `RequestGameJob` com `joinAttemptId` em `www.roblox.com`; o nosso formato antigo é o que dá CAPTCHA. Testar com 2 contas se o novo reduz o CAPTCHA |
| 32 | Reserva do `ROBLOX_singletonEvent` como padrão | **Teste do dono:** o "parou em outubro" dos concorrentes é de outro método (segurar o Event), não do nosso. Os logs de 15/10 têm 4 clientes rodando bem com a reserva (um por 52 min, com teleporte). Falta 2–3 contas num jogo que teleporta |
| 34 | Fila que espera memória livre | Como aviso antes do lote + "lançar mesmo assim", pelo commit medido de cada cliente. Usa `GlobalMemoryStatusEx` (import novo: build + scan) |
| 43 | Notificação do Windows quando a conta cai | Testar antes se o plugin pesa no antivírus, ou se o `Notification` do WebView2 resolve sem plugin |
| 47 | Idade e voz da conta | A faixa etária já é lida e não aparece; a verificação de idade é instável até no concorrente |
| 30 / 51 | Multi-instância e cookies no Mac | Depende de alguém testar num Mac |
| 37, 41 | Janela de erro do Roblox; avisos de diagnóstico | Baixa prioridade; ver a janela real antes |
| — | Região em lote pelo RoValra | Host de terceiro, desligado por padrão; só se as consultas por servidor derem CAPTCHA/429 |
| — | Renovar o cookie por auth ticket | Testar com conta de teste se não derruba as sessões abertas |
| — | Servidor privado grátis por conta | Endpoint de **compra** (`expectedPrice: 0`); só se o dono quiser |
| — | Do 42 e da 52: server hop do grupo, pausar a fila, importar do evanovar, preset por atalho (`--preset`) | Talvez, sem urgência |

## Resumo — o que fazer primeiro (segunda passada, 14/10/2026)

Os oito itens do resumo de 09/10 já saíram (1.2.0 a 2.0.0). Da segunda passada,
o que mais pesa:

| # | Ideia | Prioridade |
|---|---|---|
| 31 | [Não reconectar quando o Roblox fechou a janela porque outra abriu](#31-o-roblox-fechou-a-janela-porque-outra-abriu-não-reconectar) | alta |
| 32 | [Motivo novo para ligar a reserva do `ROBLOX_singletonEvent`](#32-motivo-novo-para-ligar-a-reserva-do-roblox_singletonevent-ideia-3) | alta (teste do dono) |
| 34 | [Fila que espera memória livre no PC](#34-fila-de-launch-que-espera-memória-livre-no-pc) | alta |
| 35 | [Conferir que o cliente entrou na conta certa](#35-conferir-que-o-cliente-entrou-na-conta-certa) | alta |
| 36 | [Região do servidor pelo IP interno (provável bug)](#36-região-do-servidor-provável-bug-e-região-na-session) | bug **confirmado e corrigido** na `develop` (14/10); falta a região na Session (média) |
| 33 | [Só fechar o handle de cliente que já subiu](#33-só-fechar-o-handle-de-cliente-que-já-subiu-e-tudo-ou-nada) | média-alta |
| 40 | [Download da build com reserva e novas tentativas](#40-download-da-build-motivo-novo-para-a-ideia-29) | média |
| 38 | [Exportar contas com senha própria](#38-exportar-as-contas-com-uma-senha-própria) | média |

Os dois bugs do MultiAlt achados nesta passada (campos da conta e região do
servidor) já foram corrigidos na `develop` em 14/10/2026 ([detalhes](#bugs-do-multialt-achados-nesta-passada)).

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
- **Situação no MultiAlt:** feito em 15/10/2026 — ver [features/webhook.md](features/webhook.md).
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

## I. Segunda passada (14/10/2026)

Achadas em 35 repositórios novos, 5 forks do RAM original com commits próprios
e 6 repositórios da primeira passada que receberam commits. Cada uma foi
cruzada com a `develop` de 14/10/2026; onde diz "conferido", a lacuna foi vista
no nosso código.

### 31. "O Roblox fechou a janela porque outra abriu": não reconectar
- **Prioridade:** alta — sem isso a reconexão automática (ideia 2) pode entrar num laço em que uma conta derruba a outra.
- **Situação no MultiAlt:** variação melhor do motivo de queda. Conferido: `client_health.rs` só conhece o `waitForNewPlayerProcess … 0X102` (como ruído, num teste).
- **O que faz:** reconhece no log `[FLog::WndProcessCheck] … mutex result is 0X000001` e `WindowsAppReloadTelemetry EventWaitResultUpdate`, que é o próprio cliente saindo porque outra instância abriu. Nesse caso não reconecta e avisa que o Multi Roblox pode precisar de ajuste.
- **Onde ver:** [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `RobloxLogWatcher.cs:61-67,300` e `AppController.RobloxUpdates.cs` (`CheckClosedByRoblox`), MIT.
- **Segurança:** limpo, só leitura de log.
- **Como faríamos aqui:** nova classe de motivo no classificador do `client_health.rs`, que a reconexão trata como "parar".

### 32. Motivo novo para ligar a reserva do `ROBLOX_singletonEvent` (ideia 3)
- **Prioridade:** alta — **confirmar** com o teste do dono (2+ contas teleportando, build de outubro).
- **Situação no MultiAlt:** a ideia 3 saiu na 2.0.0 como experimental e desligada.
- **O que mudou:** três projetos independentes usam a reserva por Mutex como **método padrão**. O do Bloxkeep diz que o método antigo, pelo Event, parou de funcionar nas builds do Roblox de outubro de 2026, com 5 contas testadas no mesmo jogo. O Toolblox registra no README do helper dele que fechar o Event dentro dos clientes derruba os outros em cascata.
- **Onde ver:** [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `MultiInstance.cs:7-61`, MIT; [`jjtjtyt6644/RobloxAccountManager`](https://github.com/jjtjtyt6644/RobloxAccountManager) — `runtime/singleton.rs`, GPL-3; [`KillianM00/roblox-multi-instance`](https://github.com/KillianM00/roblox-multi-instance) — `MutexHolder.cs:16-25`, MIT (mais fraco que o nosso: falha calado com cliente aberto). Para os casos de borda (o nome já existe como Event, reserva feita pela metade): [`Paryx-games/roblox-manager`](https://github.com/Paryx-games/roblox-manager) — `ram_core/src/process.rs` (`acquire_named_objects`), MIT.
- **Segurança:** limpo. Nada aqui justifica ligar sem o teste.

### 33. Só fechar o handle de cliente que já subiu, e tudo ou nada
- **Prioridade:** média-alta.
- **Situação no MultiAlt:** variação melhor de `platform/windows/singleton.rs`. Conferido: não olha a idade do processo.
- **O que faz:** só fecha o `ROBLOX_singletonEvent` de clientes com mais de 5 s de vida, porque fechar o de um cliente que ainda está subindo faz ele se fechar. Se algum cliente não pôde ser aberto ou lido, não fecha nenhum. Avisa quando outro programa segura o nome.
- **Onde ver:** [`S4US/account-manager`](https://github.com/S4US/account-manager) — `MultiInstanceGuard.cs`, MIT.
- **Segurança:** limpo. O mesmo projeto pede admin por padrão e deixa o arquivo de settings do Roblox somente-leitura: não trazer.

### 34. Fila de launch que espera memória livre no PC
- **Prioridade:** alta — abrir um cliente que não cabe trava o PC inteiro, não só aquela conta.
- **Situação no MultiAlt:** novo. Conferido: o teto de memória é **por cliente**; nada olha a RAM livre do PC antes de abrir.
- **O que faz:** segura o próximo launch quando a RAM livre cai abaixo de max(1 GB, 10%) ou o commit (RAM + paginação) não aguenta mais um cliente (~1,8 GB por cliente, 2 GB reservados ao Windows). A reconexão espera nessa situação **sem gastar tentativa**. Antes de abrir um lote, avisa quantas contas cabem.
- **Onde ver:** [`S4US/account-manager`](https://github.com/S4US/account-manager) — `InstanceManager.cs:69-99,365-420`, MIT; [`RaresDevLibrary/RDL-AccountManager`](https://github.com/RaresDevLibrary/RDL-AccountManager) — `Manager.cs` (laço de launch, ~93-106), `ResourceMode.cs` (`ResourceUsage.MemoryLow`), `Recovery.cs` (`CanRestart`), MIT; aviso antes do lote: [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `AppController.cs:2489`.
- **Segurança:** `GlobalMemoryStatusEx` já está no `windows-sys`; não é dependência nova.
- **Como faríamos aqui:** portão na fila de `launch_shared.rs`, com o motivo na Session ("esperando memória").

### 35. Conferir que o cliente entrou na conta certa
- **Prioridade:** alta — se o cookie trocar de mãos, a fila segue achando que deu certo.
- **Situação no MultiAlt:** variação melhor da ideia 14. Conferido: o `userid` do log só é lido para os clientes abertos pelo site (`external_clients.rs`).
- **O que faz:** compara o `userid` de `[FLog::GameJoinLoadTime]` com a conta que lançamos; se não bater, ou se aparecer `challengedByGcs`, pausa a fila e avisa.
- **Onde ver:** [`evanovar/RobloxAccountManager`](https://github.com/evanovar/RobloxAccountManager) — `src/features/launch_monitor.py` (`confirm_launch`), GPL-3.0.
- **Segurança:** limpo.

### 36. Região do servidor: provável bug, e região na Session
- **Prioridade:** o bug está **corrigido** na `develop` (14/10/2026); média para a parte nova.
- **Situação no MultiAlt:** confirmado nos logs reais do dono em 14/10/2026 — **todos** os joins iam para um `10.x` (`RCC Server Address`), com o público na linha `UDMUX Address`. O `server_machine_address` geolocalizava o `MachineAddress` cru, e a coluna Region mostrava o IP interno. Hoje ele prefere `joinScript.UdmuxEndpoints[].Address` (o mesmo campo que `ex9d/sentra` e `woogi999/pious-bootstrapper` leem) e IP privado/reservado nunca vai para a geolocalização (`server_region_http_tests`, `server_region_payload_tests`). A região na Session continua aberta.
- **O que faz:** descarta endereço privado e usa o IP público do UDMUX. A parte nova: a Session mostra a região do servidor de cada conta, lida da linha `UDMUX Address` do log, sem nenhuma chamada `join-game-instance`. Junto, dá para mostrar há quanto tempo o servidor está no ar, pela linha `Server Prefix …_<data>_RCC`.
- **Onde ver:** [`bloxstraplabs/bloxstrap`](https://github.com/bloxstraplabs/bloxstrap) — `ActivityData.cs:43`, `ActivityWatcher.cs:244-260`, MIT; [`AeroUp/Visor`](https://github.com/AeroUp/Visor) — `logwatcher.js`, MIT; uptime: [`Froststrap/Froststrap`](https://github.com/Froststrap/Froststrap) — `ActivityWatcher.cs:23,570-592`, MPL-2.0.
- **Segurança:** geolocalizar pelo serviço que já usamos (não o `ipinfo.io` do Visor). Nunca geolocalizar IP privado.

### 37. Diálogo de erro do Roblox na partida = launch que falhou
- **Prioridade:** média.
- **Situação no MultiAlt:** novo; não olhamos a classe da janela do PID.
- **O que faz:** se a janela do cliente é um diálogo (`#32770`), a conta fica marcada com o título do erro. Só conta como "aberto" a janela `WINDOWSCLIENT` que responde.
- **Onde ver:** [`SadinKai/SUNDAY`](https://github.com/SadinKai/SUNDAY) — `src/main/legacy-roblox-isolation-adapter.js:1290-1307`, `src/main/native.js:270-313`, MIT.
- **Segurança:** limpo. O mesmo projeto fecha handles dentro do cliente: não trazer.

### 38. Exportar as contas com uma senha própria
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor dos backups. Conferido: sem Pass Lock o backup leva o `AccountData.key` dentro do zip — quem tem o zip tem as contas.
- **O que faz:** exporta um arquivo cifrado com uma senha escolhida na hora, que abre em qualquer PC. Com senha, dá para oferecer backup automático para uma pasta (a do OneDrive/Drive do usuário), com retenção.
- **Onde ver:** [`Gugas19/Roblox-Account-Manager`](https://github.com/Gugas19/Roblox-Account-Manager) — `src/commands/backup.rs` (`export_accounts`), GPL-3.0 (fork do RAM que virou Tauri/Rust); [`TheFadGhost/roblox-account-manager-pro-public`](https://github.com/TheFadGhost/roblox-account-manager-pro-public) — `Services/CryptoService.cs` (`EncryptPortableJson`), MIT; backup em pasta: [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `Backup.cs`.
- **Segurança:** com o argon2 + `crypto_secretbox` que já estão no binário (sem dependência nova; o sodiumoxide saiu por causa do antivírus). Backup em pasta de nuvem **só** com senha.

### 39. Versão mais velha do app nunca grava por cima dos dados
- **Prioridade:** média — protege quem volta de versão (MSI antigo, rollback).
- **Situação no MultiAlt:** variação melhor do latch `load_failed`.
- **O que faz:** cada arquivo de dados guarda a versão do app que o salvou; uma versão mais velha lê, mas não grava.
- **Onde ver:** [`woogi999/pious-bootstrapper`](https://github.com/woogi999/pious-bootstrapper) — `core/store.rs` (`newer_than_us`), sem licença.
- **Segurança:** limpo. O mesmo projeto passou a gravar `production` no canal a cada launch — descartado (regra crítica).

### 40. Download da build: motivo novo para a ideia 29
- **Prioridade:** sobe de baixa para **média**.
- **Situação no MultiAlt:** a ideia 29 parecia só para quem usa versão customizada. Mas hoje uma falha de rede no download da build leva o `launch_url` ao último recurso do handler `roblox-player:` (regra crítica), e o app usa um host e um CDN só, sem nova tentativa (`launch.rs`, `versions.rs`).
- **O que faz:** reserva `clientsettings.roblox.com` quando o `clientsettingscdn` falha; escolhe o espelho por `/versionStudio`; até 5 tentativas por pacote; checa espaço em disco; cache por MD5.
- **Onde ver:** [`bloxstraplabs/bloxstrap`](https://github.com/bloxstraplabs/bloxstrap) — `Deployment.cs:28-182`, `Bootstrapper.cs:1455-1590`, MIT.
- **Segurança:** o Bloxstrap trata 403 como canal aposentado e regrava o canal — **não** trazer essa parte. Conferir o hash de cada pacote.

### 41. Diagnóstico: bootstrapper no handler, processo na bandeja, instalador
- **Prioridade:** média (resto da ideia 16).
- **O que faz:** avisa quando Bloxstrap/Froststrap é o handler do protocolo (eles fecham todos os clientes quando atualizam: `Bootstrapper.cs:957-976,1030`); avisa quando há processo do Roblox na bandeja (`--launch-to-tray`), que pode explicar "o launch não faz nada"; avisa quando o instalador do Roblox assumiu o launch e o jogo vai voltar em outra conta.
- **Onde ver:** [`bloxstraplabs/bloxstrap`](https://github.com/bloxstraplabs/bloxstrap) (acima); bandeja: [`VladDerK1ng/RobloxKeeper`](https://github.com/VladDerK1ng/RobloxKeeper) — `GhostWatch.cs`; instalador: [`woogi999/pious-bootstrapper`](https://github.com/woogi999/pious-bootstrapper) — `service/launch.rs` (`watch_for_self_update`).
- **Segurança:** **só avisar**. Nunca matar processo nem reescrever o handler.

### 42. Presets mais vivos: grupo, server hop e reinício programado
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor dos presets (ideia 13).
- **O que faz:**
  - preset que aponta para um **grupo** de contas, resolvido de novo a cada launch;
  - **server hop** do grupo: muda as contas juntas para outro servidor público com vaga para todas, fechando só os clientes que o app rastreia;
  - reiniciar cada cliente a cada N horas, um por vez, voltando ao mesmo servidor.
- **Onde ver:** grupo: [`Paryx-games/roblox-manager`](https://github.com/Paryx-games/roblox-manager) — `ram_ui/src-tauri/src/main.rs` (`launch_launch_preset`), `ram_core/src/account_tags.rs` (`matching_accounts`), MIT. Server hop: [`JeskoMts/Instance-Manager`](https://github.com/JeskoMts/Instance-Manager) — `ViewModels/ShellViewModel.cs` (`ServerHopAsync`, `PickServer`), Apache-2.0. Reinício: [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `AppController.Presets.cs` (`RestartDueClients`).
- **Segurança:** o reinício do Bloxkeep usa `GetLastInputInfo`: essa parte não entra.

### 43. Notificação do Windows quando a conta cai
- **Prioridade:** média — o aviso que o webhook (ideia 5) daria, sem mandar nada para fora do PC.
- **Situação no MultiAlt:** novo.
- **Onde ver:** [`BakedAleska/Toolblox`](https://github.com/BakedAleska/Toolblox) — `src-tauri/src/auto_rejoin.rs:108` (`tauri_plugin_notification`), MIT.
- **Segurança:** o plugin entra no binário — build + scan da edição padrão antes de adotar.

### 44. AFK que devolve a janela exatamente como estava
- **Prioridade:** média.
- **Situação no MultiAlt:** variação melhor de `afk.rs`.
- **O que faz:** depois do toque, re-minimiza sem ativar outra janela (nós usamos `SW_MINIMIZE`), devolve a janela coberta à mesma profundidade (a nossa fica por cima), desliga a animação de minimizar/restaurar durante o toque e devolve o mouse quando o shift-lock o puxou para o centro. Também confirma a ideia 24 (par seta → / ←); as setas estendidas já existem no `input.rs`, só faltam na lista do AFK.
- **Onde ver:** [`bloxnest/nudge-nest`](https://github.com/bloxnest/nudge-nest) — `src/Nudger.cs`, `NudgeCheck.cs`, MIT.
- **Segurança:** o "quando tocar" deles usa `GetLastInputInfo`/`AttachThreadInput`, proibidos no nosso AFK.

### 45. Janelas: posição que sobrevive à troca de monitor, miniaturas ao vivo
- **Prioridade:** média.
- **O que faz:** `SaveWindowPositions` grava a posição em **fração do monitor**, com um ID estável do monitor (hoje grava pixels). E uma grade de miniaturas ao vivo das janelas dentro do app, pelo DWM.
- **Onde ver:** [`Makmatoe/SessionDock`](https://github.com/Makmatoe/SessionDock) — `RobloxWindowLayoutPolicy.cs:42,318`, MIT; miniaturas: [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `WindowThumbnails.cs`, [`KillianM00/roblox-multi-instance`](https://github.com/KillianM00/roblox-multi-instance) — `Controls/DwmThumbnailHost.cs`.
- **Segurança:** miniatura DWM é API nativa nova — build + scan.

### 46. Esconder os nomes também no título da janela do Roblox
- **Prioridade:** média — com os nomes escondidos no app, o título "conta — Roblox" (ideia 19) ainda pode mostrar a conta numa live ou num print.
- **Onde ver:** [`minhhoang0122/bloxkeep`](https://github.com/minhhoang0122/bloxkeep) — `AppController.Names.cs`.
- **Segurança:** limpo.

### 47. Voz, faixa etária e verificação de idade por conta
- **Prioridade:** média.
- **Situação no MultiAlt:** novo.
- **O que faz:** mostra na conta se tem chat de voz, a faixa etária e se a idade é verificada, com filtro na lista.
- **Onde ver:** [`joe-jns/roblox-account-manager`](https://github.com/joe-jns/roblox-account-manager) — `src/main.js:645-668` (`voice.roblox.com/v1/settings`, `accountinformation.roblox.com/v1/birthdate`, `apis.roblox.com/age-verification-service/v1/age-verification/verified-age`), sem licença.
- **Segurança:** leitura — `read_without_refresh`, host por `endpoints::host`. O mesmo projeto guarda senha em texto puro: não trazer.

### 48. Esperar um jogador ficar disponível e entrar
- **Prioridade:** média.
- **O que faz:** em Follow, se o jogador não está num servidor em que dá para entrar, fica vigiando com espera crescente (respeitando `Retry-After`) e entra quando der.
- **Onde ver:** [`Makmatoe/SessionDock`](https://github.com/Makmatoe/SessionDock) — `JoinUserWatchPolicy.cs`, MIT.
- **Segurança:** leitura de presença sem refresh; limite de tempo.

### 49. Identidade do aparelho estável por conta
- **Prioridade:** média — **investigar**, junto com a aberta "`LOCALAPPDATA` próprio por conta".
- **Situação no MultiAlt:** variação do isolamento: hoje ele apaga, mas não toca `appStorage.json` nem `rbx-storage.id`.
- **O que faz:** guarda por conta e troca, **só com nenhum cliente aberto**, `appStorage.json`, `RobloxCookies.dat` e `rbx-storage.id`; conta nova recebe cópia limpa com `AppInstallationId`/`BrowserTrackerId` novos. Tem relação com a ideia 11 (tracker) e com o lock de `appStorage.json` do evanovar (`account_actions.py`, `_lock_roblox_storage_file`).
- **Onde ver:** [`TrapstarKS/TrapRAM`](https://github.com/TrapstarKS/TrapRAM) — `src/main/profiles.ts`, `src/shared/pure.ts:95-108`, sem licença.
- **Segurança:** não atrapalhar quem joga pelo site; respeitar "isolamento pula com cliente aberto".

### 50. Servidores privados da conta na aba Servers
- **Prioridade:** baixa-média.
- **Situação no MultiAlt:** o backend já busca os servidores privados (`avatar_games.rs`), mas a aba Servers só pede os públicos.
- **Onde ver:** [`skipnz/Roblox-Account-Manager`](https://github.com/skipnz/Roblox-Account-Manager) — `Forms/ServerList.cs:260-292`, GPL-3.0 (fork do RAM).

### 51. Mac: cookies por identificador do app
- **Prioridade:** alta se o Mac for prioridade — **confirmar num Mac**.
- **Situação no MultiAlt:** possível bug. O macOS guarda cookies pelo identificador do app, não pela pasta. Clientes com o mesmo identificador dividem o arquivo de cookie, e no rejoin uma conta entra como a outra. O nosso `platform/macos/launch.rs` abre todos com o mesmo identificador.
- **Atualiza a ideia 30:** pelo teste deles, o caminho sem reassinar (HOME separado, abrindo o cliente direto) **não** entrega o link do jogo, o que contradiz o intraducine. A correção deles reassina o app, que a ideia 30 proíbe. Decisão do dono depois de um teste. Também deles: atualizar o Roblox antes do lote, porque cópias velhas brigam pelo instalador (`RobloxVersionGate.swift`, `RobloxUpdateDriver.swift`) — sem o passo que fecha o cliente principal.
- **Onde ver:** [`estevanhernandez-stack-ed/rororo-mac`](https://github.com/estevanhernandez-stack-ed/rororo-mac) — `docs/decisions/0009-per-instance-cookie-isolation.md`, `BundleIDRewriter.swift`, MIT.
- **Segurança:** o rororo grava no log do sistema o link de launch com o ticket (`MultiInstanceCoordinator.swift:507`) e o põe no zip de diagnóstico — não reproduzir.

### 52. Pequenas (baixa prioridade)
- Lançar conta ou preset pela **bandeja** (hoje só Show/Quit, `lib.rs:470`) — [`adxm-o/irradiant-account-manager`](https://github.com/adxm-o/irradiant-account-manager).
- **Pausar/retomar** a fila de launch — [`TheFadGhost/roblox-account-manager-pro-public`](https://github.com/TheFadGhost/roblox-account-manager-pro-public) `Services/PauseController.cs`.
- Paleta de comandos **Ctrl+K** — [`breud/poob-account-manager`](https://github.com/breud/poob-account-manager) `src/components/ui/CommandPalette.tsx`.
- **Avatar da conta como ícone da janela** (`WM_SETICON`) — [`Froststrap/Froststrap`](https://github.com/Froststrap/Froststrap) `WindowManipulation.cs:75-155`; **cor da barra de título** por grupo (Windows 11, `DwmSetWindowAttribute` 35/36) — [`thanknoah/Moon-Instance-Controller`](https://github.com/thanknoah/Moon-Instance-Controller).
- Tocar uma gravação depois de **qualquer** launch, não só da reconexão — [`itsnexoria/rexio`](https://github.com/itsnexoria/rexio) `src/main.js` (`waitForInGameThenRun`).
- **Importar contas do evanovar** (`saved_accounts.json`) — [`slashznX/roblox-account-manager`](https://github.com/slashznX/roblox-account-manager) `src-tauri/src/importer.rs` (`scan_evanovar`); conferir o formato no próprio evanovar.
- **Atalho na área de trabalho** que dispara um preset — [`cursed-blip/catstrap`](https://github.com/cursed-blip/catstrap) `Integrations/Quickplay.cs` (código nativo novo: scan).
- Colar o **código curto** de 8 caracteres do servidor — [`HanazonoArchive/RBX-Server-Manager`](https://github.com/HanazonoArchive/RBX-Server-Manager). (O "scan de baixo ping" dele é só o campo `ping` da API, a média dos jogadores.)
- Desfazer remoção de contas, renomear grupo, linha de comando — [`Gugas19/Roblox-Account-Manager`](https://github.com/Gugas19/Roblox-Account-Manager).
- Voltar ao servidor privado quando o jogo leva a conta para um público — [`Galkurta/Auto-Rejoin`](https://github.com/Galkurta/Auto-Rejoin) (lá por presença; aqui seria pelo log).
- Discord Rich Presence — os bootstrappers têm, mas com várias contas mostraria uma só; daria pelo pipe `discord-ipc-N`, sem dependência nativa.

### Bugs do MultiAlt achados nesta passada
- **Campos da conta perdem o que foi digitado** — conferido e **corrigido** na `develop` em 14/10/2026 (salva ao sair da caixa e ao fechar, só se mudou; teste em `AccountFieldsDialog.test.tsx`). Era assim: `AccountFieldsDialog.tsx` só salva com Enter na caixa do valor ou ao remover um campo; trocar só a chave, criar campo com "+" ou fechar pelo X, pelo Esc ou pelo fundo descarta tudo sem aviso. O fork [`iamstrix`](https://github.com/iamstrix/iamstrix-Roblox-Account-Manager) corrigiu o mesmo defeito no RAM original (`Classes/Field.cs`, `Forms/AccountFields.cs`: salva ao sair da caixa e ao fechar).
- **Região do servidor pelo IP interno** — confirmado pelos logs reais e corrigido em 14/10/2026: ver ideia 36.

## Para decisão do dono

Ideias boas que esbarram numa regra do projeto ou trazem um host novo. Não
recomendadas por padrão; viram opção só se ele quiser.

- **Adiar o AFK enquanto o usuário mexe no PC** (Agzes) e **forçar o foco** (RoPilot): dependem de `GetLastInputInfo`/`AttachThreadInput`, proibidos de propósito no `afk.rs:2004-2029`.
- **Região dos servidores em lote pelo RoValra** ([`sleaze5`](https://github.com/sleaze5/RobloxAccountManager) `internal/integration/rovalra/client.go`): mais rápido que o `join-game-instance` por servidor, mas é host de terceiro — só desligado por padrão.
- **Seguir quem escondeu o servidor** com follow temporário e unfollow garantido por diário ([`Paryx-games`](https://github.com/Paryx-games/roblox-manager) `ram_core/src/join_user.rs`, `temporary_join`): mexe no perfil social da conta.
- **Servidor privado grátis por conta** ([`xspww/Khabarovsk`](https://github.com/xspww/Khabarovsk) `domain/roblox_private_servers.py`, `ensure_owned_private_server`, GPL-3): reusa o servidor privado que a conta tem e, se não houver e o jogo permitir, cria um a preço 0. É um endpoint de **compra** — sempre `expectedPrice: 0`, confirmar com conta de teste, e só com o dono querendo.
- **Renovar o cookie trocando um auth ticket** (`authentication-ticket/redeem`) em vez do `signoutfromallsessions` ([`TrapstarKS/TrapRAM`](https://github.com/TrapstarKS/TrapRAM) `src/main/roblox.ts:106-128`): mesma ressalva do `v2/session/refresh` da ideia 7 — só depois de provar com conta de teste que não derruba as outras sessões.
- **`LOCALAPPDATA` próprio por conta** ([`Overdusts`](https://github.com/Overdusts/roblox-multi-instance-launcher) `Core/RobloxLauncher.cs`, `SetupInstanceIsolation`): separaria settings/cookie local/logs de contas que rodam juntas (o isolamento atual não separa), mas muda onde o cliente procura as builds — investigar antes.

## Repositórios analisados (09/10 e 14/10/2026)

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
| **Segunda passada (14/10/2026)** | | | | |
| [bloxstraplabs/bloxstrap](https://github.com/bloxstraplabs/bloxstrap) | 3174 | MIT | L (telemetria ligada; mata todos os clientes no update; regrava o canal) | 36, 40, 41 |
| [Froststrap/Froststrap](https://github.com/Froststrap/Froststrap) | 296 | MPL-2.0 | L (gancho global de teclado; rejoin que mata todos) | 36, 52 |
| [Campionnn/Roblox-Account-Manager](https://github.com/Campionnn/Roblox-Account-Manager) (fork) | 9 | GPL-3.0 | L (Web API ligada sem senha por padrão) | nada |
| [cursed-blip/catstrap](https://github.com/cursed-blip/catstrap) | 8 | MIT | L, **arriscado** (asset proxy: edita `hosts`, CA própria no `cacert.pem` do cliente) | 52 |
| [Galkurta/Auto-Rejoin](https://github.com/Galkurta/Auto-Rejoin) | 4 | — | L descuidado | 52 |
| [Makmatoe/SessionDock](https://github.com/Makmatoe/SessionDock) | 2 | MIT | L (o mais cuidadoso do lote) | 45, 48 |
| [S4US/account-manager](https://github.com/S4US/account-manager) | 1 | MIT | L (pede admin; settings do Roblox somente-leitura) | 33, 34 |
| [jjtjtyt6644/RobloxAccountManager](https://github.com/jjtjtyt6644/RobloxAccountManager) | 1 | GPL-3 | L (atualiza o Roblox pelo instalador) | 32 |
| [thanknoah/Moon-Instance-Controller](https://github.com/thanknoah/Moon-Instance-Controller) | 1 | — | **S** (código incompleto, lag switch por `netsh`) | 52 |
| [Gugas19/Roblox-Account-Manager](https://github.com/Gugas19/Roblox-Account-Manager) (fork) | 1 | GPL-3.0 | L (virou Tauri/Rust; launch pelo handler cru) | 38, 52 |
| [niooii/Roblox-Account-Manager](https://github.com/niooii/Roblox-Account-Manager) (fork) | 1 | GPL-3.0 | L | nada |
| [iamstrix/iamstrix-Roblox-Account-Manager](https://github.com/iamstrix/iamstrix-Roblox-Account-Manager) (fork) | 1 | GPL-3.0 | L (`nuget.exe` commitado) | bug dos campos |
| [skipnz/Roblox-Account-Manager](https://github.com/skipnz/Roblox-Account-Manager) (fork) | 0 | GPL-3.0 | L | 50 |
| [minhhoang0122/bloxkeep](https://github.com/minhhoang0122/bloxkeep) | 0 | MIT | L (o mais parecido com o MultiAlt) | 31, 32, 34, 38, 42, 45, 46 |
| [RaresDevLibrary/RDL-AccountManager](https://github.com/RaresDevLibrary/RDL-AccountManager) | 0 | MIT | L, limpo | 34 |
| [TheFadGhost/roblox-account-manager-pro-public](https://github.com/TheFadGhost/roblox-account-manager-pro-public) | 0 | MIT | L | 38, 52 |
| [SadinKai/SUNDAY](https://github.com/SadinKai/SUNDAY) | 0 | MIT | L (fecha handles dentro do cliente) | 37 |
| [TrapstarKS/TrapRAM](https://github.com/TrapstarKS/TrapRAM) | 0 | — | L | 49 |
| [BakedAleska/Toolblox](https://github.com/BakedAleska/Toolblox) | 0 | MIT | L (helper `.exe` com fonte; widgets com acesso total) | 32, 43 |
| [joe-jns/roblox-account-manager](https://github.com/joe-jns/roblox-account-manager) | 0 | — | L (senhas em texto puro) | 47 |
| [adxm-o/irradiant-account-manager](https://github.com/adxm-o/irradiant-account-manager) | 0 | MIT/UNLICENSED | L ("Close blockers" mata todos) | 52 |
| [estevanhernandez-stack-ed/rororo-mac](https://github.com/estevanhernandez-stack-ed/rororo-mac) | 0 | MIT | L (ticket no log do sistema; reassina o app) | 51 |
| [bloxnest/nudge-nest](https://github.com/bloxnest/nudge-nest) | 0 | MIT | L, sem rede | 44 |
| [AeroUp/Visor](https://github.com/AeroUp/Visor) | 0 | MIT | L (overlay é janela por cima, não injeta) | 36 |
| [KillianM00/roblox-multi-instance](https://github.com/KillianM00/roblox-multi-instance) | 0 | MIT | L | 32, 45 |
| [breud/poob-account-manager](https://github.com/breud/poob-account-manager) | 0 | MIT | L, arriscado (`WriteProcessMemory` para FPS; cookies em texto puro) | 52 |
| [itsnexoria/rexio](https://github.com/itsnexoria/rexio) | 0 | — | L | 52 |
| [slashznX/roblox-account-manager](https://github.com/slashznX/roblox-account-manager) | 0 | — | L, invasivo (admin, lê cookies do navegador, "matar todos") | 52 |
| [geo-daik/Onyxstrap](https://github.com/geo-daik/Onyxstrap) | 0 | MIT | L (telemetria desligada por padrão) | só baixas |
| [xspww/Khabarovsk](https://github.com/xspww/Khabarovsk) | 0 | GPL-3 | L, feito para farm com executor (fecha todos; pagefile com admin) | decisão do dono |
| [Rickagon/Roblox-Account-Manager](https://github.com/Rickagon/Roblox-Account-Manager) | 0 | MIT | L, práticas ruins (tarefa agendada, mata clientes sem janela, navegador "stealth") | nada |
| [Nxxo31/NexoAccManager](https://github.com/Nxxo31/NexoAccManager) | 0 | MIT | L, fraco (captcha pelo NopeCHA; README anuncia o que não existe) | nada |
| [bhhoang/roblox-account-manager-rs](https://github.com/bhhoang/roblox-account-manager-rs) | 0 | GPL-3.0 | L, imaturo | nada |
| [iamvferdz/roblox-account-manager](https://github.com/iamvferdz/roblox-account-manager) | 0 | GPL-3.0 | cópia idêntica do ic3w0lf22 | nada |
| [tatugava/roblox-account-reconnect-scheduler](https://github.com/tatugava/roblox-account-reconnect-scheduler) | 0 | Unlicense | L (firewall + UAC + tarefa agendada) | nada |
| [HanazonoArchive/RBX-Server-Manager](https://github.com/HanazonoArchive/RBX-Server-Manager) | 0 | — | **S** pela intenção (servidor para dupe; proxies CORS e Supabase do autor) | 52 |
| [Kntt9/robloxmanager](https://github.com/Kntt9/robloxmanager) | 0 | MIT | **S** (cópia rebatizada, autor anônimo, `.exe` de 17 MB commitado, lê memória do cliente) | nada |
| [Varatos99/VHRobloxManager](https://github.com/Varatos99/VHRobloxManager) | 0 | — | **S** (estrutura de checker: cookies em massa por proxy) | nada |
| [TheFadGhost/ghost-tracker-community](https://github.com/TheFadGhost/ghost-tracker-community) | 0 | — | **S** (SaaS de farm com executor, `loadstring` remoto) | nada |
| [imtusxd/roblox-auto-rejoin](https://github.com/imtusxd/roblox-auto-rejoin) | 0 | — | serve a executor + bot de trade; `handle.exe` commitado | nada |
| **Atualizados em 14/10** | | | | |
| JeskoMts/Instance-Manager, Paryx-games/roblox-manager, woogi999/pious-bootstrapper, evanovar/RobloxAccountManager, sleaze5/RobloxAccountManager, tsukiforge/BoneFish | — | Jesko agora Apache-2.0 sem conflito | mesmo veredito; Paryx parou de fechar handles no cliente, evanovar tirou o "Kill all", pious passou a gravar `production` no canal | 32, 35, 39, 41, 42 |

## Bandeira vermelha (não clonados)

Iscas do nicho. Não abrir, não baixar, não indicar.

- **Estrelas falsas em lote**, todos `HTML`, 68–70 ★, atualizados no mesmo dia, nomes "...-Forge/-Orbit/2026": `mehrab-6767/SessionX-Orbit`, `kshong/roblox-multi-account-orbit`, `MendAura/roblox-session-forge`, `MAHDI292-sys/rustblox-account-forge`, `ItaloFelixSantos/Roblox-Account-Manager-Python`, `AFKvipproplayer/Roblox-MultiLauncher-Next`, `rajnathyadav6/Fisch-DevOps-Forge`, `donguyenbao/place-sync-hub`, `Jimeshhacker/MultiRoblox-Orchestrator`, `HeisJeremiah1/NeuzBlox-MultiRunner`, `Ferrerodillo/Nexo-Account-Vault`, `0m4rxze/Roblox-Account-Switcher`.
- **Sem código e com descrição de marketing** ("ultimate", "premium", "pro-tier"): `kich2/Multiroblox-extension-2026` (88 ★), `lifet-dot/Roblox-Extension-Multi-Account-Manager` (76 ★), `toskaman/RBX-Manager-2`, `rapha30/RobloxAccountManagerPlus-*` ("Download …"), e dezenas de repositórios de 0–1 ★ com linguagem vazia.
- **Abuso explícito:** `katicmartin854-eng/Multi-Toll-To-Hack-Roblox-Accounts`, `zZarby/RChecker` (checker de combolist), `wanz-hub/WANZZ` (bypass de key system).
- **14/10/2026 — mais estrelas falsas em lote** (`HTML`, 68–70 ★): `rayo0312/Roblox-Assist-Suite`, `kliklim/Voidstrap-Themes-And-Features`, `khadirkumar/vision-roblox-sentinel`, `jlal892/roblox-multiplex-console`, `farhan797648/Roblox-FlagForge-Engine`, `Issotos/Saweria-Roblox-Bridge`, `storehummer12-dotcom/kitstrap-core`, `deku2007530/Faction-Sniper`, `Syahmellow51/Neuz-Strap-Lite`, `SaniFM195/Fishstrap-Roblox-Config-Hub`, `Halimos51/Init-Sequence-Orchestrator`, `Ahsanmughal-byte/Fishstrap-Roblox-Launcher-Flags-Editor`, `Agum52/Goldstrap-Bootstrapper`.
- **14/10/2026 — nome de produto conhecido com dono aleatório e descrição de SEO** ("Bloxstrap v2.11.4 is the bootstrapper: bloxstrap roblox, bloxstrap fast flags…"): `bloxstrap-project/Bloxstrap`, `Protaminechangeover267/Fishstrap-Roblox`, `Haemoglobinopathyredfox8677/Fishstrap-Roblox`, `historical-soave754/robloxaccountmanager-desktop-app`, `LIxyLGy75LhmG/RobloxAutoClickerOpen`, `DeltaExec-Roblox/DeltaExecRoblox` (executor). O Bloxstrap de verdade é `bloxstraplabs/bloxstrap`.
- **14/10/2026 — abuso explícito:** `permana83/Roblox-Account-Stealing-Tool`, `Aspectise/Roblox-Mass-Tools`, `LarperOfTheWorld/QuantumStrap` (aimbot/ESP embutidos), cheats "Download … Cheat" (`iNovovonSodelx/*`, `iXebamedVasor/*`), geradores de conta e auto-signup.
- **Clonado e confirmado como isca:** `Tunneliahover/roblox-account-manager-launcher` (ver tabela).

## Descartado em todos os projetos (padrões que não entram)

- Matar todos os clientes (ao abrir, ao fechar, no "kill switch", no "Repair").
- Ler/escrever memória do cliente, fechar handles **dentro** do processo do Roblox para outros fins, modificar CoreScripts, FFlags que mexem em física/anúncios/vantagem.
- `handle64.exe`/`handle.exe` baixado e executado; atualizador que roda exe sem conferir assinatura/hash.
- Gravar canal ou reescrever o handler `roblox-player` no registro; deixar arquivo de settings do Roblox somente-leitura.
- Importar cookies do navegador; cookie em texto puro, base64 ou XOR "criptografado"; cookie em log/console; API local sem senha.
- Macros, ganchos globais de teclado/mouse, trade bot, sniper de username, scraper de grupos, bot de visitas.
- Sessão RDP por alt (exige admin + RDP Wrapper, que antivírus marcam).
- **14/10/2026:** proxy de assets que edita o `hosts` e instala uma CA própria no `cacert.pem` do cliente (catstrap); resolver captcha por serviço pago de terceiro (NopeCHA); escrever na memória do cliente para mudar o FPS (poob); "validar senha" pelo endpoint de troca de senha (escrita para responder uma leitura); importar cookies em massa por proxy (padrão de checker); vigia que mata qualquer cliente sem janela, inclusive os de fora.
