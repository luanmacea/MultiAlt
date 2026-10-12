# Interface: shell, temas, lista de contas e ações

## Objetivo

Descrever como a janela principal é montada, como temas e fontes são aplicados, e como o usuário seleciona contas e dispara ações (sidebar de conta única, barra de ações em lote, tela "Choose Game").

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Raiz e roteamento de telas/diálogos | [App.tsx](../../src/App.tsx) |
| Estado global | [store.tsx](../../src/store.tsx) |
| Chrome da janela | [TitleBar.tsx](../../src/components/layout/TitleBar.tsx), [ModalWindowControls.tsx](../../src/components/layout/ModalWindowControls.tsx), [UpdateBanner.tsx](../../src/components/layout/UpdateBanner.tsx), [NavSidebar.tsx](../../src/components/layout/NavSidebar.tsx) (barra lateral), [Toolbar.tsx](../../src/components/layout/Toolbar.tsx), [StatusBar.tsx](../../src/components/layout/StatusBar.tsx) |
| Páginas | [src/components/pages/](../../src/components/pages): `PageShell` (casca comum), `SessionPage`, `ConsolePage`, `AfkPage`, `AvatarsPage` (+ `pages/avatars/`), `GroupsPage` (+ `pages/groups/`), `ScriptsPage`, `ThemePage`, `NexusPage`, `SettingsPage`, `ChangelogPage` |
| Notas de release (janela de atualização e "What's new") | [releaseNotes.ts](../../src/releaseNotes.ts) (limpeza do texto, versões, leitura das releases), [ReleaseNotesMarkdown.tsx](../../src/components/ReleaseNotesMarkdown.tsx) (desenho do markdown) |
| Telas bloqueantes | [PasswordScreen.tsx](../../src/components/layout/PasswordScreen.tsx), [EncryptionSetupScreen.tsx](../../src/components/layout/EncryptionSetupScreen.tsx), [FirstRunWalkthrough.tsx](../../src/components/layout/FirstRunWalkthrough.tsx) |
| Tutoriais de tela (botão "Tutorial") | [src/components/tour/](../../src/components/tour): `tours.ts` (os passos), `ScreenTour.tsx` (motor + `ScreenTourHost`), `TourButton.tsx`, `tourState.ts` (aberto / já visto), `useSpotlight.tsx` (destaque, compartilhado com o walkthrough), `placement.ts` |
| Lista de contas | [AccountList.tsx](../../src/components/accounts/AccountList.tsx), [GroupSection.tsx](../../src/components/accounts/GroupSection.tsx), [AccountRow.tsx](../../src/components/accounts/AccountRow.tsx), [AccountChip.tsx](../../src/components/accounts/AccountChip.tsx) |
| Sidebar (conta única) | [DetailSidebar.tsx](../../src/components/accounts/DetailSidebar.tsx) → [SingleSelectSidebar.tsx](../../src/components/accounts/SingleSelectSidebar.tsx), [SidebarSection.tsx](../../src/components/accounts/SidebarSection.tsx) |
| Ações em lote | [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx) |
| Tela de escolha de jogo / launch em lote | [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) |
| Menu de contexto | [ContextMenu.tsx](../../src/components/menus/ContextMenu.tsx), [MenuItemView.tsx](../../src/components/menus/MenuItemView.tsx) |
| Diálogos | [src/components/dialogs/](../../src/components/dialogs), [ServerListDialog.tsx](../../src/components/server-list/ServerListDialog.tsx) |
| Tema | [theme.ts](../../src/theme.ts), [themeFonts.ts](../../src/themeFonts.ts), [fontPresets.ts](../../src/fontPresets.ts), [ThemePage.tsx](../../src/components/pages/ThemePage.tsx), backend [theme.rs](../../src-tauri/src/data/settings/theme.rs) e [presets.rs](../../src-tauri/src/data/settings/presets.rs) |
| Tamanho da interface (zoom) | [uiScale.ts](../../src/uiScale.ts) (regra do automático, pura), [useUiScale.ts](../../src/hooks/useUiScale.ts) (aplica com `setZoom`), controle em [AppTab.tsx](../../src/components/settings/AppTab.tsx) |
| Componentes genéricos | [src/components/ui/](../../src/components/ui) |
| Hooks de UI | [usePrompt.tsx](../../src/hooks/usePrompt.tsx) (`prompt`/`confirm` assíncronos), [useModalClose.ts](../../src/hooks/useModalClose.ts) (animação de fechar), [useBackdropClose.ts](../../src/hooks/useBackdropClose.ts) (clique no fundo fecha o modal), [useJoinOnlineWarning.ts](../../src/hooks/useJoinOnlineWarning.ts), [useMouseNavigation.ts](../../src/hooks/useMouseNavigation.ts) (Voltar/Avançar do mouse e Alt+setas) |

## Fluxo

### Montagem da janela ([App.tsx](../../src/App.tsx))

Ordem de decisão:

1. `!initialized` → "Loading...".
2. `needsPassword` → `PasswordScreen` (fundo animado conforme `General.RestrictedBackgroundStyle`).
3. `encryptionSetupOpen` → `EncryptionSetupScreen`.
4. App principal:
   - `ModalWindowControls` (controles de janela visíveis quando há modal aberto) + `TitleBar` (a janela nativa é criada **sem decorações** em [tauri.conf.json](../../src-tauri/tauri.conf.json), 1100×700, mínimo 750×450);
   - `UpdateBanner`;
   - à esquerda a `NavSidebar` (barra lateral); à direita a área principal, que mostra a página de `store.activePage`:
     - `accounts`: `Toolbar`, e o corpo de sempre — `ChooseGameScreen` **ou** `AccountList` + `DetailSidebar` (só com `sidebarOpen` e exatamente 1 conta selecionada), mais a `BottomActionBar` quando há seleção e a Choose Game não está aberta;
     - as outras: a página correspondente (ver "Barra lateral e páginas");
   - faixa de erro (`store.error`) no topo da área principal, em qualquer página, com botão "Close Roblox" quando o erro menciona falha no multi-Roblox;
   - `StatusBar` (largura inteira), `ContextMenu`, toasts, os diálogos e `IsolationProgressOverlay`;
   - modal genérico `store.modal` (título + `<pre>`), usado para mostrar textos longos.

**Todo diálogo cabe na janela mínima** (750x450, `tauri.conf.json`): o quadro leva `max-w-[calc(100vw-24px)]` e `max-h-[calc(100vh-24px)]` (ou `max-h-[min(<teto>,calc(100vh-24px))]` quando já tinha teto próprio), e quem cede altura é o miolo (`flex-1 min-h-0`, que rola por dentro) — nunca o cabeçalho com o X nem o rodapé com Save/Cancel. Tamanho fixo sem esse teto jogava título e botões para fora da tela: Server List, Theme Editor, Nexus e Account Utils (medido no harness, 26–27/09/2026). O menu **Add** da toolbar segue a mesma regra, com `max-h-[calc(100vh-96px)]` e rolagem própria.

### Barra lateral e páginas

Até 03/10/2026 a Toolbar tinha uma fileira de ícones (Session, Theme, Nexus, AFK, Avatars, Scripts, Settings, Help) que só dizia o que cada um fazia com o mouse parado em cima, e cada um abria um modal centralizado. Agora a [NavSidebar](../../src/components/layout/NavSidebar.tsx) mostra os nomes sempre, e cada item abre uma **página** na área principal:

| Item | Página | Observação |
|---|---|---|
| **Accounts** | lista de contas (home) | contador de contas |
| **Session** | `SessionPage`, o centro de quem joga com muitas contas. **Primeiro, em qualquer largura, as listas**: a faixa de números **Right now** (Playing / Reconnecting / Not responding / Dropped, só quando há / Joining — [SessionStats.tsx](../../src/components/session/SessionStats.tsx)), tirada dos mesmos dados das linhas (o cartão antigo contava "em jogo" pela presença e dizia "–" com ela desligada); cada número filtra a lista **In game** (segundo clique volta a todas; filtro sem ninguém se desfaz sozinho) e **Joining** rola até a fila. Depois o Painel de Sessão (`showStats`, `contained={false}`: a página rola inteira, sem caixa de rolagem dentro das listas). **Depois os cartões de ajuste** (`aside` "Session settings"): primeiro o **While you were away** (o que aconteceu enquanto ninguém olhava — ver [abaixo](#while-you-were-away-o-que-aconteceu-longe-do-pc)); quem mantém as contas no jogo, com o padrão "Reconnect accounts that drop"; no Windows, o cartão **Recordings** — a gravação de todas as contas e quando ela toca, no Modo AFK e depois da reconexão, com **Open Recordings** ([recordings.md](recordings.md#a-tela)); o teto de memória com "Close a client that stays over its limit". Abaixo de 1280 px os cartões ficam embaixo das listas, em duas colunas; acima, numa coluna ao lado (até 11/10/2026 ficavam ao lado já em 1024 px e, abaixo disso, **em cima** das listas — o "In game" começava a 3,5 telas de altura em 820 px) | contador de clientes rodando (`launchedByProgram`) |
| **Console** (logo abaixo de Session) | `ConsolePage`: só o histórico do que o app fez — launch, Auto Rejoin, Watcher e erros, cada linha com a origem ([LaunchConsole.tsx](../../src/components/console/LaunchConsole.tsx), evento `launch-log`). As listas da sessão (Entrando, Em jogo) **não** se repetem aqui: moram só na página Session. Era a aba Console da Choose Game até 11/10/2026 (pedido do dono: o painel por conta quase não é usado, e a página Session é o centro de quem tem muitas contas) | |
| **AFK Mode** | `AfkPage` → `AfkModeView variant="page"` (Auto Rejoin + AFK) | ponto verde "On" com AFK ou Auto Rejoin ligado |
| **Avatars** | `AvatarsPage` (abas Montar/Distribuir sob o cabeçalho) | |
| **Groups** (pt/es "Grupos") | `GroupsPage` (+ `pages/groups/`): busca de grupos e entrada das contas marcadas, uma por vez (ver [groups.md](groups.md)) | selo "Entrando n/m" no cabeçalho durante o lote |
| **Scripts** | `ScriptsPage` (lista + editor) | |
| **Theme** | `ThemePage`: presets, todas as categorias em cartões, prévia fixa ao lado | |
| **Nexus** | `NexusPage` (só com `ENABLE_NEXUS`) | Start/Stop no cabeçalho |
| **Settings** | `SettingsPage`: seções numa lista vertical à esquerda — as do dia a dia (Launch, Keep in game, Accounts, Privacy & security, Updates, App, Backups) em cima, as técnicas embaixo sob o rótulo **Advanced** —, conteúdo com teto de largura. General e Misc viraram as seis primeiras em 11/10/2026 (ver [settings.md](settings.md#seções)); **Backups** é uma seção própria (era um diálogo até 03/10/2026). Outra tela abre uma seção com `store.openSettingsSection(id)` | |
| **What's new** (rodapé; pt "Novidades", es "Novedades") | `ChangelogPage`: o que cada versão mudou, da mais nova para a mais antiga (ver abaixo) | fica junto do Help: os dois falam do app, não do trabalho com as contas |
| **Help** (rodapé) — **escondido** | reabre o walkthrough, que leva de volta à lista de contas | escondido desde 08/10/2026 a pedido do dono (pode voltar como FAQ): fica atrás de `ENABLE_HELP_BUTTON` em [featureFlags.ts](../../src/featureFlags.ts), desligado. O código continua; o walkthrough segue em Settings › App › "Open Walkthrough". Para religar: `VITE_ENABLE_HELP_BUTTON=true` no build ou o padrão em `true`. O rodapé (pedido do dono, 08/10/2026): **Discord** em cima (12/10/2026: abre o convite do servidor da comunidade no navegador pelo comando `open_discord_invite`, com o endereço numa constante do backend), **Send feedback** logo abaixo e, na última linha, **What's new** ao lado do botão de recolher; recolhida, os dois empilham no centro; com a janela estreita (que já recolhe sozinha) o recolher não aparece |

- **Estado:** `store.activePage` (`AppPage`) + `setActivePage`. Não há roteador. Os setters antigos (`setSettingsOpen`, `setThemeEditorOpen`, `setAvatarsDialogOpen`, `setSessionDialogOpen`, `setNexusOpen`, `setScriptsOpen`) continuam e viram navegação: `true` abre a página; `false` volta para Accounts **só se** aquela página for a aberta. Os booleanos `settingsOpen`/`scriptsOpen`/... saíram — quem quer saber lê `activePage`.
- **Casca comum:** [PageShell](../../src/components/pages/PageShell.tsx) — cabeçalho com título (`h1`), uma frase do que a página faz e as ações dela; leva `theme-modal-scope`, que traduz as classes `zinc-*` herdadas dos modais para as variáveis do tema.
- **Escape** volta para Accounts, pela pilha de Escape (`useEscapeStack`): popover ou diálogo aberto por cima consome antes; Escape digitado num campo não sai da página.
- **Voltar/Avançar do mouse e Alt+setas** (pedido do dono, 11/10/2026): os botões laterais do mouse (3 = Voltar/XButton1, 4 = Avançar/XButton2) e Alt+Esquerda/Alt+Direita funcionam como no navegador. **Voltar** fecha o que está aberto por cima (o topo da pilha de Escape: diálogo, Choose Game, menu, tutorial) exatamente como o Escape fecharia; sem nada por cima, volta à página anterior. **Avançar** anda para a frente; navegar para outro lugar depois de voltar apaga o Avançar. A casca da página e a seleção da lista de contas entram na pilha com `closesOnBack: false`: são a base da tela, então o Voltar anda no histórico em vez de cair em Accounts ou só limpar a seleção. O histórico ([pageHistory.ts](../../src/utils/pageHistory.ts), puro) guarda até 50 páginas, sem repetir a mesma em sequência, e observa `activePage` — qualquer troca entra, venha da barra lateral ou de um botão. Sem página anterior, o Voltar não faz nada. O `preventDefault` nos botões 3/4 (mousedown, mouseup, auxclick) impede o WebView2 de agir sozinho; mouseup e auxclick do mesmo clique contam uma vez. Alt+setas dentro de campo de texto ficam com o campo. Desligado na tela de senha, na de criptografia e com o app trancado. Código: [useMouseNavigation.ts](../../src/hooks/useMouseNavigation.ts), ligado uma vez em `App.tsx`.
- **Montagem:** as páginas que eram modais ficam montadas o tempo todo e só aparecem quando ativas, como os modais ficavam: o `ScriptsPage` mantém os workers dos scripts em execução e o auto-start, o `AvatarsPage` o ouvinte do lote (`avatar-batch-state`), o `GroupsPage` o do lote de grupos (`groups-join-state`), o `SettingsPage` recarrega ao entrar e chama `reloadSettings` ao sair. A página de contas (e a Choose Game) desmonta ao trocar de página: a Choose Game volta aberta (`chooseGameOpen` continua valendo), mas na aba inicial.
- **Theme:** sair da página sem salvar (barra lateral, Escape, walkthrough) devolve o tema salvo — o que o Escape/Cancel do modal faziam. "Discard changes" faz o mesmo sem sair; "Save" salva e fica.
- **Barra de título:** página não é modal — não entra no `anyModalOpen`, e os botões de janela continuam na `TitleBar`.
- **Recolher:** o botão no rodapé deixa só os ícones (tooltip à direita, só nesse modo). A escolha fica em `localStorage` (`ram_nav_collapsed`, com `try/catch`); abaixo de 900 px de largura a barra recolhe sozinha e o botão some.
- **Walkthrough:** os passos apontam `data-tour="nav-session"`/`"nav-settings"` (ou a página, se aberta: `session-page`/`settings-page`); os passos da lista de contas trazem a página de contas de volta.

### Tutoriais de tela (botão "Tutorial")

Pedido do dono (03/10/2026): "o app tem muita coisa agora; tutoriais opcionais em cada tela ajudam a guiar". Quem usa quase nunca é técnico — cada passo é uma ou duas frases curtas, com o nome do botão como ele aparece na tela.

- **Onde fica o botão:** no cabeçalho de cada página (`PageShell` com `tour="..."`, antes das ações), na Toolbar da lista de contas (some com a Choose Game aberta — aí vale o da Choose Game) e no cabeçalho da Choose Game. Nexus não tem.
- **Nunca abre sozinho.** O único sinal é um pontinho na cor de destaque no botão enquanto a pessoa nunca abriu o tutorial daquela tela. "Já visto" fica em `localStorage` (`ram_tours_seen`, com `try/catch`): storage bloqueado só faz o pontinho voltar na próxima abertura.
- **Tutoriais** ([tours.ts](../../src/components/tour/tours.ts), dados puros): Accounts (lista, selecionar, estado ao vivo / chip "precisam de atenção" / Entrar novamente, Add, painel da conta, Choose Game, nomes ocultos), Choose Game (contas, abas, Games, Servers, Friends), Console (grade de janelas, log), Session (Entrando, Em jogo, AFK Mode/Fechar contas, cliente não identificado, While you were away, resumo), AFK Mode (cliques: configurações, contas, ligar/parar; Auto Rejoin: onde, quem/iniciar), Avatars (montar, peças, prévia, salvar, distribuir, aplicar), Groups (achar, escolher o grupo, contas, entrar), Scripts, Theme, Settings (seções do dia a dia e Advanced, Launch, salvamento automático, Backups), What's new. 3 a 6 passos cada.
- **Motor** ([ScreenTour.tsx](../../src/components/tour/ScreenTour.tsx)): Voltar/Avançar, "Passo N de M", X e Escape fecham (o tutorial está no topo da pilha de Escape: o primeiro Escape fecha o tutorial, não a página), setas do teclado andam. O painel fica colado no que aponta (embaixo, em cima, ao lado; alvo grande demais recebe o painel por dentro, no canto) e nunca sai da janela ([placement.ts](../../src/components/tour/placement.ts)). O alvo que está rolado para fora da vista é trazido para a tela.
- **Alvo que falta não quebra:** cada passo lista seletores `data-tour` em ordem de preferência; nenhum na tela → o passo aparece no centro, sem destaque, com "Esta parte não está na tela agora". A pessoa saiu da tela (barra lateral, Voltar) → o tutorial fecha. O tour de boas-vindas (Ajuda) abrindo → o tutorial de tela fecha.
- **Só olha, não mexe:** o único clique que um passo dá é o `reveal` — abrir a aba ou seção que ele explica (abas da Choose Game, do AFK Mode e do Avatars; a seção Backups). Nenhum passo lança, salva, apaga, seleciona conta ou liga modo. Os testes de cada tela (`walkTour`, [tourHelpers.tsx](../../src/test-utils/tourHelpers.tsx)) andam pelo tutorial inteiro conferindo que cada alvo está na tela e que nenhum comando que muda dado passou pelo `invoke`.
- **Destaque compartilhado** com o tour de boas-vindas: [useSpotlight.tsx](../../src/components/tour/useSpotlight.tsx) (acha o alvo a cada 140 ms, anel com o resto escurecido).
- **Tutorial novo:** escreva os passos em `tours.ts` (`title`/`description` em inglês — o extrator de chaves lê esses campos), ponha os `data-tour` que faltarem (escopados pela página quando o mesmo componente existe num modal, como o AFK Mode), passe `tour` ao `PageShell` e adicione um `walkTour` no teste da tela. Traduza em pt e es.

### Página "What's new" (`ChangelogPage`)

Pedido do dono (03/10/2026): uma página que diga, para quem não é técnico, o que cada atualização mudou.

- **De onde vem:** as releases do repositório (`GET <REPO_API_URL>/releases?per_page=30`, [releaseNotes.ts](../../src/releaseNotes.ts)) — a mesma exceção "frontend lê `api.github.com`" que a janela de atualização já tinha ([architecture.md](../architecture.md#exceções-à-regra-frontend-não-acessa-rede)).
- **Quando pede:** só ao abrir a página. Abrir o app não faz pedido nenhum (o GitHub dá 60 pedidos/hora sem login). A lista fica em memória pela sessão: voltar à página não pede de novo. Falha não fica guardada — "Try again" pede outra vez.
- **O que mostra de cada versão:** só a seção `## What's Changed` (`changelogNotes`). Da v0.1.10 em diante ela já é a lista em linguagem simples (a lista técnica, recolhida em `<details>`, sai). Até a v0.1.9 era a lista automática de títulos de PR: sai o " by @autor in …/pull/N", a linha "Full Changelog" e os itens `[skip release]` (PR que não publica versão — README, site —, logo não mudou o app). Release sem a seção, ou que ficou vazia, não aparece; rascunho também não.
- **Marcas:** a versão instalada (`getVersion`) leva ponto cheio na cor de destaque e "Your version"; versões mais novas, ponto vazado e "Not installed yet". A mais nova delas tem o botão para o fluxo de atualização que já existe: **"Update available"** abre a `UpdateDialog` quando o updater já achou a versão (`store.updateInfo`); sem isso, **"Check for Updates"** roda a checagem manual (`checkForUpdates(true)`), que diz "No updates available" se o canal do usuário (stable/beta) não tem aquela versão. Versão comparada sem o `v` e sem o canal (`v0.1.10-beta` = `0.1.10`).
- **Tipo da atualização (selo):** ao lado da versão, o selo do tipo da release (pedido do dono, 04/10/2026) — **Fix** (pt "Correção", es "Corrección"; âmbar), **New features** ("Novidades"/"Novedades"; cor de destaque do tema) ou **General update** ("Atualização geral"/"Actualización general"; violeta). Componente [ReleaseKindBadge.tsx](../../src/components/ReleaseKindBadge.tsx), o mesmo da janela de atualização (`UpdateDialog`, ao lado de "v<versão>" sob o título). O tipo vem de `releaseKindOf` ([releaseNotes.ts](../../src/releaseNotes.ts)): a marca `<!-- release-kind: fix|feature|mixed -->` que o workflow escreve na primeira linha do texto da release (regra em [docs/development.md](../development.md), "Tipo da release"); sem marca, a lista em linguagem simples é classificada do mesmo jeito (itens "Fixed:"); release antiga, com títulos de PR, fica sem selo. A marca e o selo em markdown do GitHub ("🩹 Hotfix", "✨ New features", "📦 General update") nunca aparecem como texto nas notas. Na janela de atualização, o texto final da release (lido do GitHub) vence o do manifesto, que sai antes dele.
- **Estados:** esqueleto enquanto carrega; erro com "Try again" e o link para as releases no GitHub — mensagem própria quando o GitHub está limitando (403 com `x-ratelimit-remaining: 0`, ou 429); "No updates to show yet." quando a lista vem vazia.
- **Layout:** linha do tempo — versão e data à esquerda, alinhadas ao trilho, e a lista à direita; em área estreita (contêiner abaixo de `@xl`) a versão sobe para cima da lista. Data no idioma do app (`Intl.DateTimeFormat`, `dateStyle: "long"`).
- **Harness:** `?scenario=changelog` responde o `fetch` do GitHub com releases no formato real de cada época (`src/dev/harness/releases.ts`); `&current=`, `&update=1`, `&fail=offline|rate`, `&delay=`. As duas mais novas trazem a marca do tipo (0.2.1 correção, 0.2.0 geral); a 0.1.10 cai na classificação pela lista (novidades); as antigas ficam sem selo. A janela de atualização sozinha: `?scenario=update&kind=fix|feature|mixed|none`.

### Toolbar

Só na página de contas, e só com o que age na lista (mais o chip de atenção, abaixo): busca (filtra por username, alias, descrição, grupo), selecionar tudo, ocultar nomes, abrir o painel da conta, **Presets** (abre a lista de presets de launch — [presets.md](presets.md)) e menu **Add**: Quick Add (cookie ou username), Browser Login, User:Pass Login, Import Cookie, Import Old Account Data, **Create Accounts** (cadastro no navegador — [account-creation.md](account-creation.md)), Account Generator (só com `ENABLE_ACCOUNT_GENERATOR`, desligado por padrão) e Roblox Versions.

**Chip "N precisam de atenção"** (âmbar, à esquerda dos botões): quantas contas caíram, estão reconectando, não respondem ou **fecharam** — as com cliente aberto, as que reconectam com o cliente já fechado e as que têm o aviso "Fechou" (decisão de 12/10/2026: ninguém vai reabrir essa conta sozinho, então ela espera a pessoa — "Entrar novamente" ou o ×; reconexão que desistiu ou parou sem esse aviso não conta). Só aparece com N > 0; o clique abre a página Session, onde se age sobre elas. A contagem é `attentionIds` ([utils/accountLiveState.ts](../../src/utils/accountLiveState.ts)), da mesma store que alimenta a Session.

### Nomes ocultos (`Names hidden`)

O botão da toolbar existe para gravar ou compartilhar a tela sem expor as contas, então vale para **o app inteiro**, não só para a lista. Regra:

- **Toda tela que mostra o nome de uma conta do usuário passa pelo helper compartilhado** [utils/accountName.ts](../../src/utils/accountName.ts) — `accountLabel(account, store, fallback)` fora de componente, ou o hook `useAccountLabel()` ([hooks/useAccountLabel.ts](../../src/hooks/useAccountLabel.ts)). Vale para texto, `title`, `aria-label`, placeholder, prompt, confirmação e toast. Nada de `account.Alias || account.Username` direto na tela: até 03/10/2026 a máscara estava copiada em sete arquivos, e o AFK Mode, o Avatars, diálogos, o menu de contexto e os toasts mostravam o nome real com o modo ligado.
- A máscara: as primeiras `HiddenNameLetters` letras e `********`; com `0` (ou prévia que mostraria tudo) vira `************`. O fallback (ex.: "User ID: 123") também é mascarado — identifica a conta tanto quanto o nome.
- **Foto:** some junto com o nome (`hideAccountAvatar`: `hideUsernames && !showAvatarsWhenHidden`). A letra do círculo sem foto (`accountInitial`) só aparece se a prévia já a mostraria.
- Campo que nasce com o nome da conta (alias na sidebar, "Set Alias" do menu, username do Outfits) fica em bolinhas (`masked-input`).
- **Só o que aparece é mascarado.** O que vai para o backend (`add_account`, `set_avatar`...) e o que é copiado para a área de transferência continua com o nome real.
- **Não mascara:** nomes de terceiros (amigos, jogador procurado no Server List — ver [friends.md](friends.md)); o stream de log do backend na página Console (o nome da conta na coluna é mascarado, mas o texto da linha vem pronto do backend); o painel de criação de contas, que mostra usuário **e senha** da conta que está sendo cadastrada; os dados entregues aos scripts do usuário.
- Teste: suíte `hidden-names` (`bun run t hidden-names`) — cada tela tem um caso "com nomes ocultos o nome real não está no documento".

### Lista de contas

- Contas agrupadas por `Group` (ver regras em [accounts.md](accounts.md)); grupos podem ser colapsados e têm menu (ordenar alfabeticamente, copiar, alternar visibilidade).
- **Seleção** (`handleSelect` em [store.tsx](../../src/store.tsx)): clique simples seleciona só a conta; Ctrl/Alt/Cmd alterna; Shift seleciona o intervalo desde o último clique na ordem visível (grupos colapsados ficam fora); Shift+Ctrl soma o intervalo. Arrastar no fundo da lista faz seleção por retângulo (inclui membros de grupos colapsados quando o retângulo cobre o cabeçalho).
- **Arrastar** pela alça (ícone de grip à esquerda da linha, não a linha inteira — o corpo da linha é usado pela seleção por retângulo) para outro grupo move as contas selecionadas para esse grupo; soltar sobre outra linha reordena (`reorder_accounts`). `onDragEnd` na alça limpa `dragState`, então um arrasto cancelado (Esc / soltar fora) não deixa estado velho que um drop posterior de texto de cookie usaria.
- **Soltar texto** na lista: extrai todos os cookies `_|WARNING:-DO-NOT-SHARE-THIS...|<token>` e adiciona cada um.
- Cada linha mostra avatar (48×48, `batched_get_avatar_headshots`), nome/alias, tempo desde `LastUse`, pontos de status e botão "Join".
- **Estado ao vivo** ([AccountLiveSegment.tsx](../../src/components/accounts/AccountLiveSegment.tsx), crítica de design P0 de 11/10/2026 — com 7 clientes abertos a linha só tinha a bolinha laranja e "now"): conta com cliente aberto, entrando ou reconectando ganha, à direita do nome, a palavra do estado com a cor/ícone da Session (Jogando verde, Entrando na cor de destaque, Reconectando e Não respondendo âmbar, Caiu vermelho, "Ainda fora de um jogo" apagado) e o jogo (`useGameIdentity`; sem nome, o Place ID). Caiu sem jogo conhecido (a queda fecha a sessão): o motivo da queda no lugar do jogo. Em jogo, também o tempo em jogo. Tudo no tooltip.
  - **Mesmas fontes e prioridade da Session:** `accountLiveState` ([utils/accountLiveState.ts](../../src/utils/accountLiveState.ts)) usa o `runningStatus` de [sessionGroups.ts](../../src/utils/sessionGroups.ts) sobre `launchedByProgram`, `clientHealth`, `autoReconnect` (`activeReconnectIds`, o mesmo da Session), a fila de launch (`launching`) e a sessão de agora. A sessão (`get_current_sessions`) é lida **uma vez para a lista** pelo `AccountLiveStateProvider` ([hooks/useAccountLiveState.tsx](../../src/hooks/useAccountLiveState.tsx)), e só com algum cliente aberto no Windows.
  - Conta parada (ou cliente aberto sem informação, fora do Windows): nada novo — a linha fica como era.
  - **Uma linha só** (a altura da linha não muda). A linha é um container (`@container/row`): abaixo de ~34rem o jogo some e só fica a palavra; abaixo de ~46rem some o relógio. O jogo corta antes da palavra. Janela ≥1600 px solta o teto de largura e o nome do jogo aparece inteiro.
  - A palavra "Caiu" usa a chave `account-dropped`: o "Dropped" da faixa da Session é plural em português ("Caíram").
  - **Fechou** (pedido do dono, 12/10/2026 — a conta sumia da linha sem dizer nada): cliente aberto pelo app que fechou no meio do jogo e que ninguém está reabrindo ([launch.md](launch.md#cliente-que-fechou-e-entrar-novamente)) mostra "Fechou" (âmbar, ícone de desligar; chave `account-closed`), o motivo da queda (mesmas frases da Session: "Fechou sem sair do jogo", "Expulso: …", "O servidor fechou"…), o jogo e, no tooltip, a hora. No fim do pedaço, **Entrar novamente** (o launch normal, para o mesmo jogo e servidor; desligado enquanto a conta espera na fila) e o **×** que dispensa o aviso. Os botões não selecionam a linha. Prioridade: Reconectando e Entrando vencem o Fechou; o aviso some quando a conta abre de novo (qualquer launch), no × ou ao reabrir o app.

### Sidebar de conta única (`SingleSelectSidebar`)

Cabeçalho (nome, presença, aviso de queda — quebra em até 3 linhas —, validade, moderação) e, na área rolável, **nesta ordem** (o uso diário primeiro): **Tools** (Server List, Utilities, Browser, Join Group), **Launch Exceptions**, **Roblox Version (all accounts)** — grava a `Versions.DefaultVersion` **global**, não o campo `RobloxVersion` da conta (com "Latest installed" e "Manage versions...", e só aparece com alguma versão no catálogo) —, **Alias**, **Description** e por último **History** (sessões recentes, tempo de jogo de 14 dias, "Join again" e export CSV — [history.md](history.md)), recolhível pelo título (`SidebarSection collapseId`, lembrado no `localStorage`). Trocar de conta volta a rolagem ao topo. Antes a ordem era Alias › Description › History › ..., e as ferramentas ficavam 4–6 telas abaixo. A versão **por conta** existe no backend (`RobloxVersion`, `versions_set_account_override`), mas nenhuma tela a define — ver [roblox-versions.md](roblox-versions.md).

**O que vale para muitas contas não mora aqui** (decisão do dono, 10/10/2026: o painel mostra uma conta por vez e quase não é aberto por quem tem muitas). A reconexão automática saiu do painel e foi para a página Session — padrão no resumo, chave por conta e lote na lista **In game** (ver [Painel de Sessão](#painel-de-sessão-sessionpanel)). Configuração nova desse tipo vai para lá, não para cá. O **History** continua aqui.

### Barra de ações em lote (`BottomActionBar`)

Aparece com ≥ 1 conta selecionada:

| Ação | Comportamento |
|---|---|
| Deselect | limpa seleção |
| Account settings | alterna o sidebar |
| Refresh Cookies | `refresh_cookie` em série, 2 s entre contas |
| Copy cookies | copia cookies (um por linha) |
| Make Friends | `make_selected_friends` modo `mesh` ou `star`; confirma acima de 30 pedidos; progresso via `friend-link-state`, com uma entrada **por conta** no Painel de Sessão (aguardando/processando/amizade feita/erro) e o contador "X / Y contas processadas" |
| Move to Group / New group | `moveToGroup` |
| Restart launched clients | só contas lançadas pelo app |
| Auto Rejoin | abre diálogo ou adiciona contas ao Auto Rejoin ativo |
| Close All Roblox | `killAllRobloxProcesses` |
| Remove | exige digitar `REMOVE` |
| **Choose Game** | abre a `ChooseGameScreen` |

### Choose Game ([ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx))

Substitui a lista de contas; fecha com Esc ou "voltar". Mostra as contas selecionadas no topo e abas:

| Aba | Faz |
|---|---|
| Favorites | favoritos com VIPs; clicar lança todas as contas selecionadas (público ou VIP). |
| Games | busca de jogos; clicar lança todas as selecionadas. |
| Recent | jogos recentes. |
| Follow | Campo de join link ([join-links.md](join-links.md)) e o card "Follow a Player": `lookup_user` + `get_presence` resolvem o servidor do alvo **uma vez** e `launchAll` manda todas as contas selecionadas para lá (`launch_multiple` com várias contas, com o piso anti-captcha). Alvo fora de jogo (`presence < 2`) vira toast e nada é lançado; em jogo mas com o servidor escondido pela privacidade, pede confirmação para cair num servidor público do mesmo jogo. (Isto já foi um laço de `launch_roblox` com `followUser: true` e 3 s entre contas, que furava o piso de 8 s.) Atalhos para Server List, Utilities, Auto Rejoin e Scripts. |

**Console e Windows saíram da Choose Game em 11/10/2026** (pedido do dono): viraram a página **Console** da barra lateral — o log e, no topo, o cartão da grade de janelas (ver [Barra lateral e páginas](#barra-lateral-e-páginas)). As listas da sessão moram só na página **Session**. A Choose Game ficou só com as formas de escolher o jogo.

**Depois de mandar entrar, volta para a lista de contas** (pedido do dono, 12/10/2026): Join Game, Favorites, Servers, Friends e Follow fecham a Choose Game e mostram a lista de contas, que já diz o estado de cada uma ao vivo (Entrando, Jogando...). Antes iam para a Session e a Choose Game ficava aberta por baixo — clicar em Contas a reabria com o lote anterior. O link resolvido continua com o botão **Open Session**, que é um pedido explícito.

`launchAll` (hook `useLauncher`): confirma contas online, leva para a página **Session** (onde está a fila de quem está entrando; era a aba Console), grava `placeId`/`jobId` na store e chama `joinServer` (1 conta) ou `launchMultiple` (várias) passando o alvo explicitamente. O registro nos recentes é feito pela **store**, só quando o `invoke` de launch retorna sucesso (a `ChooseGameScreen` não registra mais por conta própria).

**Trocar de aba não perde o que foi carregado.** Cada aba é desmontada ao sair, então o que ela buscou vive num cache de memória da sessão ([utils/sessionCache.ts](../../src/utils/sessionCache.ts)) — nunca em disco nem no `localStorage`, e some ao fechar o app. Ao voltar (para a aba ou para a Choose Game), a aba mostra o que tinha no primeiro desenho e atualiza por trás, com um indicador discreto, trocando os dados quando a resposta chega:

| Aba | Chave do cache | Indicador |
|---|---|---|
| Games | última busca (uma só) | *Updating...* ao lado da busca |
| Servers | place + ordem + lote + páginas; só varredura terminada | *Updating...* ao lado do Refresh; a lista só troca no fim da varredura nova ([server-choice.md](server-choice.md#voltar-à-aba-cache-da-varredura)) |
| Friends | ids das contas selecionadas | spinner por conta; cada conta troca quando a dela volta ([friends.md](friends.md#carga-progressiva-e-cache-da-aba)) |

Favorites e Recent já nasciam prontos (vêm do `localStorage`); Follow não busca nada da rede. O cartão da grade, na página Console, guarda o último `list_display_monitors` do mesmo jeito. A volta **não** faz requisição a mais que antes — ela já recarregava; a aba Friends até economiza, juntando-se a uma rodada que ainda esteja em curso. Os testes zeram esses caches pelo `resetTauriMocks()`.

Controles de grade (cartão **Window grid** no topo da página **Console**, [WindowGridCard.tsx](../../src/components/console/WindowGridCard.tsx); era a aba Windows da Choose Game até 11/10/2026): monitores, **Gap**, **Arrange in grid** e **Arrange in grid on launch**. O campo **Gap** (0–200 px) é editado como texto e só é aplicado/persistido (`General.GridGap`) ao perder o foco ou com Enter (`commitGap`); valor inválido volta ao anterior. Os monitores marcados, o gap e a grade automática são **lidos de `store.settings` e gravados por `store.updateSetting`**, que atualiza a store junto com o INI. Antes o cartão guardava a seleção num estado próprio, nascido da store, e gravava direto no backend: a store ficava velha e, ao reabrir a tela, o monitor desmarcado aparecia marcado de novo (travado em `WindowGridCard.test.tsx`). Só no Windows.

### Grade de janelas

A grade é feita de **células fixas** ([windowing.rs](../../src-tauri/src/platform/windows/windowing.rs), `grid_slots`): todas as células do tamanho da janela que cabem na área de trabalho dos monitores escolhidos (`GridMonitors`, vazio = todos), com `GridGap` entre elas, da esquerda para a direita e de cima para baixo, monitor por monitor. Tamanho da célula: o tamanho global (`OverrideClientWindowSize` + `ClientWindowWidth/Height`) quando ligado; senão, na grade automática, o tamanho com que a janela nova abriu, e no botão manual o tamanho mais comum entre as janelas. **A célula cresce até o tamanho que a janela aceitou** (`accepted_size` + `grid_cell_size`): o Roblox não deixa a janela menor que ~800x600 de área útil, então um global de 520x420 vira janela de 816x638 — medido no teste real de 03/10/2026, quando células de 520 sobrepunham as alts.

- **Automática no launch** (`General.AutoArrangeGrid`, ligada por padrão): quando a janela de um cliente aparece depois do launch de uma conta, da fila ou do Auto Rejoin, a mesma task que confere o tamanho pelo PID ([launch.md](launch.md#tamanho-da-janela-conferido-pelo-pid)) a põe na **primeira célula livre** (`pick_grid_slot`). Uma célula está ocupada quando o centro de uma janela da grade está dentro dela. **Janelas já abertas não se mexem** — o usuário pode estar jogando nelas. **Grade cheia: dá a volta** — a janela vai para a célula menos ocupada, empate na de menor índice, ou seja, sobrepõe a partir da célula 0. Uma janela por vez (`GRID_PLACEMENT`), para duas janelas da fila não escolherem a mesma célula. Na conta do launch de uma conta, a grade vence a posição salva pelo Watcher. **A [reconexão automática](watcher.md#reconexão-automática) passa pelo mesmo caminho** (`run_reconnect_attempt` → `launch_roblox_windows`): o cliente reaberto cai numa célula dos monitores marcados (`GridMonitors`), não em outro monitor — conferido em 11/10/2026 e travado por `a_reconnected_client_lands_in_the_grid_of_the_ticked_monitors` (`auto_reconnect_tests`).
- **Botão Arrange in grid** (`arrange_windows_grid`): as mesmas células, a i-ésima janela (ordem de cima para baixo, esquerda para direita) na célula i, dando a volta quando há mais janelas que células.
- **Fora da grade, nos dois**: contas com janela própria — exceção de launch com tamanho (`ClientOverrideWindowWidth/Height`) ou tela cheia (`account_keeps_own_window`). Elas não são movidas nem encolhidas e **bloqueiam as células que cobrem** (`slot_overlaps`): na automática essas células contam como ocupadas, e o botão manual só usa as livres (`unblocked_slots`; tudo coberto = grade inteira). Sem isso a alt nascia por cima da principal de 1000x1000 parada em (0,0); os PIDs saem do rastreamento de processos (`grid_excluded_pids`). Também ficam de fora: conta que começa minimizada (não é desminimizada), janela em tela cheia ou maximizada (na automática) e janela minimizada.

- **Menor que o mínimo e sem moldura** (opcionais, desligados): `General.GridAllowSmallWindows` deixa a célula do tamanho pedido (a janela é posta com `SWP_NOSENDCHANGING`, e o Roblox não a aumenta) e `General.GridBorderless` tira a barra de título e a borda das janelas da grade. Só nos clientes que o app abriu; a moldura volta quando a opção desliga ou o app fecha. Ver [performance.md](performance.md#grade-menor-que-o-mínimo-e-sem-moldura-generalgridallowsmallwindows-generalgridborderless).

Testes: `win_grid_slot_tests` (células, primeira livre, volta, célula do tamanho aceito, janelas fora da grade bloqueando), `win_grid_style_tests` (célula pequena, sem moldura), `client_window_plan_tests` (quem entra na grade). Interruptores: `WindowGridCard.test.tsx` e `settingsTabs.test.tsx` (aba Optimization, junto do tamanho de janela global).

### Tela "Choose Game" — chips e abas

No cabeçalho, **Save as preset** abre o editor de presets com as contas da tela (e o Place ID do campo) — [presets.md](presets.md).

Os chips de conta no topo têm um **x** que tira aquela conta do lote sem sair da tela (o lote nunca fica vazio: o x da última conta é desabilitado).

Abas: Favorites, Games, Recent, **Servers** ([server-choice.md](server-choice.md)), Friends ([friends.md](friends.md)) e Follow. Na aba Games, cada jogo tem dois botões: entrar (▶) e **ver servidores**, que leva para a aba Servers já com aquele place.

### Painel de Sessão (`SessionPanel`)

Resolve as dores de quem joga com muitas contas: cancelar entradas no meio do caminho, acompanhar uma operação em lote conta por conta, e achar/fechar uma conta específica sem caçar janela por janela no Windows.

Aparece na página **Session** da barra lateral ([SessionPage.tsx](../../src/components/pages/SessionPage.tsx)), disponível a qualquer momento; o item da barra mostra o contador de clientes rodando. Até 11/10/2026 aparecia também na aba Console da Choose Game, acima do log; a aba virou a página Console, só com o log.

| Seção | Fonte | Ações |
|---|---|---|
| **Joining** | evento `launch-queue` ([multi-launch.md](multi-launch.md#fila-observável-e-cancelamento)) | ✕ por conta (`cancel_account_launch`), "Stop queue" (`stop_launch_queue`) |
| **Make Friends** | evento `friend-link-state` | nenhuma (só acompanhamento) — uma linha por conta com aguardando/processando/amizade feita/erro, o erro **na conta que enviou** o pedido que falhou, marca de conta principal no modo `star`, e o contador "X / Y contas processadas" |
| **Closed** | `get_closed_clients` / evento `closed-clients` ([launch.md](launch.md#cliente-que-fechou-e-entrar-novamente)) | **Rejoin** (o launch normal, para o mesmo jogo e servidor) e **×** por conta — mesma ação da linha da lista de contas ([ClosedClientsSection.tsx](../../src/components/session/ClosedClientsSection.tsx)). Fica entre **Reconnecting** e **In game**, e só com alguém nela. Conta que ainda aparece em **Reconnecting** (inclusive a que desistiu) fica de fora — uma conta, uma linha —, e dispensar a linha dela lá dispensa o aviso de "fechou" junto |
| **In game** | `get_running_instances` (rastreador de PID) | **Focus** (`focus_roblox_window`) e **Close** (`cmd_kill_roblox`) por linha — o Close fica afastado do Focus, atrás de um divisor, e tem **segundo passo na própria linha**: o primeiro clique só arma ("Close?", em vermelho), o segundo fecha; desarma sozinho em 4 s, no Escape ou quando o foco sai. No cabeçalho, **AFK Mode** (abre o Modo AFK com as contas marcadas, ou todas — o Start do Auto Rejoin de lá adota **sem fechar nada**, ver [botting.md](botting.md#modo-afk-a-tela)) e **Close all clients (n)** (sempre **todas as da lista**, mesmo com linhas marcadas); as marcadas fecham por **Close selected (n)** na barra de lote. Os dois perguntam sempre, até para um cliente só: o diálogo lista as contas (as cinco primeiras e "and N more"), marca "(opened from the website)" nas abertas pelo site com uma nota de quantas são, e o botão diz o que faz ("Close 3 clients" — `confirm(..., { confirmLabel, details, note })` em [usePrompt.tsx](../../src/hooks/usePrompt.tsx)). Fecha só as da lista: `closeRobloxClients`, nunca `killAllRobloxProcesses`. Até 11/10/2026 era "Close accounts", que sem marcação fechava todas sem dizer quais, e o Close da linha fechava no primeiro clique, colado no Focus. Só no Windows, os controles da linha são **chips compactos** que abrem um menu pequeno ([ChipMenu.tsx](../../src/components/session/ChipMenu.tsx)): teto de memória (a memória de agora / o limite, âmbar acima dele) e reconexão automática (↻ On/Off; apagado quando segue o padrão, cadeado quando o AutoRelaunch do Nexus a mantém ligada). A informação ao vivo (nome, estado, tempo, motivo da queda) fica com a largura que sobra, e o motivo da queda tem linha própria, que quebra. Com linhas marcadas, os mesmos menus valem para o lote (**Reconnect**, **Memory limit**) — ver [watcher.md](watcher.md#reconexão-automática) |

A lista **In game** é agrupada por jogo e servidor, com **Needs attention** no topo (ver [history.md](history.md#sessão-de-agora-na-página-session)). Cada jogo fica separado do seguinte por um fio; a linha do servidor só tem caixa de seleção (e contagem) quando o jogo tem mais de um servidor — com um só, ela repetiria a do jogo. **Focus** e **Close** da linha ficam neutros e sem borda até o mouse ou o foco chegar (o **Close** fica vermelho aí, e o "Close?" armado continua vermelho cheio); a linha marcada ganha um fundo na cor de destaque. Cada linha tem em cima o nome e o estado curto (**Playing**, **Reconnecting**, a queda com motivo, **Not responding** ou **Not in a game yet**); embaixo a **sessão de agora** — jogo (nome por `useGameIdentity`, ou "Place <id>"), "Public server"/"Private server" quando se sabe e o **tempo em jogo** andando ("12m", "1h 05m"), contado da entrada no jogo atual (o mesmo início do "Playing now" do histórico). Nome comprido é cortado com o texto inteiro no tooltip; cabe em 820 px sem rolagem lateral. Fonte e regras: [history.md](history.md#sessão-de-agora-na-página-session).

Ao lado do nome de cada conta da **In game** aparece, em vermelho, **por que ela caiu** quando o log do Roblox diz — "Disconnected: lost connection", "Kicked: <mensagem do jogo>", "The server shut down", "Disconnected: the account joined somewhere else", "Closed without leaving the game" (o código do Roblox fica no tooltip). O aviso some quando a conta entra num jogo de novo. Janela travada há 30 s (só clientes abertos pelo app) mostra **"Not responding"** em âmbar, no mesmo lugar ([watcher.md](watcher.md#não-respondendo)). O mesmo texto aparece no topo do painel da conta (`SingleSelectSidebar`) e num toast. Vem de `health` em `get_running_instances` e do evento `roblox-client-health` — ver [watcher.md](watcher.md#quedas-lidas-do-log-do-cliente). Só avisa: não fecha nada, inclusive nos clientes abertos pelo site.

Na **In game** também entram os clientes abertos **fora do app** (pelo site, ou antes de o app abrir): os reconhecidos pelo log do Roblox aparecem como qualquer conta, com a marca "Opened outside the app"; os que não deu para reconhecer aparecem como **"Roblox open, account unknown"** (motivo e uma linha do que fazer; o PID no tooltip), com **Show window** (`focus_client_window`) e **Identify** (escolher a conta → `identify_external_client`), sem caixa de seleção, fora da contagem e sem botão de fechar — ver [external-clients.md](external-clients.md).

**Acessibilidade e texto** (crítica da página Session, 11/10/2026):

- **Contraste:** `--panel-muted` deixou de ser a cor dos rótulos com 85% de opacidade (e o `.theme-muted` tirava mais 15%): `readableMuted` ([theme.ts](../../src/theme.ts)) parte da cor dos rótulos e a puxa para a cor do texto do painel até chegar a **4,5:1** sobre o painel e sobre os cartões (`--panel-soft`), em todo tema — preset ou do usuário, claro também; tema que já lê bem fica com a sua cor. Dentro de `theme-modal-scope` o `text-zinc-500` usa essa mesma variável. Tema claro ganha `data-theme-tone="light"` no `<html>`, e as cores de estado em tom 400 (âmbar, vermelho, verde), feitas para fundo escuro, escurecem no CSS. Travado em [themeContrast.test.ts](../../src/themeContrast.test.ts).
- **Leitor de tela:** mudança de estado de uma conta (caiu com o motivo, não responde, reconectando, voltou a jogar) é anunciada numa região `aria-live="polite"`; a contagem regressiva da reconexão é `role="timer"` (não fala a cada segundo). Títulos em ordem: `h1` da página, `h2` das seções, `h3` dos grupos.
- **Alvos:** caixas de seleção com área de 24 px (`SelectBox`), botões de ícone (cancelar, dispensar) de 24 px, chips de 24 px de altura.
- **Texto:** tamanhos com vírgula em pt/es/de ("1,2 GB", `formatMemoryMb(mb, idioma)`); a frase da página sem "Make Friends" em inglês; a chave "Reconnect accounts that drop" daqui diz "na própria linha da lista Em jogo" (é o único interruptor do padrão: Settings › Keep in game só mostra o estado e leva até aqui); o cartão **Recordings** some sem gravação nenhuma; "In game" vazio tem **Choose a game** (com contas marcadas abre a Choose Game; sem, leva à lista de contas); o cliente sem conta diz **"Roblox open, account unknown"** com uma linha explicando o que fazer (o PID foi para o tooltip); "Closing MultiAlt leaves every Roblox client running." fica no cabeçalho do "In game", perto do Close all.

**Marcar contas não mexe nas linhas:** a faixa de lote entrava entre o cabeçalho e a lista e descia tudo (~76 px no primeiro clique). Agora é uma **barra presa ao pé do cartão** (`sticky bottom-0`, `session-bulk-bar`) com "N selected", **Clear selection** e os menus de lote (**Reconnect**, **Memory limit**). **Shift+clique** marca o intervalo desde a última conta clicada, na ordem em que as linhas estão na tela (grupos recolhidos ficam de fora), com o novo estado da clicada — como na lista de contas. A caixa de cada grupo (jogo, servidor, atenção, sem jogo) marca as contas dele.

**Joining** vazio é só o cabeçalho ("Nothing waiting", sem o Stop queue): a caixa vazia empurrava o "In game" para baixo. Embutido em outro lugar, o painel guarda a rolagem própria de cada lista (`contained`, o padrão); a página Session desliga.

Regras: cancelar **nunca** chama `cmd_kill_roblox` (há teste de regressão para isso); os nomes respeitam o mascaramento de `hideUsernames`; linhas terminais (`done`/`failed`/`cancelled`) continuam visíveis até a próxima fila substituir.

A seção **Make Friends** só aparece depois que houve uma execução (`total > 0`): uma seção vazia permanente empurraria o "In game" para baixo. O retrato da última execução fica na tela até a próxima começar — é onde se vê quais contas falharam. Uma conta só conta como "processada" quando **todos os pares dela** acabaram; conta que já era amiga de todo mundo termina de saída, senão ficaria "aguardando" para sempre.

### While you were away (o que aconteceu longe do PC)

Crítica de design aprovada pelo dono (11/10/2026): quem deixa o Modo AFK rodando por horas via quedas, kicks, reconexões, paradas de emergência e falhas de envio só como **toast**, que some em segundos — voltando ao PC, sobrava o "Sends so far: 13". Agora esses fatos ficam numa lista curta.

- **Dados** ([awayDigest.ts](../../src/utils/awayDigest.ts)): anel em memória dos **últimos 50** eventos da sessão do app (nada vai para disco; fechar o app esquece), do mais novo para o mais antigo. Sai dos **mesmos eventos** que já viravam toast, ligados na [store](../../src/store.tsx): `roblox-client-health` (caiu com motivo / voltou a um jogo / não respondendo / voltou a responder), `auto-reconnect` (desistiu ou parou; `reconnected` = relançou e ficou), `afk-status` (falha de envio por conta — anotada **quando começa ou muda de motivo**, não a cada ciclo: a janela fechada falharia de 10 em 10 min), `afk-emergency-stop` e `botting-account-cycle` com `ok: false` (Auto Rejoin que não reabriu). O relançamento **bem-sucedido** do Auto Rejoin fica de fora de propósito: é rotina a cada ciclo e enterraria os 50 lugares em poucas horas. Nenhum Rust novo.
- **Repetição:** o mesmo fato (mesmo tipo, conta e motivo) dentro de **5 s** é descartado (`AWAY_DEDUPE_MS`).
- **Texto na hora de mostrar:** o evento guarda dado, não frase — o idioma e os **nomes ocultos** valem os de agora (`useAccountLabel`). Âmbar = problema, verde = recuperou.
- **Página Session:** o cartão **While you were away** abre a coluna de ajustes ([AwayDigest.tsx](../../src/components/session/AwayDigest.tsx)): os 5 últimos com hora ("14:32"), conta e o que houve (até duas linhas — a coluna é estreita), **Show all (n)** / **Show less**, **Clear**. Sem nada: "Nothing happened yet." (o cartão fica, para a pessoa saber que existe; é também a âncora `session-away` do tutorial).
- **Não lido:** o que chegou depois da última olhada. **Abrir a página Session zera** (efeito na store sobre `activePage`); clicar no cartão também — para o que chegou com a página aberta, que aparece como "N new".
- **Modo AFK:** uma linha acima das abas, só com algo não lido — "3 events while you were away · 14:32 conta · o último fato · **View**". O View vai para a Session (no modal, fecha o modal antes).

### Tema e fontes

1. No startup a store aplica `DEFAULT_THEME` e depois o tema salvo (`get_theme`).
2. `applyThemeCssVariables` ([theme.ts](../../src/theme.ts)) transforma `ThemeData` em variáveis CSS; `normalizeTheme` preenche campos ausentes.
3. Fontes (`font_sans`, `font_mono`): origem `google` (link gerado por `buildGoogleFontsHref`), `local` (arquivo em `RAMThemeFonts/`, resolvido por `resolve_theme_font_asset` e servido com `convertFileSrc`) ou `system`.
4. Página Theme: presets embutidos (`THEME_PRESETS`: Legacy v4, Catppuccin, Studio, Terminal, Jakarta, Plex, Soft, Bubble, Graphite, Ocean, Sunset...), presets do usuário (`get/save/delete_theme_preset` → `RAMThemePresets.json`), import (`import_theme_preset_file`) e export (`export_theme_preset_file`).
5. Salvar → `update_theme` → `RAMTheme.ini`.
6. `sync_windows_navbar_theme` é chamado quando muda `ThemeWindowsNavbar` ou `dark_top_bar`.

### Tamanho da interface

Em monitor de notebook (1920x1080 a 150% no Windows → monitor lógico de 1280x720) cabeçalhos, abas, linhas e espaçamentos comiam a tela; no 2560x1440 do dono a interface estava certa. Settings › App › **Interface size** (`General.InterfaceScale`) resolve com o **zoom nativo do WebView** (`getCurrentWebview().setZoom(fator)`, permissão `core:webview:allow-set-webview-zoom` em [capabilities/default.json](../../src-tauri/capabilities/default.json)). O zoom nativo escala tudo por igual, inclusive as classes em px do Tailwind (`text-[12px]`), que um truque de `rem` no CSS não pegaria.

| Opção (INI) | Zoom |
|---|---|
| `auto` (padrão, também chave ausente ou valor desconhecido) | pela **janela** (cresce) e pelo **monitor** (encolhe), ver abaixo |
| `110` / `100` / `90` / `80` | fixo, ignora janela e monitor |

Quem nunca mexeu fica no automático; quem escolheu um tamanho fixo continua com ele — nada é migrado. Na lista, a opção mostra o zoom que o automático escolheu agora (**"Automatic (170%)"**; `useAppliedUiScale` em [useUiScale.ts](../../src/hooks/useUiScale.ts), publicado a cada `setZoom` bem-sucedido). Com um valor fixo salvo, a opção aparece só como "Automatic".

**Regra do automático** ([uiScale.ts](../../src/uiScale.ts), `resolveUiScale`), duas metades que nunca brigam:

- **Crescer, pela janela** (`growUiScale`, desde 11/10/2026): `min(largura / 2200, altura / 1240)` da área interna da janela em px lógicos, arredondado para o degrau de 5% **mais próximo** e preso entre 100% e 200%. 2200x1240 é o dobro do desenho do app: a tela crescida ainda tem o espaço de uma janela padrão. Exemplos: 4K a 100% maximizada (~3840x2090) → **170%**; 2560x1440 maximizada (~2560x1390) → 110%; 1920x1080 maximizada, janela padrão 1100x700 ou mínima → 100%. Pedido do dono: num monitor 4K a 100% tudo ficava pequeno no canto de cima.
- **Encolher, pelo monitor** (`autoUiScale`): só quando a janela não cresceu. `min(largura / 1440, altura / 800)` do monitor lógico, arredondado **para baixo** em passos de 5% e preso entre 80% e 100%. Exemplos: 2560x1440 → 100%; 1920x1080 a 125% (1536x864) → 100%; 1366x768 → 90%; 1920x1080 a 150% (1280x720) → 85%. Janela acima de 2200x1240 só existe em monitor muito acima de 1440x800, então as duas metades não se cruzam.

- Para **encolher**, o tamanho usado é o **lógico do monitor** em que a janela está: `currentMonitor()` (pixels físicos) dividido pelo `scaleFactor` dele. **Não** é o da janela: a primeira versão encolhia pela janela, e uma janela restaurada no tamanho padrão (1100x700) encolhia a interface para 80% até no monitor de 2560x1440 do dono. Para **crescer**, é a janela, mas só para cima — a janela padrão fica em 100%. O tamanho da janela vem de `getCurrentWindow().innerSize()` (px físicos) dividido por `scaleFactor()`. **Não** usar `window.innerWidth` em nenhuma das duas: está em px CSS, muda com o próprio zoom, e o zoom passaria a realimentar a conta que o decide.
- [useUiScale.ts](../../src/hooks/useUiScale.ts) roda no [App.tsx](../../src/App.tsx) antes dos `return` antecipados, então vale também na tela de senha. Espera as settings chegarem (`store.settings !== null`) para quem escolheu valor fixo não ver o automático piscar na abertura. No automático recalcula em `onMoved` — a janela pode ter ido para outro monitor — e em `onResized` — maximizar/restaurar cresce ou devolve a interface — (debounce de 150 ms); `setZoom` só é chamado quando o fator muda. A exceção é `onScaleChanged` (janela arrastada para monitor com outra escala): em qualquer modo, reaplica mesmo com o mesmo fator, porque a troca de DPI mexe na escala do WebView2 e o zoom não pode ficar para trás sem o hook perceber.
- A escolha vale **na hora**: a página Settings grava pelo `useSettings` dela e a store só relê as settings ao sair da página, então o `AppTab` também dispara o evento `ram-ui-scale` (`announceUiScale`), que o hook escuta até a store recarregar.
- Com zoom, o viewport CSS simplesmente fica maior (janela de 1280 px lógicos a 85% = ~1506 px CSS). `100vh`, os tetos `max-h-[calc(100vh-…)]` dos diálogos, o posicionamento de menus/tooltips por `window.innerWidth` e `clientX` (tudo em px CSS) e o limiar da `NavSidebar` (900 px CSS de layout) continuam coerentes; o mínimo da janela (750x450 lógicos) vira ~937x562 px CSS a 80%. Arrastar pela `TitleBar` usa `startDragging`, que não depende de coordenada.
- Fora do Tauri (testes, navegador) a API não existe: o hook engole o erro e a interface fica em 100%. No harness (`bun run dev:ui`) o `setZoom` é um no-op registrado em `window.__harness.calls()` como `plugin:webview|set_webview_zoom`; a "janela" e o "monitor" do dublê são a própria aba (`innerWidth` x `devicePixelRatio`), lidos só na abertura — para ver o 170% na opção, abra a aba já no tamanho grande.

### Janela grande (layout largo)

Num monitor 4K a 100% (pedido do dono, 11/10/2026) a página Session parava em ~1040 px de lista + 300 px de coluna e o resto ficava vazio. Regra desde então: **listas, cartões e tabelas das páginas ocupam a largura**; teto de largura fica só para texto de leitura (`max-w-[68ch]` da frase do `PageShell`, notas de versão) e formulário (rótulo longe do controle). Travado por [wideLayout.test.ts](../../src/components/pages/wideLayout.test.ts): `max-w-[≥600px]` ou `max-w-3xl`+ em `pages/`, `session/`, `afk-mode/`, `console/` e `accounts/` só com exceção listada e motivo.

| Página | Na janela grande |
|---|---|
| Session | Listas sem teto (`flex-1`). As linhas do **In game** viram 2 colunas com a lista a partir de 72rem (~1150 px) e 3 a partir de 110rem (~1760 px) — container query na própria lista (`@container/ingame`), então o painel estreito e a janela padrão continuam com uma coluna. A coluna de ajustes: 300 px (xl), 340 px (2xl), **640 px em duas colunas de cartões** a partir de 125rem (2000 px), 760 px a partir de 175rem (2800 px). |
| Settings | Formulário: 760 px, 920 px a partir de 2xl. |
| AFK Mode › Recordings | Coluna "When it plays" até 420 px com a área a partir de 80rem (antes 320 px). Clicks e Rejoin já seguiam o contêiner. |
| Theme, Avatars, Scripts, Nexus | A coluna lateral fixa alarga um pouco a partir de 2xl (Theme 340, Avatars 380/320, Scripts 380, Nexus 320 px); o miolo já era `1fr`. |
| Console, lista de contas, What's new | Sem mudança: já ocupavam a largura (What's new é texto de leitura, 880 px centralizado). |

**Breakpoint arbitrário de janela é em rem** (`min-[125rem]:`, não `min-[2000px]:`): o Tailwind 4 não ordena px junto dos `sm:`…`2xl:` (em rem) — a regra em px saía **antes** no CSS e o `xl:w-[300px]` vencia o `min-[2000px]:w-[640px]` (achado no build, 11/10/2026). O `wideLayout.test.ts` recusa `min-[Npx]:`/`max-[Npx]:` em `src/components`.

Com o automático, uma janela 4K maximizada fica em ~2260 px CSS (170%); com 100% fixo, em 3840 px CSS — o layout cobre os dois. Medido no harness (11/10/2026, cenário `client-drops`, 8 contas): 820, 1100, 2300 e 3840 px de largura sem rolagem lateral; em 2300 px, listas com 1400 px e 2 colunas de linhas; em 3840 px, 3 colunas.

### Status bar

Total/filtradas, selecionadas, contas online e em jogo (se `ShowPresence`), contas lançadas pelo app, status do Auto Rejoin/gerador e a linha de `actionStatus` (ver abaixo).

### Feedback de ação: toast vs. `actionStatus`

São **dois canais com papéis diferentes**, e nenhuma mensagem vai nos dois:

| Canal | Significa | Onde aparece | Quem escreve |
| --- | --- | --- | --- |
| `toasts` | "isto **acabou de acontecer**" | pilha no canto inferior direito ([App.tsx](../../src/App.tsx)), 2500 ms | `addToast(frase)` |
| `actionStatus` | "isto **está acontecendo agora**" (substituível) | linha única na `StatusBar`, com o timeout de cada chamada | `setActionStatusMessage(frase, tom, timeoutMs)` |

- `addToast` calcula o tom **uma vez** (`toneFromMessage` em [toastTone.ts](../../src/utils/toastTone.ts)) e o guarda no item da fila junto com um `id` — o `id` é a chave de lista, para que a saída de um toast não remonte os que ficaram. `addToast` **não** escreve em `actionStatus`: escrevia, e depois que a `StatusBar` passou a desenhar `actionStatus` a mesma frase apareceria duas vezes na tela.
- A cor dos dois canais (e do Console de launch) vem do **mesmo** mapa `TONE_STYLES` de [toastTone.ts](../../src/utils/toastTone.ts): `info` neutro (cores do painel), `success` esmeralda, `warn` âmbar, `error` vermelho. Não criar paleta paralela.
- O tom é deduzido do **texto**, porque quase todo call site entrega a frase já traduzida; por isso o catálogo tem de preservar o marcador em cada idioma — contrato travado por [locales.test.ts](../../src/i18n/locales.test.ts).
- Progresso vai para `actionStatus`, nunca para toast: download do Chromium, download/instalação da build do Roblox (`timeoutMs` de 60 s), `Launching account N/M...`, `Settings saved` (evento `ram-action-status` disparado por [useSettings.ts](../../src/hooks/useSettings.ts)) e a falha de ciclo do Auto Rejoin (`warn`).

## Regras de negócio

- O sidebar de detalhes só existe para **uma** conta; com múltiplas seleções as ações ficam na barra inferior/Choose Game.
- Se o único grupo for `Default`, a lista não mostra cabeçalho de grupo.
- Presença é atualizada a cada `max(1, PresenceUpdateRate)` minutos (mínimo 30 s), em lotes de 100 IDs; `0` = offline, `1` = online, `2` = em jogo, `3` = no Studio. O comando `get_presence` manda o cookie de uma conta como "viewer" (a primeira válida, `pick_viewer_cookie`) e cai para a chamada sem cookie se a autenticada falhar — ver [friends.md](friends.md#armadilhas--cuidados).
- Toasts ficam empilhados no canto inferior direito, cada um pintado com o tom da própria mensagem; erros persistentes vão para a faixa vermelha até o usuário fechar.
- Diálogos fecham com Esc (Server List, Choose Game); nas páginas da barra lateral o Esc volta para a lista de contas.
- **Clicar no fundo escurecido fecha o modal só se o botão desceu e subiu no fundo** ([useBackdropClose.ts](../../src/hooks/useBackdropClose.ts)). Com `onClick` cru, apertar dentro de um campo (selecionando o texto) e soltar fora do painel fechava o modal: o navegador dispara o `click` no ancestral comum, que é o fundo. Vale para todos os modais (prompt/confirm, modal genérico, Add Account, Import, Contas novas, Versions, Server List, Account Utils, Account Fields, Missing Assets, Update, AFK Mode). Fundo novo usa `<div {...useBackdropClose(fechar)}>`; uma guarda em `useBackdropClose.test.tsx` falha se um fundo `fixed inset-0 ... bg-black/NN` voltar a ter `onClick`.
- **Send feedback** (rodapé da barra lateral, [FeedbackDialog.tsx](../../src/components/dialogs/FeedbackDialog.tsx)): duas opções, "Report a problem" e "Suggest an idea", que abrem no navegador os formulários de [.github/ISSUE_TEMPLATE/](../../.github/ISSUE_TEMPLATE/) (`open_feedback_form` em [services.rs](../../src-tauri/src/commands/services.rs)). **O app não envia nada**: a pessoa escreve e envia no GitHub, com a conta dela. O frontend manda só o tipo (`bug`/`idea`); o endereço sai de uma lista fechada no Rust e não pode ter `&`, porque vai pelo `cmd /C start` (travado em `services_command_tests`). Os formulários só valem depois de chegarem na `main` (o GitHub lê os modelos da branch padrão); mudar só eles não gera release (`paths-ignore` do release-v4.yml). "Report a problem" abre um segundo passo com a caixa **"Add a diagnostic summary"** (desmarcada): marcada, mostra a prévia do resumo **anonimizado** e o botão de copiar; o resumo só sai do app pela área de transferência, colado pela pessoa — ver [support.md](support.md#reportar-problema-com-resumo-anonimizado).
- **Estado da conta com cor e ícone** ([StatusBadge.tsx](../../src/components/accounts/StatusBadge.tsx)): as bolinhas da linha de conta e da legenda da barra de status têm um ícone por estado (✕ inválida, relógio sem uso 20d+, foguete iniciada, wi-fi online, controle em jogo, martelo Studio), para quem não distingue as cores (pedido do dono, 08/10/2026). Na legenda o selo é decorativo (`aria-hidden`), porque o nome está escrito ao lado.
- **Telas pequenas** (monitor comum, ou o mínimo da janela, 750x450): todo conteúdo tem que ter rolagem até ele. A área das abas da Choose Game é `overflow-y-auto` (era `overflow-hidden`, e a aba Follow, sem rolagem própria, cortava a parte de baixo); todo diálogo tem `max-h-[...100vh...]` no painel. Travado em [smallScreen.test.ts](../../src/components/layout/smallScreen.test.ts); para procurar corte novo, `bun run ui:audit` (ver [development.md](../development.md#telas-pequenas)).
- Export de tema grava `<nome>.ram-theme.json` na **pasta de dados do usuário** (`get_runtime_data_dir()` — a do exe só no modo portátil); se o tema usa fontes locais, grava `<nome>.ram-theme.zip` incluindo os arquivos das fontes.
- Fontes importadas aceitam só `.ttf`, `.otf`, `.woff`, `.woff2` e são deduplicadas pelo SHA-256 do conteúdo.

## Configurações relacionadas

| Seção.Chave | Efeito na UI |
|---|---|
| `General.HideUsernames`, `HiddenNameLetters`, `ShowAvatarsWhenHidden` | Mascaramento de nomes e fotos em todas as telas (ver "Nomes ocultos" acima; helper `utils/accountName.ts`). |
| `General.ShowPresence`, `PresenceUpdateRate` | Pontos de presença e contagem na status bar. |
| `General.DisableAgingAlert`, `DisableImages` | Indicador de idade / avatares. |
| `General.MinimizeToTray` | Botão fechar da TitleBar esconde na bandeja. |
| `General.ThemeWindowsNavbar` | Barra nativa segue o tema. |
| `General.RestrictedBackgroundStyle` | Fundo da tela de senha. |
| `General.InterfaceScale` | Tamanho da interface: `auto` (padrão; cresce com a janela grande, encolhe em monitor pequeno) ou `110`/`100`/`90`/`80` (ver "Tamanho da interface"). |
| `General.GridGap`, `GridMonitors` | Arranjo em grade (cartão Window grid da página Console). |
| `General.AutoArrangeGrid` | Grade automática no launch (cartão Window grid da página Console e Settings > Optimization). |
| `General.FirstRunWalkthroughState` | Exibição do walkthrough inicial. |

## Armadilhas / cuidados

- **Ação em lote vai para a [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx) ou para a [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx)** — não existe mais painel lateral de multi-seleção. Havia um (`MultiSelectSidebar.tsx`, 517 linhas) que ninguém conseguia abrir desde que o painel virou de uma conta só; foi apagado depois de ser editado três vezes por engano. A [DetailSidebar.tsx](../../src/components/accounts/DetailSidebar.tsx) continua sendo só para **uma** conta.
- `store.tsx` é um Context único com ~3000 linhas (3026 em 27/09/2026); qualquer mudança de estado re-renderiza todos os consumidores de `useStore()`.
- Estado de UI como `placeId`/`jobId` é gravado no INI a **cada** mudança (`SavedPlaceId`, `SavedJobId`, `SavedLaunchData`).
- `ScriptsPage`, as demais páginas que eram modais e os diálogos ficam sempre montados; efeitos deles (ex.: auto-start de scripts) rodam mesmo com a página/diálogo fechado. Não trocar por `{active && <Page/>}`: os scripts em execução morreriam ao sair da página.
- Abrir uma página a partir de um diálogo (Scripts pela Server List, Settings pelo gerador) **fecha o diálogo antes**: aberto, ele cobriria a página e ficaria com o Escape.
- Por a janela não ter decorações nativas, os controles de minimizar/maximizar/fechar são responsabilidade de `TitleBar` e `ModalWindowControls` — ao criar um novo overlay de tela cheia, inclua-o em `anyModalOpen` em [App.tsx](../../src/App.tsx) para os controles continuarem acessíveis. As telas bloqueantes (`PasswordScreen`, `EncryptionSetupScreen`) trocam a árvore inteira, sem `TitleBar`, e por isso cada uma renderiza a sua `ModalWindowControls visible`. A de criptografia não tinha: na primeira execução (sem Cancel, e com o Esc sem efeito) só se saía concluindo ou com Alt+F4 — e se o Continue falhasse, o usuário ficava preso. Aberta pelas Settings, o Esc agora faz o mesmo que o Cancel. **Mover a janela** também é responsabilidade dos dois: a `TitleBar` é o único ponto que chama `startDragging`, e ela some nas telas bloqueantes e fica coberta pelo fundo de qualquer diálogo — por isso a `ModalWindowControls` tem uma **alça** (ícone ⠿, "Move window") antes de minimizar; só o botão esquerdo arrasta. Com a alça a pílula ocupa 142 px a partir da borda direita, e a `VaultKeyBanner` reserva `pr-40` (160 px) para o texto não passar por baixo dela (medido no harness, 27/09/2026).
