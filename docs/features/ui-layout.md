# Interface: shell, temas, lista de contas e ações

## Objetivo

Descrever como a janela principal é montada, como temas e fontes são aplicados, e como o usuário seleciona contas e dispara ações (sidebar de conta única, barra de ações em lote, tela "Choose Game").

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Raiz e roteamento de telas/diálogos | [App.tsx](../../src/App.tsx) |
| Estado global | [store.tsx](../../src/store.tsx) |
| Chrome da janela | [TitleBar.tsx](../../src/components/layout/TitleBar.tsx), [ModalWindowControls.tsx](../../src/components/layout/ModalWindowControls.tsx), [UpdateBanner.tsx](../../src/components/layout/UpdateBanner.tsx), [Toolbar.tsx](../../src/components/layout/Toolbar.tsx), [StatusBar.tsx](../../src/components/layout/StatusBar.tsx) |
| Telas bloqueantes | [PasswordScreen.tsx](../../src/components/layout/PasswordScreen.tsx), [EncryptionSetupScreen.tsx](../../src/components/layout/EncryptionSetupScreen.tsx), [FirstRunWalkthrough.tsx](../../src/components/layout/FirstRunWalkthrough.tsx) |
| Lista de contas | [AccountList.tsx](../../src/components/accounts/AccountList.tsx), [GroupSection.tsx](../../src/components/accounts/GroupSection.tsx), [AccountRow.tsx](../../src/components/accounts/AccountRow.tsx), [AccountChip.tsx](../../src/components/accounts/AccountChip.tsx) |
| Sidebar (conta única) | [DetailSidebar.tsx](../../src/components/accounts/DetailSidebar.tsx) → [SingleSelectSidebar.tsx](../../src/components/accounts/SingleSelectSidebar.tsx), [SidebarSection.tsx](../../src/components/accounts/SidebarSection.tsx) |
| Ações em lote | [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx) |
| Sidebar multi-seleção (não usada) | [MultiSelectSidebar.tsx](../../src/components/accounts/MultiSelectSidebar.tsx) |
| Tela de escolha de jogo / launch em lote | [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) |
| Menu de contexto | [ContextMenu.tsx](../../src/components/menus/ContextMenu.tsx), [MenuItemView.tsx](../../src/components/menus/MenuItemView.tsx) |
| Diálogos | [src/components/dialogs/](../../src/components/dialogs), [SettingsDialog.tsx](../../src/components/settings/SettingsDialog.tsx), [ServerListDialog.tsx](../../src/components/server-list/ServerListDialog.tsx) |
| Tema | [theme.ts](../../src/theme.ts), [themeFonts.ts](../../src/themeFonts.ts), [fontPresets.ts](../../src/fontPresets.ts), [ThemeEditorDialog.tsx](../../src/components/dialogs/ThemeEditorDialog.tsx), backend [theme.rs](../../src-tauri/src/data/settings/theme.rs) e [presets.rs](../../src-tauri/src/data/settings/presets.rs) |
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
   - `UpdateBanner`, `Toolbar`;
   - faixa de erro (`store.error`), com botão "Close Roblox" quando o erro menciona falha no multi-Roblox;
   - corpo: `ChooseGameScreen` **ou** `AccountList` + `DetailSidebar` (só com `sidebarOpen` e exatamente 1 conta selecionada);
   - `BottomActionBar` quando há seleção e a Choose Game não está aberta;
   - `StatusBar`, `ContextMenu`, toasts, todos os diálogos e `IsolationProgressOverlay`;
   - modal genérico `store.modal` (título + `<pre>`), usado para mostrar textos longos.

### Toolbar

Busca (filtra por username, alias, descrição, grupo), selecionar tudo, ocultar nomes, abrir sidebar e menu **Add**: Quick Add (cookie ou username), Browser Login, User:Pass, Import Cookie, Import Old AccountData, Generator, Versions. Botões para Theme Editor, Nexus (se `ENABLE_NEXUS`), Scripts e Settings.

### Lista de contas

- Contas agrupadas por `Group` (ver regras em [accounts.md](accounts.md)); grupos podem ser colapsados e têm menu (ordenar alfabeticamente, copiar, alternar visibilidade).
- **Seleção** (`handleSelect` em [store.tsx](../../src/store.tsx)): clique simples seleciona só a conta; Ctrl/Alt/Cmd alterna; Shift seleciona o intervalo desde o último clique na ordem visível (grupos colapsados ficam fora); Shift+Ctrl soma o intervalo. Arrastar no fundo da lista faz seleção por retângulo (inclui membros de grupos colapsados quando o retângulo cobre o cabeçalho).
- **Arrastar** pela alça (ícone de grip à esquerda da linha, não a linha inteira — o corpo da linha é usado pela seleção por retângulo) para outro grupo move as contas selecionadas para esse grupo; soltar sobre outra linha reordena (`reorder_accounts`). `onDragEnd` na alça limpa `dragState`, então um arrasto cancelado (Esc / soltar fora) não deixa estado velho que um drop posterior de texto de cookie usaria.
- **Soltar texto** na lista: extrai todos os cookies `_|WARNING:-DO-NOT-SHARE-THIS...|<token>` e adiciona cada um.
- Cada linha mostra avatar (48×48, `batched_get_avatar_headshots`), nome/alias, tempo desde `LastUse`, pontos de status e botão "Join".

### Sidebar de conta única (`SingleSelectSidebar`)

Alias, descrição, validade, presença, override de versão do Roblox (campo `RobloxVersion`, com "Latest installed" e "Manage versions..."), Join Group, Browser (abre navegador logado), Server List e Utilities (`AccountUtilsDialog`).

### Barra de ações em lote (`BottomActionBar`)

Aparece com ≥ 1 conta selecionada:

| Ação | Comportamento |
|---|---|
| Deselect | limpa seleção |
| Account settings | alterna o sidebar |
| Refresh Cookies | `refresh_cookie` em série, 2 s entre contas |
| Copy cookies | copia cookies (um por linha) |
| Make Friends | `make_selected_friends` modo `mesh` ou `star`; confirma acima de 30 pedidos; progresso via `friend-link-progress` |
| Move to Group / New group | `moveToGroup` |
| Restart launched clients | só contas lançadas pelo app |
| Botting | abre diálogo ou adiciona contas ao botting ativo |
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
| Follow | `lookup_user` + `get_presence`; se o alvo não estiver em jogo (`presence < 2`) pede confirmação; chama `launch_roblox` com `followUser: true` para cada conta, 3 s entre elas. Atalhos para Server List, Utilities, Botting e Scripts. |
| Console | log ao vivo do evento `launch-log` (auto-scroll, limpar) + controles de grade: `list_display_monitors` e `arrange_windows_grid(monitorIndices, gap)` para ladrilhar janelas do Roblox. |

`launchAll` (hook `useLauncher`): confirma contas online, muda para a aba Console, grava `placeId`/`jobId` na store e chama `joinServer` (1 conta) ou `launchMultiple` (várias) passando o alvo explicitamente. O registro nos recentes é feito pela **store**, só quando o `invoke` de launch retorna sucesso (a `ChooseGameScreen` não registra mais por conta própria).

Controles de grade (Console): o campo **Gap** (0–200 px) é editado como texto e só é aplicado/persistido (`General.GridGap`) ao perder o foco ou com Enter (`commitGap`); valor inválido volta ao anterior.

### Tema e fontes

1. No startup a store aplica `DEFAULT_THEME` e depois o tema salvo (`get_theme`).
2. `applyThemeCssVariables` ([theme.ts](../../src/theme.ts)) transforma `ThemeData` em variáveis CSS; `normalizeTheme` preenche campos ausentes.
3. Fontes (`font_sans`, `font_mono`): origem `google` (link gerado por `buildGoogleFontsHref`), `local` (arquivo em `RAMThemeFonts/`, resolvido por `resolve_theme_font_asset` e servido com `convertFileSrc`) ou `system`.
4. Theme Editor: presets embutidos (`THEME_PRESETS`: Legacy v4, Catppuccin, Studio, Terminal, Jakarta, Plex, Soft, Bubble, Graphite, Ocean, Sunset...), presets do usuário (`get/save/delete_theme_preset` → `RAMThemePresets.json`), import (`import_theme_preset_file`) e export (`export_theme_preset_file`).
5. Salvar → `update_theme` → `RAMTheme.ini`.
6. `sync_windows_navbar_theme` é chamado quando muda `ThemeWindowsNavbar` ou `dark_top_bar`.

### Status bar

Total/filtradas, selecionadas, contas online e em jogo (se `ShowPresence`), contas lançadas pelo app e status do Botting.

## Regras de negócio

- O sidebar de detalhes só existe para **uma** conta; com múltiplas seleções as ações ficam na barra inferior/Choose Game.
- Se o único grupo for `Default`, a lista não mostra cabeçalho de grupo.
- Presença é atualizada a cada `max(1, PresenceUpdateRate)` minutos (mínimo 30 s), em lotes de 100 IDs; `0` = offline, `1` = online, `2` = em jogo, `3` = no Studio.
- Toasts ficam empilhados no canto inferior direito; erros persistentes vão para a faixa vermelha até o usuário fechar.
- Diálogos fecham com Esc (Settings, Server List, Choose Game).
- Export de tema grava `<nome>.ram-theme.json` na pasta do exe; se o tema usa fontes locais, grava `<nome>.ram-theme.zip` incluindo os arquivos das fontes.
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
| `General.GridGap`, `GridMonitors` | Arranjo em grade (Console). |
| `General.FirstRunWalkthroughState` | Exibição do walkthrough inicial. |

## Armadilhas / cuidados

- `MultiSelectSidebar.tsx` não é importado em nenhum lugar (código morto); [DetailSidebar.tsx](../../src/components/accounts/DetailSidebar.tsx) documenta que ações multi-conta vivem na `BottomActionBar` e na `ChooseGameScreen`. Mudanças de ações em lote devem ir para esses dois.
- `store.tsx` é um Context único com ~2100 linhas; qualquer mudança de estado re-renderiza todos os consumidores de `useStore()`.
- Estado de UI como `placeId`/`jobId` é gravado no INI a **cada** mudança (`SavedPlaceId`, `SavedJobId`, `SavedLaunchData`).
- `ScriptsDialog` e demais diálogos ficam sempre montados; efeitos deles (ex.: auto-start de scripts) rodam mesmo com o diálogo fechado.
- Por a janela não ter decorações nativas, os controles de minimizar/maximizar/fechar são responsabilidade de `TitleBar` e `ModalWindowControls` — ao criar um novo overlay de tela cheia, inclua-o em `anyModalOpen` em [App.tsx](../../src/App.tsx) para os controles continuarem acessíveis.
