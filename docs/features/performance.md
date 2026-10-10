# Desempenho enquanto você joga

## Objetivo

Deixar rápido o cliente que o usuário está jogando e aliviar os outros, sem
fechar nada. Vem das ideias 18, 20 e 22 de
[ideias-de-outros-gerenciadores.md](../ideias-de-outros-gerenciadores.md)
(pacote **Desempenho**, ver [plano-ideias.md](../plano-ideias.md)). Tudo
opcional e desligado por padrão.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Decisão (quem é o cliente em uso, carência, o que mudar) e o laço | [platform/windows/focus_follow.rs](../../src-tauri/src/platform/windows/focus_follow.rs) |
| Troca de prioridade/EcoQoS/memória ao vivo, teto do Job | [platform/windows/optimization.rs](../../src-tauri/src/platform/windows/optimization.rs) (`apply_process_policy_live`, `set_job_cpu_cap`) |
| Timer de 1 s e devolução ao fechar o app | [commands/focus_follow.rs](../../src-tauri/src/commands/focus_follow.rs), [lib.rs](../../src-tauri/src/lib.rs) |
| Launch avisa o perfil do cliente | [commands/launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) (`apply_windows_post_launch_profile`) |
| Tela | Settings > Optimization, cartão **While you play** ([OptimizationTab.tsx](../../src/components/settings/OptimizationTab.tsx)) |

## Otimização que segue o foco (`Optimization.FollowFocus`)

Interruptor **Follow the window in use**: o cliente que você está jogando roda
a toda velocidade, os outros desaceleram.

### Fluxo

1. Uma vez por segundo o app pergunta ao Windows qual janela está em primeiro
   plano (`GetForegroundWindow` + `GetWindowThreadProcessId`).
2. Se ela é de um cliente **que o app abriu**, ele vira o "cliente em uso".
   Janela de fora (Discord, o próprio MultiAlt, um cliente aberto pelo site)
   **não** troca o cliente em uso: o último continua a toda velocidade.
3. Cliente em uso e cliente na carência → **toda velocidade**: prioridade
   normal, power throttling devolvido ao Windows (`ControlMask = 0`), memória
   normal e, se o perfil tem teto de CPU, o teto do Job **desligado**.
4. Os outros → **fundo**: a política do perfil com que a conta abriu se o
   usuário ligou `EnableProcessPolicy`; senão uma leve (abaixo do normal, EcoQoS,
   timer ignorado, memória baixa). O teto de CPU do Job volta, se o perfil tem.
5. Só há chamada ao processo quando a velocidade desejada **muda**; a olhada
   de cada segundo não abre processo nenhum.

### Regras de negócio

- **Só clientes que o app abriu** (rastreados e não adotados). O cliente
  aberto pelo site só é exibido: nunca recebe prioridade, teto nem nada.
- **Carência de 35 s** para cliente novo (carregar o jogo é a parte pesada).
  Os que já estavam abertos quando a opção ligou não têm carência.
- Antes de mexer num PID o app confere que ele ainda é um Roblox (o Windows
  reaproveita PIDs).
- Com a opção ligada, o launch **não aplica** a prioridade do perfil (o laço
  cuida dela); o Job (teto de CPU e de memória) continua sendo criado no
  launch, e o laço reaplica a velocidade logo depois (`focus_follow_forget_applied`).
- **Desligar a opção ou fechar o app** devolve cada cliente ao que o launch
  deu: a política do perfil se ligada, senão o normal; o teto do Job volta se o
  perfil o tiver. Fechar o app não fecha cliente nenhum.
- `BackgroundMode` do perfil ("Idle até no cliente em foco") vale só para os
  clientes de fundo enquanto a opção está ligada.

### Por que pergunta em vez de evento

A ideia original reage à troca de foco por `SetWinEventHook`. Aqui não:

- `SetWinEventHook` seria uma API nova no binário (risco de antivírus), e o
  `afk_input_safety_tests` reprova o AFK mode citar um módulo que leia
  entrada — pôr o evento em `platform/windows/` faria do módulo `windows`
  inteiro um "leitor".
- `GetForegroundWindow` e `GetWindowThreadProcessId` já estavam no binário. A
  pergunta custa microssegundos; a troca de janela aparece em até 1 s.

### Fora daqui (de propósito)

- **Teto de memória que pagina em vez de matar** (RobloxKeeper:
  `EmptyWorkingSet`/`SetProcessWorkingSetSizeEx`): ficou de fora porque nenhuma
  das duas APIs está no binário hoje.
- Nada de afinidade de CPU, turbo, plano de energia, tarefa agendada ou admin.

## Configurações relacionadas

| Seção | Chave | Default | Efeito |
|---|---|---|---|
| Optimization | `FollowFocus` | `false` | Liga a otimização que segue o foco. |
| Optimization | `{Normal,BottingPlayer,BottingBot}EnableProcessPolicy` e demais | ver [settings.md](settings.md#optimization) | Política de fundo (se ligada) e o estado devolvido ao desligar. |

## Testes

`focus_follow_tests` (cliente em uso, carência, plano de mudanças, políticas,
devolução, padrão desligado), `win_optimization_tests` (máscaras do power
throttling e do teto do Job ao vivo), `settingsTabs.test.tsx` (interruptor).
Suíte: `bun run t performance`.

## Armadilhas / cuidados

- O efeito só se vê com cliente real: Gerenciador de Tarefas > Detalhes >
  coluna Prioridade (Normal no jogo em uso, Abaixo do normal nos outros) e a
  coluna "Modo de eficiência".
- O rastreamento é o do app: cliente que o app abriu e que o Auto Rejoin
  relançou entra de novo com carência nova.
