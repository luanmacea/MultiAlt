# Scripts do usuário

## Objetivo

Permitir que o usuário escreva pequenos programas JavaScript que automatizam o app (ler contas, lançar, controlar o Auto Rejoin, falar com serviços externos, desenhar uma mini-UI), executados em um **sandbox** (Web Worker) com permissões explícitas por script.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Store e persistência (`RAMScripts.json`) | [data/scripts.rs](../../src-tauri/src/data/scripts.rs) |
| Diálogo, host de execução, allowlists | [ScriptsDialog.tsx](../../src/components/dialogs/ScriptsDialog.tsx) |
| Código do Worker (API `ram`, sandbox) | [scripting/workerSource.ts](../../src/scripting/workerSource.ts) |
| Limites e validações de segurança | [scripting/security.ts](../../src/scripting/security.ts) |
| Tipos | [scripting/types.ts](../../src/scripting/types.ts) |
| Montagem no app | [App.tsx](../../src/App.tsx) (`<ScriptsDialog>` sempre montado) |

## Modelo (`ManagedScript`, JSON camelCase)

| Campo | Default | Observação |
|---|---|---|
| `id` | — | obrigatório, ≤ 96 chars, só `A-Z a-z 0-9 - _ .` |
| `name` | — | obrigatório, ≤ 120 chars, sem caracteres de controle |
| `description` | `""` | ≤ 2048 chars |
| `language` | `javascript` | qualquer valor é normalizado para `javascript` |
| `source` | `""` | ≤ 262 144 bytes, sem `\0` |
| `enabled` | `true` | desabilitar para o runtime |
| `trusted` | `false` | exigido por ações sensíveis |
| `autoStart` | `false` | inicia automaticamente |
| `permissions` | tudo `false` | `allowInvoke`, `allowHttp`, `allowWebSocket`, `allowWindow`, `allowModal`, `allowSettings`, `allowUi`, `allowPrivateNetwork` |
| `createdAtMs` / `updatedAtMs` | agora | `createdAtMs` é preservado em updates |

Comandos: `get_scripts` (ordenado por nome, case-insensitive), `save_script` (upsert), `delete_script`.

## Fluxo

1. `ScriptsDialog` é montado sempre em [App.tsx](../../src/App.tsx) (a renderização visual só acontece quando aberto), então ao entrar no app ele chama `get_scripts`.
2. Uma única vez após carregar, inicia todos os scripts com `enabled && autoStart`.
3. **Start**: `createScriptWorker()` cria um Worker a partir de um Blob com `WORKER_SOURCE`; o host envia o código e metadados.
4. No Worker, o código passa por `normalizeUserCode` (remove BOM, NFKC...) e `assertNoDynamicImport`, e roda dentro de um `AsyncFunction` em modo estrito com `ram`, `console` e timers injetados e com os nomes globais perigosos sombreados.
5. Cada `ram.<algo>(...)` vira uma mensagem `host-request { requestId, action, payload }`. O host em [ScriptsDialog.tsx](../../src/components/dialogs/ScriptsDialog.tsx) valida permissão/trust, executa e responde. Timeout de host request no worker: 20 s.
6. O host envia eventos para o script: `window:update` (snapshot do app), `ws` (mensagens de WebSocket) e `ui` (interações com a mini-UI).
7. **Stop**: mensagem `stop` → rejeita pendentes, limpa handlers e `close()`; o host também faz `worker.terminate()`.
8. **Save**: `sanitizeScriptSourceForSave` (normaliza aspas/traços "inteligentes", remove caracteres invisíveis, desembrulha bloco ```` ``` ````) → `save_script`. Se a assinatura de segurança (`getScriptSecuritySignature`: trusted + 8 permissões, com `allowPrivateNetwork`) mudou e o script está rodando, ele é **parado**.

## API `ram` e permissões

| Chamada | Ação host | Permissão | Exige `trusted` |
|---|---|---|---|
| `ram.log/info/debug/warn/error`, `now`, `randomId`, `sleep`, `on` | local | — | — |
| `ram.invoke(cmd, args)` | `invoke` | `allowInvoke` | **sim** |
| `ram.http.request/get/post` | `http.request` | `allowHttp` | **sim** |
| `ram.ws.connect/send/close` | `ws.*` | `allowWebSocket` | **sim** |
| `ram.ws.list` | `ws.list` | `allowWebSocket` | não |
| `ram.window.snapshot/accounts/selected` | `window.*` | `allowWindow` | não |
| `ram.modal.alert/confirm/prompt/json` | `modal.*` | `allowModal` | não |
| `ram.settings.get/all` | `settings.get/all` | `allowSettings` | não |
| `ram.settings.set` | `settings.set` | `allowSettings` | **sim** |
| `ram.ui.set/patch/clear`, `ram.ui.on` | `ui.*` | `allowUi` | não |

Além dessas, `allowPrivateNetwork` não habilita nenhuma chamada nova: ela libera alvos de localhost/rede privada dentro de `ram.http.*` e `ram.ws.*` (ver "Rede privada" abaixo).

### Allowlist de `ram.invoke`

Somente estes comandos (`SCRIPT_INVOKE_COMMANDS`): `update_account`, `add_account`, `remove_account`, `validate_cookie`, `launch_roblox`, `launch_multiple`, `cmd_kill_roblox`, `cmd_kill_all_roblox`, `get_presence`, `start_botting_mode`, `stop_botting_mode`, `get_botting_mode_status`, `add_botting_accounts`, `set_botting_player_accounts`, `botting_account_action`, `start_generator`, `stop_generator`, `get_generator_status`, `generator_test_key`, `start_web_server`, `stop_web_server`, `start_nexus_server`, `stop_nexus_server`, `nexus_send_command`, `get_theme`.

**`get_accounts` é a exceção filtrada.** Ele não está em `SCRIPT_INVOKE_COMMANDS` (passagem direta) porque devolve `SecurityToken` (cookie) e `Password` de todas as contas. Ele continua chamável via `ram.invoke("get_accounts", {})`, mas passa por `SCRIPT_INVOKE_SANITIZERS` em [scripting/security.ts](../../src/scripting/security.ts): `redactAccountSecrets` remove `SecurityToken`/`Password`/`Cookie`/`RobloSecurity` e, dentro de `Fields`, qualquer chave com `password`, `cookie`, `secret`, `token` ou `apikey` no nome. O resto (`UserID`, `Username`, `Alias`, `Group`, `Valid`, `LastUse`, `Fields` não secretos, ...) chega intacto.

O ciclo ler → editar → gravar continua funcionando: `update_account` no backend **sempre** relê cookie e senha da store antes de gravar ([data/accounts/commands.rs](../../src-tauri/src/data/accounts/commands.rs)), então devolver a conta filtrada não apaga credencial nenhuma.

## Regras de negócio

- **Backend** (`ScriptStore::upsert`): valida id/nome/descrição/tamanho do fonte; máximo **256** scripts; arquivo `RAMScripts.json` maior que **8 MB** não é carregado.
- **Sandbox** (Worker): bloqueia `import()`, `importScripts`, `eval` (direto, opcional e indireto) e construtores `Function`/`AsyncFunction`/`GeneratorFunction`/`AsyncGeneratorFunction` por regex no fonte; sombreia `globalThis`, `self`, `window`, `document`, `navigator`, `location`, `fetch`, `WebSocket`, `XMLHttpRequest`, `Worker`, `indexedDB`, `caches`, `localStorage`, `WebTransport`, `WebSocketStream`, `RTCPeerConnection`, `webkitRTCPeerConnection`, `RTCDataChannel`, etc. (`lockDownDangerousGlobals` também redefine esses globais como `undefined` não configurável em `self`, incluindo `caches` e `indexedDB`). Eventos permitidos em `ram.on`: `window:update`, `ws`, `ui` (máx. 32 handlers por evento, 96 no total).
- **HTTP**: só `http:`/`https:`; métodos `GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS`; timeout ≤ 30 s; ≤ 64 headers; corpo ≤ 256 KiB; resposta ≤ 512 KiB. Executado com `fetch` no WebView.
- **WebSocket**: só `ws:`/`wss:`; ≤ 8 conexões por script; mensagem ≤ 256 KiB.
- **Rede privada bloqueada por padrão** (`isPrivateOrLoopbackHost`): `localhost`, `*.localhost`, 10/8, 127/8, 0/8, 169.254/16, 172.16/12, 192.168/16, IPv6 loopback/ULA/link-local, IPv4-mapeado privado, hosts numéricos ambíguos (hex/octal) e hosts com `%`. Liberar exige a **permissão** `allowPrivateNetwork` no script, concedida pelo usuário no diálogo ("Private Network (localhost/LAN)"). O campo `allowPrivateNetwork` no payload de `ram.http.request`/`ram.ws.connect` é **ignorado** (`resolvePrivateNetworkAccess`) — antes era ele que liberava o acesso, ou seja, o script se autoconcedia a permissão.
- **Onde o grant mora:** o backend (`RAMScripts.json`) não persiste `allowPrivateNetwork`; ele fica em `RAMSettings.ini`, seção `[ScriptPrivateNetwork]`, com o id do script como chave (`true`/`false`). Como `ram.settings` só enxerga seções `Script.<id>`, nenhum script consegue se autoconceder o acesso. O grant é lido junto com `get_scripts`, gravado após `save_script` e zerado no `delete_script`.
- **Settings de script**: cada script só enxerga a seção `Script.<id>` (ou `Script.id-<fnv1a>-<hex>` se o id tiver caracteres fora do padrão); chave ≤ 80 chars, valor ≤ 4096 bytes.
- **UI**: ≤ 80 elementos; tipos `button, text, number, toggle, select, textarea, badge, divider`; ≤ 120 opções por select; texto ≤ 2048 chars; patch ≤ 16 KiB.
- **Logs**: mensagem truncada em 4000 chars; ≤ 64 host requests pendentes.
- O snapshot de janela (`window:update` para quem tem `allowWindow`, e `ram.window.accounts`) **não** inclui cookie nem senha: só `userId, username, alias, group, valid, lastUse, lastAttemptedRefresh`, além de place/job/launchData, seleção, presença, contas lançadas, status de botting/generator e settings **redigidas** (`redactSecretSettings`).
- **Settings no snapshot sem segredos:** `redactSecretSettings` remove de cada seção as chaves cujo nome termina em `password`, `apikey`, `api_key`, `secret` ou `token` (case-insensitive), exceto flags que começam com `allow`, `every`, `auto` ou `require` (ex.: `EveryRequestRequiresPassword` continua visível). Assim `WebServer.Password`, `BloxGen.ApiKey` etc. não chegam aos scripts.

## Configurações relacionadas

| Onde | O quê |
|---|---|
| `RAMScripts.json` (pasta de dados do usuário — `get_scripts_path` em [paths.rs](../../src-tauri/src/data/settings/paths.rs); só é a pasta do exe no modo portátil) | Scripts salvos. |
| `RAMSettings.ini` → `[Script.<id>]` | Estado persistido por cada script via `ram.settings`. |

## Armadilhas / cuidados

- As verificações de permissão existem **apenas no frontend**. O backend não sabe quem chamou um comando; `save_script` aceita qualquer combinação de `trusted`/`permissions`.
- Um script `trusted` com `allowInvoke` ainda pode **usar** as contas (lançar, remover, trocar grupo) e pedir `validate_cookie` com um cookie que ele mesmo forneça; o que ele não consegue mais é **ler** o cookie/senha das contas já cadastradas. Trate "trusted + allowInvoke" como controle total do app, não como leitura de credenciais.
- Ao acrescentar um comando à allowlist, confira o que ele **devolve**: se a resposta carrega segredo, ele precisa de um filtro em `SCRIPT_INVOKE_SANITIZERS`, não de uma entrada em `SCRIPT_INVOKE_COMMANDS`.
- `ram.settings.all` só devolve a seção do próprio script. O evento `window:update` é enviado a **todo** script em execução (no start e a cada mudança), mas agora **aparado pela permissão**: sem `allowWindow` o script recebe apenas as settings redigidas e o `ts` — nada de contas, seleção, presença, contas lançadas, estado de botting/generator nem place/job/launchData (`snapshotForPermissions`, em [scripting/security.ts](../../src/scripting/security.ts)). Antes o empurrão ia inteiro e `allowWindow` só protegia as chamadas `ram.window.*`, então um script sem a permissão recebia a lista de contas de graça.
- As settings continuam indo para todo script de propósito, **exceto** as chaves secretas filtradas por `redactSecretSettings`. Uma chave secreta nova com nome fora do padrão (ex.: `...Key`, `...Pass`) **vaza** — siga os sufixos `Password`/`ApiKey`/`Secret`/`Token` ou atualize `isSecretSettingKey`.
- Os bloqueios do sandbox são por regex e sombreamento de nomes — não são um isolamento de processo. Não rode scripts de terceiros sem ler.
- Ao adicionar um comando Tauri que scripts devem usar, é preciso incluí-lo em `SCRIPT_INVOKE_COMMANDS`; comandos fora da lista são recusados.
- `start_web_server`/`start_nexus_server` estão na allowlist mesmo em builds sem essas features; nesses builds retornam erro.
