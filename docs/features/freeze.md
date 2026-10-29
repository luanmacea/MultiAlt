# Freeze (congelar o cliente)

Pedido do dono em 29/10/2026, a partir da issue #40 ("Freezing Client for X
seconds"). Congela o cliente do Roblox das contas marcadas na página **Sessão**:
o jogo para de desenhar e de gastar CPU/GPU, a conta **continua conectada**, e
ao soltar ele segue de onde estava. Serve para aliviar o PC com várias contas
abertas e para quem usa o congelamento em jogo (a issue cita "bug interactions").

## Onde fica

- Cabeçalho "In game" da página Sessão, ao lado do **AFK Mode**: botão
  **Freeze (N)** — vale para as contas marcadas, ou todas se nenhuma estiver
  marcada (a mesma regra do AFK Mode). Abre um menu com o tempo: *Until I
  unfreeze*, 10 s, 30 s, 1 min, 5 min. Com todas as contas-alvo já congeladas
  ele vira **Unfreeze (N)**.
- Linha da conta congelada: selo **Frozen · 12s** (ou só *Frozen* quando é até
  soltar); clique solta.
- Cada congelar/soltar vira linha no Console da conta.
- Só Windows (o botão não aparece nas outras plataformas).

Código: [FreezeControl.tsx](../../src/components/session/FreezeControl.tsx)
(tela), [commands/freeze.rs](../../src-tauri/src/commands/freeze.rs) (estado,
relógio, comandos `freeze_clients` / `unfreeze_clients` / `frozen_clients`,
evento `freeze-changed`) e
[platform/windows/freeze.rs](../../src-tauri/src/platform/windows/freeze.rs)
(o mecanismo). Feature do Cargo `client-freeze`, nas duas edições via `standard`.

## Como funciona

O truque que a issue descreve — segurar o botão direito na barra de título do
Roblox — congela porque o Windows prende a **thread da janela** num laço próprio
até o botão soltar, e o jogo roda nessa thread. Esse laço não pode ser provocado
por mensagem: medido em 29/10/2026 numa janela de teste, o `WM_NCRBUTTONDOWN`
postado volta na hora, porque o Windows confere o botão de verdade (nem
`AttachThreadInput` + `SetKeyboardState` engana). Abrir o menu do sistema
congelaria também, mas mostra um menu na tela e fecha com qualquer clique.

Então o app **pausa a mesma thread** (`SuspendThread` na thread dona da janela,
`GetWindowThreadProcessId`) e a solta depois (`ResumeThread`):

- **Só a thread da janela**, nunca o processo inteiro: as outras threads do
  cliente (a rede) seguem rodando, como no truque do clique. Com o processo
  inteiro pausado a conexão cairia.
- Cada pausa do app é desfeita **uma vez só**: o app nunca solta uma pausa que
  não foi ele que pôs. Antes de soltar, confere que a thread ainda é do mesmo
  processo (`GetProcessIdOfThread`), porque o id pode ter sido reaproveitado.
- Conta já congelada que recebe Freeze de novo só troca o tempo.
- Tempo máximo de um congelamento com tempo: 1 hora. *Until I unfreeze* não tem
  teto.

## O que o resto do app faz com uma conta congelada

- **"Não respondendo"**: congelada, a janela fica "Não respondendo" para o
  Windows depois de 5 s. O app sabe que foi ele (`window_is_frozen`): o Painel
  de Sessão não mostra *Not responding*, o Watcher ("fechar se não responder")
  não a fecha, e o aviso de Discord *NotResponding* não sai.
- **AFK Mode** pula a vez dela (a tecla só chegaria depois e o foco numa janela
  parada não serve); ela volta vencida ao soltar.
- **Fechar o cliente** (Close, Auto Rejoin, Reconnect) funciona normalmente; a
  conta sai da lista de congeladas quando o cliente fecha ou reabre com outro
  pid.
- **Live** mostra o último quadro.
- **Fechar o MultiAlt** solta todas (`freeze_release_all`, no `ExitRequested` e
  no `Exit`). Se o app **cair** com contas congeladas, a lista fica em
  `RAMFrozen.json` (pasta de dados) e a próxima abertura solta as que ainda
  estão de pé (`freeze_recover_on_start`).
- Janela congelada que fica "Não respondendo" pode ganhar o "fantasma" do
  Windows (título com *(Não respondendo)*); clicar nela várias vezes pode trazer
  o "Fechar o programa?" do Windows — isso é do Windows, o app nunca fecha.

## A medir com cliente de verdade (pendente)

Validado só na janela de teste (`freeze_thread_tests`). Falta, com conta de
teste:

1. o Windows deixa abrir a thread do cliente (`OpenThread` com
   `THREAD_SUSPEND_RESUME`) — se negar, o erro aparece no Console e na tela;
2. a conta segue conectada congelada por 10 s, 1 min e 5 min, e volta ao jogo;
3. CPU e GPU do cliente caem para perto de zero enquanto congelado;
4. o Painel não mostra *Not responding* e nada fecha a conta;
5. fechar o MultiAlt com conta congelada solta a conta.

Testes: suíte `freeze` (`bun run t freeze`).
