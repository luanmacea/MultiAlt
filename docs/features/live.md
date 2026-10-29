# Live (transmitir as contas)

## Objetivo

Pedido do dono (28/10/2026): ajudar quem faz live. Antes, para mostrar várias
contas no stream, a pessoa capturava a tela inteira — e aparecia o que não devia
(outras janelas, a barra de tarefas). Agora a página **Live** escolhe quais
contas abertas aparecem e abre a janela **"MultiAlt Live"**: as contas lado a
lado, numa grade que sempre usa a janela inteira. No OBS, a pessoa adiciona uma
**Captura de janela** dessa janela, e só ela vai para o stream.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Miniaturas do DWM, encaixe na célula, conta minimizada restaurada invisível | [platform/windows/live_view.rs](../../src-tauri/src/platform/windows/live_view.rs) |
| Comandos: lista de contas, seleção, abrir/fechar a janela, grade, clicar para jogar | [commands/live_view.rs](../../src-tauri/src/commands/live_view.rs) |
| A janela (`?view=live`): mede o tamanho, calcula a grade, desenha nomes e molduras | [live/LiveWindow.tsx](../../src/live/LiveWindow.tsx), grade em [live/liveGrid.ts](../../src/live/liveGrid.ts) |
| A página Live (barra lateral, logo abaixo de Discord alerts) | [pages/LivePage.tsx](../../src/components/pages/LivePage.tsx) |
| Permissão da janela nova | [capabilities/live.json](../../src-tauri/capabilities/live.json) |
| Chaves | `[Live]` em [settings.md](settings.md) |

## Como funciona

- **Quem desenha é o Windows.** Cada conta é uma miniatura ao vivo do DWM (a
  mesma do Alt+Tab), registrada na janela da Live (`DwmRegisterThumbnail`) e
  posta no quadro da conta (`DwmUpdateThumbnailProperties`). O app não copia
  imagem nenhuma: pesa quase nada mesmo com muitas contas, e a imagem é a da
  própria janela, no ritmo dela.
- **A grade é da tela** (`liveGrid`): testa cada número de colunas e fica com o
  que dá o maior quadro 4:3 em que todas cabem; a grade fica centrada, com o nome
  da conta numa faixa em cima de cada quadro. A tela manda as células em pixels
  físicos para o backend (`live_set_layout`) sempre que muda (contas, tamanho da
  janela, nomes ligados ou não). O backend encaixa cada miniatura no quadro
  mantendo a proporção da janela de origem (`live_fit_rect`).
- **Quais contas:** as que têm cliente aberto (pelo app ou pelo site,
  reconhecidas) e estão marcadas na página (`Live.Accounts`; vazio = todas, até
  as que abrirem depois). A janela relê a lista a cada 2 s.
- **Clicar num quadro** traz a conta para a frente para jogar
  (`live_focus_account`).

## O que o Windows deixa (medido num cliente real em 28/10/2026)

Teste com duas contas no jogo e a miniatura registrada numa janela de teste:

| Janela do Roblox | Miniatura |
|---|---|
| Atrás de outras janelas | ao vivo |
| Fora da tela | ao vivo |
| Transparente (alfa 0 ou 1) | ao vivo, imagem inteira |
| **Minimizada** | **congela** no último quadro |

Por isso **"Keep minimized accounts live"** (`Live.KeepMinimized`, ligado): com a
Live aberta, conta minimizada é restaurada **invisível** (alfa 0: não aparece e
não recebe clique — `make_window_invisible`, a mesma peça do AFK), atrás de tudo
e sem pegar o foco. Quando a Live fecha (ou o app), ela volta a ser minimizada e
só então volta a aparecer (`live_release_hidden`). Clicar no quadro dela para
jogar a deixa visível de vez (`live_unhide`).

## Regras de negócio

- **Só Windows.** Nas outras plataformas a lista vem vazia.
- A Live **nunca fecha nem move** cliente nenhum; só a transparência e o
  minimizar/restaurar das janelas que ela mesma tirou do minimizado.
- Os nomes seguem "Names hidden" (`General.HideUsernames` + `HiddenNameLetters`);
  com "Show account names" desligado, não há faixa de nome.
- Fundo `dark`, `black` ou `green` (chroma key no OBS).

## Testes

`bun run t live` — Rust: `live_view_tests` (encaixe na célula),
`live_view_command_tests` (seleção: todas, lista, nenhuma); front:
[liveGrid.test.ts](../../src/live/liveGrid.test.ts) (maior quadro, nada
sobreposto, tudo dentro da janela, pixels físicos) e
[LivePage.test.tsx](../../src/components/pages/LivePage.test.tsx).

## Armadilhas / cuidados

- A captura do OBS: a miniatura do DWM faz parte da janela composta. Conferir
  com a **Captura de janela** no modo "Windows 10 (1903 e acima)"; o modo antigo
  (BitBlt) não vê miniaturas.
- O DWM dá o tamanho da janela inteira da origem, não só da área interna: o
  encaixe usa esse tamanho, e a imagem sai com uma faixa fina nas bordas quando
  a moldura do Roblox é grossa.
