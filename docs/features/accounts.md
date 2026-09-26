# Contas

## Objetivo

Armazenar e gerenciar as contas Roblox (alts) do usuário: sessão (cookie), metadados (alias, descrição, grupo, campos livres), ordem na lista e proteção em disco do arquivo `AccountData.json`. Também expõe as ações por conta que falam com a API do Roblox (amizade, bloqueio, avatar, privacidade, senha/e-mail etc.).

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Modelo `Account` (serde, compatível com RAM v3) | [data/accounts/model.rs](../../src-tauri/src/data/accounts/model.rs) |
| `AccountStore` (load/save/criptografia/import) | [data/accounts/store.rs](../../src-tauri/src/data/accounts/store.rs) |
| Comandos de CRUD e senha | [data/accounts/commands.rs](../../src-tauri/src/data/accounts/commands.rs) |
| Criptografia (sodiumoxide + DPAPI legado) | [data/crypto.rs](../../src-tauri/src/data/crypto.rs) |
| Comandos de API por conta | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs), [commands/account_helpers.rs](../../src-tauri/src/commands/account_helpers.rs) |
| Grupo `moderadas` | [commands/launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) (`MODERATED_GROUP`, `is_moderated_error`, `mark_account_moderated`) |
| Login por navegador / user:pass | [chromium/commands.rs](../../src-tauri/src/chromium/commands.rs) |
| Estado e ações no frontend | [store.tsx](../../src/store.tsx) (`loadAccounts`, `addAccountByCookie`, `removeAccounts`, `updateAccount`, `moveToGroup`, `reorderAccounts`, `unlock`, `applyEncryptionMethod`) |
| Tipo TS | [types.ts](../../src/types.ts) (`Account`, `parseGroupName`, `getFreshnessColor`) |
| Telas de senha/criptografia | [PasswordScreen.tsx](../../src/components/layout/PasswordScreen.tsx), [EncryptionSetupScreen.tsx](../../src/components/layout/EncryptionSetupScreen.tsx), [MiscellaneousTab.tsx](../../src/components/settings/MiscellaneousTab.tsx) |
| Adicionar/importar | [Toolbar.tsx](../../src/components/layout/Toolbar.tsx), [ImportDialog.tsx](../../src/components/dialogs/ImportDialog.tsx), [AccountList.tsx](../../src/components/accounts/AccountList.tsx) (drag & drop) |
| Campos | [AccountFieldsDialog.tsx](../../src/components/dialogs/AccountFieldsDialog.tsx) |
| Utilitários por conta | [AccountUtilsDialog.tsx](../../src/components/dialogs/AccountUtilsDialog.tsx), [ContextMenu.tsx](../../src/components/menus/ContextMenu.tsx) |

## Modelo de dados

`Account` é serializado em **PascalCase** (mesmo formato do RAM antigo em C#):

| Campo JSON | Rust | Tipo | Default / observação |
|---|---|---|---|
| `Valid` | `valid` | bool | `false`; `Account::new` cria com `true`. |
| `SecurityToken` | `security_token` | string | cookie `.ROBLOSECURITY`. `null` → `""`. |
| `Username` | `username` | string | |
| `LastUse` | `last_use` | datetime | formato `%Y-%m-%dT%H:%M:%S%.f`; vazio/`null` → agora. |
| `Alias` | `alias` | string | nome exibido na lista quando preenchido. |
| `Description` | `description` | string | notas. |
| `Password` | `password` | string | senha em texto (opcional). |
| `Group` | `group` | string | `"Default"`; **omitido no JSON quando for `"Default"`**. |
| `UserID` | `user_id` | i64 | **chave única** da conta. |
| `Fields` | `fields` | map string→string | valores `null` viram `""`. |
| `LastAttemptedRefresh` | `last_attempted_refresh` | datetime | atualizado a cada tentativa de refresh de sessão. |
| `BrowserTrackerID` | `browser_tracker_id` | string | aceita alias `BrowserTrackerId`; gerado sob demanda no launch. |

### Campos (`Fields`) com significado no código

| Chave | Usado por | Efeito |
|---|---|---|
| `RobloxVersion` | [commands/versions.rs](../../src-tauri/src/commands/versions.rs), [launch.rs](../../src-tauri/src/commands/launch.rs) | Override de versão do Roblox por conta. |
| `NoCookieRefresh` | [store.tsx](../../src/store.tsx) | `"true"` exclui a conta do auto-refresh de cookie. |
| `Window_Position_X`, `Window_Position_Y`, `Window_Width`, ... | [watcher.rs](../../src-tauri/src/commands/watcher.rs) | Posição de janela salva pelo Watcher (`SaveWindowPositions`). |

Qualquer outra chave é livre (editável em "View/Edit Fields").

## Fluxo

### Carregamento / desbloqueio

1. No startup ([lib.rs](../../src-tauri/src/lib.rs)) o backend verifica `needs_password()`: `true` se o arquivo começa com um header RAM (criptografado) e ainda não há hash de senha em memória. Nesse caso as contas não são carregadas.
2. O frontend chama `needs_password`; se `true`, [App.tsx](../../src/App.tsx) mostra `PasswordScreen`.
3. O usuário digita a senha → `unlock_accounts(password)` → `load_with_password`: calcula `sha512(senha.trim())`, descriptografa, parseia e **guarda em memória o hash e a chave já derivada** (`SessionKey`), reutilizados por todos os saves da sessão.
4. Se o arquivo não for criptografado, `load()` tenta JSON puro e, se falhar, DPAPI legado (Windows).

### Onboarding / troca de método de criptografia

1. Primeira execução (settings recém-criadas → `EncryptionOnboardingState = pending`) e zero contas → `EncryptionSetupScreen` em modo `firstRun`.
2. Também acessível por Settings → Misc → "Change Encryption Method".
3. Opções:
   - **Pass Lock**: senha com pelo menos 8 caracteres (validado na UI e no backend) → `set_encryption_password(password)`.
   - **No Password (Not Encrypted)**: `set_encryption_password(null)` — a UI chama assim desde que o rótulo antigo ("Default Encryption") escondia que o arquivo fica em texto puro.
4. `set_password` troca o hash em memória e re-grava o arquivo imediatamente no novo formato. Se o store está **bloqueado** (nenhum hash em memória) e o arquivo é criptografado → erro "Accounts are locked; unlock them before changing the password." (re-cifrar um arquivo nunca decifrado gravaria uma lista vazia por cima). `set_password(None)` a partir de um store **desbloqueado** é permitido e grava o arquivo em texto puro (remove a criptografia de propósito).
5. O frontend grava `General.EncryptionOnboardingState = completed` e `General.EncryptionMethod = password|default`.

### Adicionar contas

| Origem | Caminho |
|---|---|
| Cookie (Quick Add / Import Cookie / drag & drop de texto) | `validate_cookie(cookie)` → `add_account(securityToken, username, userId)` |
| Username (Quick Add sem cookie) | `lookup_user(username)` → `add_account` com `securityToken: ""` (conta sem sessão) |
| Login no navegador | `open_login_browser` abre Chromium via CDP; ao detectar o cookie emite `browser-login-detected`; a store chama `extract_browser_cookie` (até 8 tentativas, 350 ms) → `addAccountByCookie` → `close_login_browser`. |
| user:pass em lote | uma linha `usuario:senha` por vez → `import_userpass`: abre o login, preenche `#login-username`, espera até ~240 s (480 × 500 ms) pelo cookie, valida e salva com `Password` preenchida. |
| Arquivo antigo (`AccountData.json` de RAM v3/v4) | `import_old_account_data(fileData, password?)`; se o arquivo for criptografado e sem senha → erro `IMPORT_PASSWORD_REQUIRED` e a UI pede a senha. |

### Remover, editar, reordenar, agrupar

- `remove_account(userId)` — a UI (barra inferior) exige digitar `REMOVE` para confirmar remoção em lote.
- `update_account(account)` substitui a conta com o mesmo `UserID` (retorna `false` se não existir), **exceto credenciais**: `SecurityToken` e `Password` são sempre copiados do registro salvo no store. O snapshot do frontend fica velho quando o backend rotaciona o cookie (refresh de sessão, `SetField` do web server…), e editar alias/grupo a partir dele não pode regravar o cookie invalidado.
- `reorder_accounts(userIds)` — usado por drag & drop e "Sort Alphabetically" (ordena por `Alias || Username` dentro do grupo).
- `moveToGroup` no frontend = `update_account` para cada conta com o novo `Group`.

## Regras de negócio

- **`UserID` é a identidade**: `add` com um `UserID` existente **atualiza** `SecurityToken`, `Username`, `Valid`, `LastUse` (e `Password` só se a nova não for vazia); alias, grupo, campos e descrição são preservados.
- `reorder`: IDs listados vão na ordem pedida; contas não listadas são anexadas no final na ordem atual.
- Toda mutação (`add`, `remove` efetivo, `update` efetivo, `reorder`, `set_password`, import com mudanças) regrava o arquivo inteiro.
- **Save atômico e sob o mesmo lock:** as mutações chamam `save_locked(&accounts, …)` **ainda segurando o guard do `Mutex` de contas**, então o que vai para o disco é exatamente o snapshot que a mutação acabou de produzir — não existe janela entre mutar e gravar em que outra escrita possa intercalar. A gravação em si vai para `AccountData.json.tmp` e troca pelo arquivo final com `atomic_replace` (`MoveFileExW` com `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` no Windows, `rename` nos demais). Crash ou disco cheio nunca deixam o arquivo truncado.
- **Ordem de lock:** `accounts` → `session`, sempre. `set_password` solta o guard de `session` antes de pegar o de `accounts` justamente para não inverter essa ordem.
- **Argon2 só no unlock:** a chave de gravação (`SessionKey`) é derivada uma única vez por unlock/`set_password` e reutilizada. O salt de 16 bytes passa a ser sorteado por sessão em vez de por gravação; o **nonce** continua sorteado a cada gravação, e o layout do arquivo é o mesmo de antes. Antes, cada `save()` rodava um Argon2i MODERATE (256 MiB) segurando o lock na thread principal — mover N contas de grupo congelava a UI N vezes.
- **Save recusado após load com falha:** se o arquivo existe mas não pôde ser decodificado, o store marca `load_failed` e todo `save()` retorna erro ("Account file could not be loaded; refusing to overwrite it…") até um load bem-sucedido — uma lista vazia em memória nunca sobrescreve as contas do usuário.
- **Sem texto puro por cima de arquivo criptografado:** com o store bloqueado (sem hash) e o arquivo criptografado, `save()` recusa ("Accounts are locked; unlock them before making changes."). A única exceção é `set_password(None)` a partir de um store desbloqueado.
- **Import de arquivo antigo** (`import_old_account_data`):
  - contas com `UserID <= 0` são ignoradas (`skipped`);
  - `UserID` duplicado dentro do arquivo importado conta como `skipped` e o **último** vence;
  - `UserID` já existente é **substituído integralmente** (`replaced`); novos são anexados (`added`);
  - duplicatas na lista final são removidas; só salva se `added > 0 || replaced > 0`;
  - a senha de import é `trim()`-ada, igual à de unlock — a mesma senha colada com espaço no fim serve para os dois.
- **Grupos**:
  - string livre; vazio é tratado como `"Default"`;
  - prefixo numérico de 1–3 dígitos define a ordem (`"01 Main"` → sortKey 1, exibido como `Main`); sem prefixo → sortKey 999999, depois ordem alfabética ([types.ts](../../src/types.ts) `parseGroupName`);
  - se o único grupo existente é `Default`, a lista é mostrada sem cabeçalho;
  - **ordem manual**: arrastar um grupo pelo punho do cabeçalho reordena e grava `General.GroupOrder`. A partir daí manda a ordem manual, e o prefixo numérico/alfabética só ordena o que ficou de fora. Detalhes que importam:
    - grava a lista **inteira** dos grupos existentes, não só os visíveis — com busca ativa a tela mostra um subconjunto, e gravar "o que está na tela" apagaria da ordem os grupos escondidos pelo filtro;
    - o valor é **JSON** (`["Zeta","Alts, velhas"]`), não lista por vírgula como as outras chaves de lista do INI: nome de grupo é texto livre e pode conter vírgula;
    - valor estragado (texto que não é JSON, JSON que não é lista) vira "sem ordem manual" — a lista de contas não pode deixar de abrir por causa de uma linha torta no INI;
    - grupo que não existe mais sai da ordem; grupo novo entra no fim, não no meio;
    - o arrasto começa num **punho** no cabeçalho, não no cabeçalho inteiro, porque o cabeçalho também colapsa o grupo e recebe drop de conta. O estado do arrasto de grupo (`groupDragState`) é separado do de conta (`dragState`) pelo mesmo motivo: misturados, soltar um grupo moveria contas. Soltar um grupo sobre uma **linha de conta** não faz nada — o alvo é o cabeçalho.
- **Grupo `moderadas`**: quando obter o auth ticket falha no launch e o erro contém `moderated`, `is banned` ou `account has been` (case-insensitive), `mark_account_moderated` move a conta para o grupo `moderadas`, persiste e emite `account-moderated`. Não faz nada se já estiver nesse grupo. O frontend recarrega a lista e mostra o toast "`<nome>` is moderated — moved to 'moderadas'". Chamado em [launch.rs](../../src-tauri/src/commands/launch.rs) nos fluxos de launch único e múltiplo.
- **Indicadores na linha** ([AccountRow.tsx](../../src/components/accounts/AccountRow.tsx)): ponto vermelho = `Valid == false`; cor de "idade" quando `LastUse` > 20 dias (ou seja, 20 dias **sem jogar**, não desde o cadastro) (amarelo → vermelho em 30 dias; desligável com `DisableAgingAlert`); âmbar = lançado pelo app; presença (Online/In Game/In Studio) se `ShowPresence`.
- **Busca** filtra por `Username`, `Alias`, `Description` e `Group` (case-insensitive).
- **Amizade entre contas** (`make_selected_friends`): mínimo 2 contas; modo `mesh` (todos os pares) ou `star` (todos com uma conta principal, que deve estar na seleção). Pula pares já amigos, envia pedido nos dois sentidos, espera `delayMs` → `Friends.RequestDelayMs` → 2500 ms (limitado a **500–60000**) entre pedidos, e verifica no final. Acima de 30 pedidos a UI pede confirmação. Guarda no máximo 20 mensagens de erro.
  - **Uma execução por vez:** um `AtomicBool` estático bloqueia chamadas concorrentes → erro "Já existe uma vinculação de amizades em andamento." (a UI pode remontar e disparar um segundo lote, dobrando pedidos e o risco de rate limit/captcha).
  - **Nunca renova sessão:** a leitura de amigos (`fetch_friend_set`) não usa `run_with_session_retry` (falha vira lista vazia) e `send_directed_friend` apenas reporta erro de sessão (401) como "(sessão inválida — refaça o login da conta)". Motivo: o refresh chama `signoutfromallsessionsandreauthenticate`, que desloga a conta de todas as sessões e derruba clientes abertos (ver [authentication.md](authentication.md)).

### Criptografia

| Aspecto | Implementação ([crypto.rs](../../src-tauri/src/data/crypto.rs)) |
|---|---|
| Hash da senha | `sha512(senha.trim())` |
| KDF | Argon2i13 (`OPSLIMIT_MODERATE`, `MEMLIMIT_MODERATE`) com salt de 16 bytes |
| Cifra | `secretbox` (XSalsa20-Poly1305), nonce de 24 bytes |
| Layout | `RAM_HEADER` + salt(16) + nonce(24) + ciphertext |
| Quando o salt e a chave são sorteados/derivados | Uma vez por unlock ou `set_password` (`SessionKey` em [accounts/store.rs](../../src-tauri/src/data/accounts/store.rs)); o **nonce** continua novo a cada gravação |
| Headers aceitos | `RAM_HEADER` (ic3w0lf22) e `TRANSITION_RAM_HEADER` (niccdevs); gravação sempre com `RAM_HEADER` |
| Legado | Windows: `CryptUnprotectData` (DPAPI) com entropia fixa, só para **leitura** |

### Comandos de API por conta ([account_api.rs](../../src-tauri/src/commands/account_api.rs))

Os que usam o cookie da conta passam por `run_with_session_retry` (ver [authentication.md](authentication.md)), **exceto** `make_selected_friends`, que nunca renova a sessão.

| Comando | O que faz |
|---|---|
| `get_robux` | Saldo de Robux. |
| `get_user_info`, `lookup_user` | Info pública / busca por username (sem cookie). |
| `send_friend_request`, `make_selected_friends` | Amizade (individual / em lote). |
| `block_user`, `unblock_user`, `get_blocked_users`, `unblock_all_users` | Bloqueios. |
| `set_follow_privacy`, `get/set_private_server_invite_privacy` | Privacidade. |
| `set_avatar`, `get_outfits`, `get_outfit_details` | Avatar/outfits (copiar avatar de outro usuário). |
| `join_group` | Entrar em grupo Roblox. |
| `purchase_product` | Compra com preço e vendedor esperados. |
| `change_password` | Troca senha; se a resposta trouxer novo cookie, persiste. |
| `change_email`, `set_display_name` | Conta. |
| `quick_login_enter_code`, `quick_login_validate_code` | Aprovar login por código de 6 dígitos. |
| `check_pin`, `unlock_pin` | PIN da conta (4 dígitos). |
| `get_place_details`, `get_servers`, `get_universe_places`, `get_asset_*` | Usam cookie **se** `userId` for passado; sem retry. |

## Configurações relacionadas

| Seção.Chave | Default | Uso |
|---|---|---|
| `General.EncryptionMethod` | `default` | Método escolhido (`default`/`password`), informativo. |
| `General.EncryptionOnboardingState` | `pending` em instalação nova, `completed` se o INI já existia | Abre o onboarding. |
| `General.AutoCookieRefresh` | `true` | Auto-refresh (ver [authentication.md](authentication.md)). |
| `General.DisableAgingAlert` | `false` | Esconde indicador de idade. |
| `General.GroupOrder` | `[]` | Ordem manual dos grupos, em JSON. Vazio/`[]` = ordem automática (prefixo numérico, depois alfabética). |
| `General.HideUsernames`, `HiddenNameLetters`, `ShowAvatarsWhenHidden`, `HideRobuxWhenHidden` | `false` / `0` | Mascaramento de nomes. |
| `General.ShowPresence`, `PresenceUpdateRate` | `true`, `5` | Presença na lista. |
| `Friends.RequestDelayMs` | (sem default no INI → 2500) | Intervalo entre pedidos de amizade. |
| `Login.PersistentProfile`, `Login.StealthMode` | `true`, `true` | Comportamento do navegador de login. |

## Armadilhas / cuidados

- **Sem senha = texto puro, e a UI diz isso.** Sem senha em memória (`session = None`), `save()` escreve o JSON sem criptografia; a única proteção "default" existente é a leitura de arquivos DPAPI legados. A opção se chama **"No Password (Not Encrypted)"** e avisa que cookies e senhas ficam legíveis no PC.
- Esquecer a senha = perda do arquivo; não há recuperação no código.
- A senha é `trim()`-ada em todos os caminhos que a consomem: `load_with_password`, `set_password` e `decode_accounts_for_import`.
- Não altere `RAM_HEADER`, parâmetros do Argon2 ou o layout sem migração: quebra a leitura de todos os arquivos existentes.
- `update_account` substitui o objeto inteiro (menos `SecurityToken`/`Password`, que vêm do store) — sempre envie a conta completa (o frontend faz `{ ...account, Campo: valor }`). Para trocar o cookie use `add_account` (mesmo `UserID`) ou os fluxos de refresh; `update_account` ignora cookie/senha enviados.
- **Latch de arquivo ilegível (vale para todas as stores de dados).** Quando um arquivo de persistência existe mas não pôde ser lido ou parseado, a store carrega vazia/no default, marca `load_failed` e **recusa toda gravação** até o arquivo ser corrigido/restaurado e o app reiniciado — assim a primeira gravação não apaga os dados do usuário. Mudanças feitas nessa sessão retornam erro. Hoje aplicam o latch: `AccountStore` (`AccountData.json`), `ScriptStore` (`RAMScripts.json`), `VersionsCatalogStore` (`RAMVersions.json`), `ThemePresetStore`, `ThemeStore` e `SettingsStore` (`RAMSettings.ini`). Um arquivo de 0 byte **não** conta como corrupção (é um estado vazio legítimo e continua gravável).
- **`LastUse` mede uso, não cadastro.** É escrito ao criar/atualizar via `add` e, desde então, também a cada launch que dá certo: `AccountStore::mark_used` é chamado em `launch_queue_mark` quando a fila marca `Done` (cobre conta única e lote, nas duas plataformas), no ciclo do Botting (`launch_account_for_cycle`) e nos dois launches do web server (que não passam pela fila). Estado que não seja `Done` não conta — tentativa não é uso.
- O indicador de idade e o auto-refresh de cookie dependem dele, e é por isso que a diferença importa: **antes**, uma conta jogada todo dia mas cadastrada há 30 dias entrava no auto-refresh, e o refresh desloga a conta de todas as sessões (podendo derrubar o cliente aberto). Agora o auto-refresh mira quem está de fato parado, que é o que a regra de 20 dias sempre quis dizer.
- `MultiSelectSidebar.tsx` existe mas não é importado em lugar nenhum; as ações em lote reais estão em [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx).
- O botão "Copy cookies" coloca cookies em texto na área de transferência.
