# Interface: shell, temas, lista de contas e ações

## Objetivo

Descrever como a janela principal é montada, como temas e fontes são aplicados, e como o usuário seleciona contas e dispara ações (sidebar de conta única, barra de ações em lote, tela "Choose Game").

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Raiz e roteamento de telas/diálogos | [App.tsx](../../src/App.tsx) |
| Estado global | [store.tsx](../../src/store.tsx) |
| Chrome da janela | [TitleBar.tsx](../../src/components/layout/TitleBar.tsx), [ModalWindowControls.tsx](../../src/components/layout/ModalWindowControls.tsx), [UpdateBanner.tsx](../../src/components/layout/UpdateBanner.tsx), [NavSidebar.tsx](../../src/components/layout/NavSidebar.tsx) (barra lateral), [Toolbar.tsx](../../src/components/layout/Toolbar.tsx), [StatusBar.tsx](../../src/components/layout/StatusBar.tsx) |
| Páginas | [src/components/pages/](../../src/components/pages): `PageShell` (casca comum), `SessionPage`, `AfkPage`, `AvatarsPage` (+ `pages/avatars/`), `ScriptsPage`, `ThemePage`, `NexusPage`, `SettingsPage` |
| Telas bloqueantes | [PasswordScreen.tsx](../../src/components/layout/PasswordScreen.tsx), [EncryptionSetupScreen.tsx](../../src/components/layout/EncryptionSetupScreen.tsx), [FirstRunWalkthrough.tsx](../../src/components/layout/FirstRunWalkthrough.tsx) |
| Lista de contas | [AccountList.tsx](../../src/components/accounts/AccountList.tsx), [GroupSection.tsx](../../src/components/accounts/GroupSection.tsx), [AccountRow.tsx](../../src/components/accounts/AccountRow.tsx), [AccountChip.tsx](../../src/components/accounts/AccountChip.tsx) |
| Sidebar (conta única) | [DetailSidebar.tsx](../../src/components/accounts/DetailSidebar.tsx) → [SingleSelectSidebar.tsx](../../src/components/accounts/SingleSelectSidebar.tsx), [SidebarSection.tsx](../../src/components/accounts/SidebarSection.tsx) |
| Ações em lote | [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx) |
| Tela de escolha de jogo / launch em lote | [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) |
| Menu de contexto | [ContextMenu.tsx](../../src/components/menus/ContextMenu.tsx), [MenuItemView.tsx](../../src/components/menus/MenuItemView.tsx) |
| Diálogos | [src/components/dialogs/](../../src/components/dialogs), [ServerListDialog.tsx](../../src/components/server-list/ServerListDialog.tsx) |
| Tema | [theme.ts](../../src/theme.ts), [themeFonts.ts](../../src/themeFonts.ts), [fontPresets.ts](../../src/fontPresets.ts), [ThemePage.tsx](../../src/components/pages/ThemePage.tsx), backend [theme.rs](../../src-tauri/src/data/settings/theme.rs) e [presets.rs](../../src-tauri/src/data/settings/presets.rs) |
| Componentes genéricos | [src/components/ui/](../../src/components/ui) |
| Hooks de UI | [usePrompt.tsx](../../src/hooks/usePrompt.tsx) (`prompt`/`confirm` assíncronos), [useModalClose.ts](../../src/hooks/useModalClose.ts) (animação de fechar), [useJoinOnlineWarning.ts](../../src/hooks/useJoinOnlineWarning.ts) |

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
| **Session** | `SessionPage`: Painel de Sessão + resumo ao lado (clientes abertos, entrando, em jogo; quem mantém as contas no jogo) | contador de clientes rodando (`launchedByProgram`) |
| **AFK Mode** | `AfkPage` → `AfkModeView variant="page"` (Auto Rejoin + AFK) | ponto verde "On" com AFK ou Auto Rejoin ligado |
| **Avatars** | `AvatarsPage` (abas Montar/Distribuir sob o cabeçalho) | |
| **Scripts** | `ScriptsPage` (lista + editor) | |
| **Theme** | `ThemePage`: presets, todas as categorias em cartões, prévia fixa ao lado | |
| **Nexus** | `NexusPage` (só com `ENABLE_NEXUS`) | Start/Stop no cabeçalho |
| **Settings** | `SettingsPage`: seções numa lista vertical à esquerda, conteúdo com teto de largura | |
| **Help** (rodapé) | reabre o walkthrough, que leva de volta à lista de contas | |

- **Estado:** `store.activePage` (`AppPage`) + `setActivePage`. Não há roteador. Os setters antigos (`setSettingsOpen`, `setThemeEditorOpen`, `setAvatarsDialogOpen`, `setSessionDialogOpen`, `setNexusOpen`, `setScriptsOpen`) continuam e viram navegação: `true` abre a página; `false` volta para Accounts **só se** aquela página for a aberta. Os booleanos `settingsOpen`/`scriptsOpen`/... saíram — quem quer saber lê `activePage`.
- **Casca comum:** [PageShell](../../src/components/pages/PageShell.tsx) — cabeçalho com título (`h1`), uma frase do que a página faz e as ações dela; leva `theme-modal-scope`, que traduz as classes `zinc-*` herdadas dos modais para as variáveis do tema.
- **Escape** volta para Accounts, pela pilha de Escape (`useEscapeStack`): popover ou diálogo aberto por cima consome antes; Escape digitado num campo não sai da página.
- **Montagem:** as páginas que eram modais ficam montadas o tempo todo e só aparecem quando ativas, como os modais ficavam: o `ScriptsPage` mantém os workers dos scripts em execução e o auto-start, o `AvatarsPage` o ouvinte do lote (`avatar-batch-state`), o `SettingsPage` recarrega ao entrar e chama `reloadSettings` ao sair. A página de contas (e a Choose Game) desmonta ao trocar de página: a Choose Game volta aberta (`chooseGameOpen` continua valendo), mas na aba inicial.
- **Theme:** sair da página sem salvar (barra lateral, Escape, walkthrough) devolve o tema salvo — o que o Escape/Cancel do modal faziam. "Discard changes" faz o mesmo sem sair; "Save" salva e fica.
- **Barra de título:** página não é modal — não entra no `anyModalOpen`, e os botões de janela continuam na `TitleBar`.
- **Recolher:** o botão no rodapé deixa só os ícones (tooltip à direita, só nesse modo). A escolha fica em `localStorage` (`ram_nav_collapsed`, com `try/catch`); abaixo de 900 px de largura a barra recolhe sozinha e o botão some.
- **Walkthrough:** os passos apontam `data-tour="nav-session"`/`"nav-settings"` (ou a página, se aberta: `session-page`/`settings-page`); os passos da lista de contas trazem a página de contas de volta.

### Toolbar

Só na página de contas, e só com o que age na lista: busca (filtra por username, alias, descrição, grupo), selecionar tudo, ocultar nomes, abrir o painel da conta e menu **Add**: Quick Add (cookie ou username), Browser Login, User:Pass Login, Import Cookie, Import Old Account Data, **Create Accounts** (cadastro no navegador — [account-creation.md](account-creation.md)), Account Generator e Roblox Versions.

### Lista de contas

- Contas agrupadas por `Group` (ver regras em [accounts.md](accounts.md)); grupos podem ser colapsados e têm menu (ordenar alfabeticamente, copiar, alternar visibilidade).
- **Seleção** (`handleSelect` em [store.tsx](../../src/store.tsx)): clique simples seleciona só a conta; Ctrl/Alt/Cmd alterna; Shift seleciona o intervalo desde o último clique na ordem visível (grupos colapsados ficam fora); Shift+Ctrl soma o intervalo. Arrastar no fundo da lista faz seleção por retângulo (inclui membros de grupos colapsados quando o retângulo cobre o cabeçalho).
- **Arrastar** pela alça (ícone de grip à esquerda da linha, não a linha inteira — o corpo da linha é usado pela seleção por retângulo) para outro grupo move as contas selecionadas para esse grupo; soltar sobre outra linha reordena (`reorder_accounts`). `onDragEnd` na alça limpa `dragState`, então um arrasto cancelado (Esc / soltar fora) não deixa estado velho que um drop posterior de texto de cookie usaria.
- **Soltar texto** na lista: extrai todos os cookies `_|WARNING:-DO-NOT-SHARE-THIS...|<token>` e adiciona cada um.
- Cada linha mostra avatar (48×48, `batched_get_avatar_headshots`), nome/alias, tempo desde `LastUse`, pontos de status e botão "Join".

### Sidebar de conta única (`SingleSelectSidebar`)

Alias, descrição, validade, presença, **Roblox Version (all accounts)** — grava a `Versions.DefaultVersion` **global**, não o campo `RobloxVersion` da conta (com "Latest installed" e "Manage versions...", e só aparece com alguma versão no catálogo) —, Join Group, Browser (abre navegador logado), Server List e Utilities (`AccountUtilsDialog`). A versão **por conta** existe no backend (`RobloxVersion`, `versions_set_account_override`), mas nenhuma tela a define — ver [roblox-versions.md](roblox-versions.md).

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
| Console | **histórico geral das ações** (evento `launch-log`, auto-scroll, limpar): launch, Auto Rejoin e Watcher, cada linha com a origem (`step`) numa coluna de largura fixa — sem ela, `[watcher]` e `[rejoin-retry]` empurram o nome da conta para colunas diferentes. Linha de sessão (início/fim do Auto Rejoin) vem com `userId` nulo e aparece como `—`. O Painel de Sessão fica acima do log. |
| Windows | controles de grade: `list_display_monitors` e `arrange_windows_grid(monitorIndices, gap)` — organiza nos monitores as janelas do Roblox que já estão abertas. |

`launchAll` (hook `useLauncher`): confirma contas online, muda para a aba Console, grava `placeId`/`jobId` na store e chama `joinServer` (1 conta) ou `launchMultiple` (várias) passando o alvo explicitamente. O registro nos recentes é feito pela **store**, só quando o `invoke` de launch retorna sucesso (a `ChooseGameScreen` não registra mais por conta própria).

Controles de grade (aba **Windows**): o campo **Gap** (0–200 px) é editado como texto e só é aplicado/persistido (`General.GridGap`) ao perder o foco ou com Enter (`commitGap`); valor inválido volta ao anterior.

### Tela "Choose Game" — chips e abas

Os chips de conta no topo têm um **x** que tira aquela conta do lote sem sair da tela (o lote nunca fica vazio: o x da última conta é desabilitado).

Abas: Favorites, Games, Recent, **Servers** ([server-choice.md](server-choice.md)), Friends ([friends.md](friends.md)), Follow, Console e **Windows**. Na aba Games, cada jogo tem dois botões: entrar (▶) e **ver servidores**, que leva para a aba Servers já com aquele place.

### Painel de Sessão (`SessionPanel`)

Resolve as dores de quem joga com muitas contas: cancelar entradas no meio do caminho, acompanhar uma operação em lote conta por conta, e achar/fechar uma conta específica sem caçar janela por janela no Windows.

Aparece em dois lugares, com o mesmo estado vindo do store:

1. na aba **Console** do Choose Game, acima do log ([ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx));
2. na página **Session** da barra lateral ([SessionPage.tsx](../../src/components/pages/SessionPage.tsx)), disponível a qualquer momento; o item da barra mostra o contador de clientes rodando.

| Seção | Fonte | Ações |
|---|---|---|
| **Joining** | evento `launch-queue` ([multi-launch.md](multi-launch.md#fila-observável-e-cancelamento)) | ✕ por conta (`cancel_account_launch`), "Stop queue" (`stop_launch_queue`) |
| **Make Friends** | evento `friend-link-state` | nenhuma (só acompanhamento) — uma linha por conta com aguardando/processando/amizade feita/erro, o erro **na conta que enviou** o pedido que falhou, marca de conta principal no modo `star`, e o contador "X / Y contas processadas" |
| **In game** | `get_running_instances` (rastreador de PID) | **Focus** (`focus_roblox_window`), **Close** (`cmd_kill_roblox`), **Auto Rejoin** (adota as contas em jogo no ciclo **sem fechar nada** — ver [botting.md](botting.md)), seleção múltipla com uma confirmação só |

Regras: cancelar **nunca** chama `cmd_kill_roblox` (há teste de regressão para isso); os nomes respeitam o mascaramento de `hideUsernames`; linhas terminais (`done`/`failed`/`cancelled`) continuam visíveis até a próxima fila substituir.

A seção **Make Friends** só aparece depois que houve uma execução (`total > 0`): uma seção vazia permanente roubaria altura de um painel que já tem teto de 45% na aba Console. O retrato da última execução fica na tela até a próxima começar — é onde se vê quais contas falharam. Uma conta só conta como "processada" quando **todos os pares dela** acabaram; conta que já era amiga de todo mundo termina de saída, senão ficaria "aguardando" para sempre.

### Tema e fontes

1. No startup a store aplica `DEFAULT_THEME` e depois o tema salvo (`get_theme`).
2. `applyThemeCssVariables` ([theme.ts](../../src/theme.ts)) transforma `ThemeData` em variáveis CSS; `normalizeTheme` preenche campos ausentes.
3. Fontes (`font_sans`, `font_mono`): origem `google` (link gerado por `buildGoogleFontsHref`), `local` (arquivo em `RAMThemeFonts/`, resolvido por `resolve_theme_font_asset` e servido com `convertFileSrc`) ou `system`.
4. Página Theme: presets embutidos (`THEME_PRESETS`: Legacy v4, Catppuccin, Studio, Terminal, Jakarta, Plex, Soft, Bubble, Graphite, Ocean, Sunset...), presets do usuário (`get/save/delete_theme_preset` → `RAMThemePresets.json`), import (`import_theme_preset_file`) e export (`export_theme_preset_file`).
5. Salvar → `update_theme` → `RAMTheme.ini`.
6. `sync_windows_navbar_theme` é chamado quando muda `ThemeWindowsNavbar` ou `dark_top_bar`.

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
- Export de tema grava `<nome>.ram-theme.json` na **pasta de dados do usuário** (`get_runtime_data_dir()` — a do exe só no modo portátil); se o tema usa fontes locais, grava `<nome>.ram-theme.zip` incluindo os arquivos das fontes.
- Fontes importadas aceitam só `.ttf`, `.otf`, `.woff`, `.woff2` e são deduplicadas pelo SHA-256 do conteúdo.

## Configurações relacionadas

| Seção.Chave | Efeito na UI |
|---|---|
| `General.HideUsernames`, `HiddenNameLetters`, `ShowAvatarsWhenHidden`, `HideRobuxWhenHidden` | Mascaramento de nomes (Choose Game e lista usam `maskName`). |
| `General.ShowPresence`, `PresenceUpdateRate` | Pontos de presença e contagem na status bar. |
| `General.DisableAgingAlert`, `DisableImages` | Indicador de idade / avatares. |
| `General.MinimizeToTray` | Botão fechar da TitleBar esconde na bandeja. |
| `General.ThemeWindowsNavbar` | Barra nativa segue o tema. |
| `General.RestrictedBackgroundStyle` | Fundo da tela de senha. |
| `General.GridGap`, `GridMonitors` | Arranjo em grade (aba Windows da Choose Game). |
| `General.FirstRunWalkthroughState` | Exibição do walkthrough inicial. |

## Armadilhas / cuidados

- **Ação em lote vai para a [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx) ou para a [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx)** — não existe mais painel lateral de multi-seleção. Havia um (`MultiSelectSidebar.tsx`, 517 linhas) que ninguém conseguia abrir desde que o painel virou de uma conta só; foi apagado depois de ser editado três vezes por engano. A [DetailSidebar.tsx](../../src/components/accounts/DetailSidebar.tsx) continua sendo só para **uma** conta.
- `store.tsx` é um Context único com ~3000 linhas (3026 em 27/09/2026); qualquer mudança de estado re-renderiza todos os consumidores de `useStore()`.
- Estado de UI como `placeId`/`jobId` é gravado no INI a **cada** mudança (`SavedPlaceId`, `SavedJobId`, `SavedLaunchData`).
- `ScriptsPage`, as demais páginas que eram modais e os diálogos ficam sempre montados; efeitos deles (ex.: auto-start de scripts) rodam mesmo com a página/diálogo fechado. Não trocar por `{active && <Page/>}`: os scripts em execução morreriam ao sair da página.
- Abrir uma página a partir de um diálogo (Scripts pela Server List, Settings pelo gerador) **fecha o diálogo antes**: aberto, ele cobriria a página e ficaria com o Escape.
- Por a janela não ter decorações nativas, os controles de minimizar/maximizar/fechar são responsabilidade de `TitleBar` e `ModalWindowControls` — ao criar um novo overlay de tela cheia, inclua-o em `anyModalOpen` em [App.tsx](../../src/App.tsx) para os controles continuarem acessíveis. As telas bloqueantes (`PasswordScreen`, `EncryptionSetupScreen`) trocam a árvore inteira, sem `TitleBar`, e por isso cada uma renderiza a sua `ModalWindowControls visible`. A de criptografia não tinha: na primeira execução (sem Cancel, e com o Esc sem efeito) só se saía concluindo ou com Alt+F4 — e se o Continue falhasse, o usuário ficava preso. Aberta pelas Settings, o Esc agora faz o mesmo que o Cancel. **Mover a janela** também é responsabilidade dos dois: a `TitleBar` é o único ponto que chama `startDragging`, e ela some nas telas bloqueantes e fica coberta pelo fundo de qualquer diálogo — por isso a `ModalWindowControls` tem uma **alça** (ícone ⠿, "Move window") antes de minimizar; só o botão esquerdo arrasta. Com a alça a pílula ocupa 142 px a partir da borda direita, e a `VaultKeyBanner` reserva `pr-40` (160 px) para o texto não passar por baixo dela (medido no harness, 27/09/2026).
