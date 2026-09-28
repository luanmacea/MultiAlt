# CLAUDE.md

## 1. Visão geral do projeto

Roblox Account Manager 4 é um gerenciador desktop de múltiplas contas Roblox — reescrita em Tauri 2 (Rust + React/TypeScript) de uma versão anterior. Permite adicionar/gerenciar várias contas (alts), lançar múltiplos clientes Roblox simultaneamente, customizar versões instaladas do Roblox, automatizar o Auto Rejoin (`botting` no código — na tela o nome é Auto Rejoin) e gerenciar servidores VIP/privados por conta.

**Estado atual:** em desenvolvimento ativo. Plataforma primária é Windows (integração profunda com APIs Win32: registro, processos, CryptoAPI); suporte a macOS é parcial.

**Decisões de arquitetura relevantes:**
- Divisão clara IPC: frontend (React) só fala com o backend via comandos Tauri (`src-tauri/src/commands/*`), não acessa arquivos nem rede por conta própria. As exceções que existem estão listadas em `docs/architecture.md` ("Exceções à regra") — notas de release do updater em `api.github.com`, `ram.http`/`ram.ws` dos scripts, Google Fonts e `localStorage` — e não se cria outra.
- Estado de contas/settings no backend usa stores com `Mutex<_>` gerenciados pelo Tauri (`tauri::State`), persistidos em arquivo (o `AccountData.json` é encriptado com sodiumoxide: com a senha do usuário, ou com a chave do aparelho do `AccountData.key` ao lado. Duas exceções em texto puro: o caminho degradado em que a chave não pôde ser criada — grava JSON puro e avisa na faixa — e o `AccountData.json.bak` que a migração deixa. Ver `docs/features/accounts.md`; settings em INI, scripts/versões em JSON).
- Existe um servidor HTTP local opcional (feature `webserver`, baseado em axum) e um servidor WebSocket (feature `nexus`) para integração com scripts Lua externos (Nexus.lua) — ambos atrás de feature flags no Cargo, não sempre compilados.
- Lógica específica de OS isolada em `platform/windows/` e `platform/macos/` para manter o resto do backend portável.
- "Isolamento pré-launch" (cache wipe, limpeza de registro, MAC rotation) existe deliberadamente para uma conta não herdar sessão/cache/fingerprint da anterior — é tratado como feature central, não hack pontual. Mas ele **só roda com nenhum cliente Roblox aberto**: com cliente aberto é pulado inteiro (inclusive o spoof), para não fechar as outras contas. Não separa contas que rodam juntas (`docs/features/isolation.md`).
- i18n via react-i18next com chaves extraídas por script próprio (`scripts/i18n/extract-keys.ts`) e traduzidas no próprio repositório (`src/locales/<idioma>/common.json`, com o inglês como fonte; o português acompanha cada mudança). O Crowdin do projeto original foi removido em 28/09/2026.

**Convenções:**
- Frontend em TypeScript estrito (tsconfig strict mode); estado global em padrão Zustand-like (`src/store.tsx`).
- Comentários/commits do projeto majoritariamente em português (ver histórico de commits), apesar do código em inglês.
- Servidores VIP/privados usam Job ID com prefixo `vip:` ou decodificação de URL para acesso.

**Limitações conhecidas:** suporte macOS incompleto; funcionalidades de isolamento/registro são Windows-only.

**Regras críticas (não quebrar):**
- Launch no Windows **nunca** usa o handler `roblox-player:` cru — o único caminho até ele é o último recurso do `launch_url`, quando o download da build falha (aí o instalador do Roblox pode aparecer); não crie outro. **A build aberta tem que casar com o canal que o cliente vai consultar**, e quem decide isso é o campo `channel:` de dentro da URL de launch (ele vence o registro — provado nos logs do cliente):
  - `launch_url` (protocolo): `build_launch_url` emite `channel:` vazio = produção → abre sempre a build de **produção** (`ensure_player_exe_for_channel(PRODUCTION_CHANNEL)`), baixando-a em silêncio se faltar;
  - `default_player_dir` (old join, sem URL): o cliente lê o canal do **registro** → build daquele canal.
  Ver `docs/features/launch.md`.
- **Não sobrescrever o canal do Roblox.** O usuário também joga pelo site; forçar `production` faz o launch do site divergir e acionar o instalador do Roblox, que fecha todos os clientes. A única escrita permitida é o reparo em `set_player_channel` quando o canal atual foi **aposentado** (o endpoint dele responde 401/404) — timeout, 429, 5xx ou resposta ilegível não contam (`channel_repair_tests`). Canal ≠ `production` no registro **não** é defeito (é o Roblox que grava). O isolamento Medium/Full **apaga** a chave `HKCU/Software/ROBLOX Corporation` inteira, canal junto, e o app não a recria.
- Nada no fluxo de launch pode fechar clientes de outras contas (isolamento pula em vez de matar; o Auto Rejoin só fecha as próprias alts). A exceção que existe é opt-in e desligada por padrão: `General.AutoCloseRobloxForMultiRbx` mata todos os clientes quando o mutex do Multi Roblox não pode ser tomado — não crie outro caminho assim, nem ligue essa opção por conta própria. **Fechar o app também não fecha cliente nenhum:** até 28/09/2026 o `cleanup_multi_roblox_on_exit` ([lib.rs](src-tauri/src/lib.rs)) matava todos os clientes quando havia mais de um aberto (herdado do projeto original, sem teste) — tirava do jogo até a conta que o usuário abriu pelo site. Hoje ele só solta o mutex e limpa o rastreamento (`exit_cleanup_tests`). Build faltando é baixada pelo app, nunca pelo instalador do Roblox (fora o último recurso do `launch_url` acima).
- Não usar `run_with_session_retry` / `refresh_account_session` em leituras não críticas: o refresh chama `signoutfromallsessionsandreauthenticate` e derruba as sessões abertas da conta.

**Testes:** durante a iteração rode só a suíte da funcionalidade (`bun run t <suite>`, `bun run t --list`; mapa em `scripts/test-suites.ts`). Antes de commit/PR rode `bun run check` (typecheck + auditoria das suítes + vitest + `cargo test --all-features`). Teste novo precisa estar em alguma suíte — a auditoria falha se ficar órfão. Testes Rust ficam em `#[cfg(test)] mod <nome>_tests` **dentro** de cada arquivo (submódulos são `include!()`, não módulos), com nome único. URLs da API do Roblox sempre via `endpoints::host(...)` — literal `https://*.roblox.com` em `api/` quebra os testes mockados. Ao corrigir bug, escreva o teste que falha primeiro. Ver `docs/development.md#testes`.

**Validar a UI no navegador (harness):** `bun run dev:ui` sobe o **frontend de verdade** em `localhost:1420` com o lado Tauri trocado por dublês (`src/dev/harness/`, alias no `vite.config.ts` sob `UI_HARNESS=1`). Serve para **procurar** problemas que só aparecem com a tela montada e dados chegando aos poucos — ordem de lista, estados de carregamento, mensagens — sem compilar o app nem tocar em conta nenhuma.

Cenário pela URL: `?scenario=<nome>&accounts=<n>` (`window.__harness.scenarios` lista os disponíveis). Um cenário entrega os mesmos dados que o backend entregaria, **inclusive na ordem ruim**: quem tem que se virar é a UI.

Fluxo com agentes (é assim que se usa):

1. **Dispare um agente por área** (aba Servers, aba Friends, Painel de Sessão, criação de contas...). Cada um abre o seu `?scenario=`, dirige a tela pelo navegador e **só relata**: o que fez, o que esperava, o que viu.
2. **Nenhum agente corrige nada.** O relatório é entrada, não veredito.
3. **Valide cada achado** antes de mexer no código: reproduza, e decida se é bug de verdade, cenário irreal ou expectativa errada. Achado de agente que não reproduz é descartado — e vale dizer isso no relatório final.
4. **Só então corrija**, na ordem de gravidade, com o teste que falha primeiro (ver `docs/development.md#testes`).
5. **Todo bug confirmado vira teste** na suíte da funcionalidade. O harness acha; quem impede a volta é o teste.

Regras do harness: ele **não substitui** `bun run check`, não fala com a rede nem com o Roblox, e um cenário nunca implementa o comportamento que está sendo testado (senão o teste passa sozinho).

**Git (padrão do projeto):** ao terminar uma tarefa, **commitar e dar push imediatamente**, sem perguntar — um commit por tarefa, mensagem em português descrevendo o que mudou. Não acumular várias tarefas num commit só; o usuário não revisa o código antes. Rodar `bun run check` antes de commitar; se falhar, corrigir antes de commitar.

**Autoria só do dono.** Commits e PRs saem só em nome dele: **nunca** adicionar `Co-Authored-By: Claude ...` (nem outra linha de coautoria de IA) na mensagem de commit, nem o rodapé "Generated with Claude Code" em PR. Vale para subagentes também. Pedido do dono (28/09/2026) — essas linhas punham "claude" como contribuidor ao lado dele no GitHub.

**O trabalho vive em `develop`.** É para lá que vão os commits do dia a dia (`git push` na branch atual). A `main` é a branch de **release**: todo push nela dispara o workflow que compila, assina e publica ([docs/development.md](docs/development.md)). Por isso:

- **nunca commitar nem fazer merge na `main` sem o usuário pedir** — cada merge vira uma versão publicada;
- quando ele mandar publicar, o caminho é merge de `develop` em `main` e push;
- depois de uma release, **trazer a `main` de volta para a `develop`** (`git merge main`) antes de seguir, para as duas não divergirem. O workflow **não** commita o número da versão: ele sai das tags a cada release. A série é `0.x` até a primeira versão completamente corrigida; a 1.0.0 (ou qualquer número escolhido) sai pondo o número no `package.json`, no `tauri.conf.json` e no `Cargo.toml` — ver "Numero da versao" em [docs/development.md](docs/development.md).

Limites do push automático: **nunca** `--force`/`--force-with-lease` e nunca reescrever histórico já publicado. Se o push for recusado porque a branch divergiu, integrar o remoto (`git pull --rebase`), rodar `bun run check` de novo e só então empurrar; se houver conflito, parar e avisar o usuário. Não criar branch nem PR sem o usuário pedir.

**Build ao terminar (padrão do projeto):** depois do `bun run check`, do commit e do push, **gerar o executável e dizer onde ele está**, sem perguntar. É como o dono testa o que foi feito — `bun run check` compila o Rust só em modo debug e não produz executável nenhum.

- Executável: `bun run tauri build --no-bundle` → `src-tauri/target/release/roblox-account-manager.exe`.
- Assinatura do artefato de update: exportar `TAURI_SIGNING_PRIVATE_KEY` (conteúdo de `%USERPROFILE%/.tauri/roblox-account-manager.key`) e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (conteúdo de `%USERPROFILE%/.tauri/roblox-account-manager.password.txt`, ao lado dela). Os dois caminhos vão com **barra normal** de propósito: escritos com contrabarra, a sequência contrabarra+r que abre `roblox-account-manager.key` já foi interpretada como retorno de carro duas vezes neste arquivo, e o caminho chegou mutilado a quem foi ler. Sem isso, ver a ressalva abaixo. Detalhes em [docs/development.md](docs/development.md).
- Instalador (quando pedido): `bun run tauri build` → `src-tauri/target/release/bundle/nsis/*-setup.exe`. **Local, só o NSIS:** o `bundle.targets` do [tauri.conf.json](src-tauri/tauri.conf.json) é `["nsis"]`; o MSI sai no CI (`PUBLISH_MSI`, ligado desde 28/09/2026 — é o download recomendado: sai 0/75 no VirusTotal, **não pede admin** e tem auto-update; o setup NSIS leva 1/75 no empacotador) ou localmente com `bun run tauri build --bundles msi`. ⚠️ O MSI usa um **template WiX próprio** ([src-tauri/wix-peruser.wxs](src-tauri/wix-peruser.wxs)) para ser per-user, preso ao Tauri **2.10.0**: ao subir a versão do Tauri, comparar com o `main.wxs` oficial da tag nova, reaplicar as cinco mudanças e **testar a instalação de verdade** — o build passa mesmo com o template defasado. Detalhes em [docs/development.md](docs/development.md). Um `.msi` em `bundle/msi/` é sobra de build anterior — não o entregue como build nova. ⚠️ **Sem as variáveis de assinatura o comando sai com erro mesmo dando certo:** `createUpdaterArtifacts` está ligado e, sem `TAURI_SIGNING_PRIVATE_KEY` no ambiente, a assinatura do artefato de update falha *depois* de o instalador já estar gravado (medido em 26/09/2026, quando ainda saíam dois: "Finished 2 bundles" aparecia antes do erro). Ou seja: o código de saída 1 aqui **não** quer dizer que o instalador não saiu — confira a data do arquivo antes de dizer que falhou. O que de fato não sai é o `.sig` do auto-update.
- Na entrega, citar o caminho, o tamanho e o horário do arquivo — sem isso não dá para saber se o que está na pasta é a build nova ou a da semana passada.
- Se a build falhar, isso é resultado da tarefa: reportar o erro junto, não omitir.
- **Escanear no VirusTotal e no Windows Defender ao terminar** (`bun run scan <caminho>`, [scripts/scan.ts](scripts/scan.ts) — roda os dois motores; ou `bun run vt` só o VirusTotal, `scripts/defender-scan.ps1` só o Defender) e reportar o veredito junto com a entrega. **O Windows Defender local é obrigatório na validação**: é o motor que o dono vê na tela, e ele marcou coisas que o resto do VirusTotal não marcou — passar só no VirusTotal não basta. **Ao mexer em algo que entra no binário (dependência nova, código nativo, criptografia), gerar os instaladores e escanear os setups também, não só o `.exe` solto** (`bun run tauri build --bundles nsis,msi` depois `bun run scan --release`, que pega o `.exe`, o NSIS e o MSI do último build) — o dono distribui o setup. Reportar o veredito de cada arquivo. É como o dono confere que nenhuma mudança reintroduziu o falso positivo de antivírus. O gatilho conhecido é código nativo que os modelos de ML associam a malware — foi assim que a criptografia com libsodium (`sodiumoxide`) levou o `Trojan:Win32/Wacatac.B!ml` da Microsoft (achado por bissecção em 28/09/2026, com `bun run vt`; ver `docs/development.md`). A chave da API mora em `%USERPROFILE%/.tauri/virustotal.key`, fora do repositório. Se der marcação nova, avisar **qual** mudança foi (bissecção se preciso) antes de mexer — pode ser funcionalidade que o dono quer manter, e a decisão é dele.

**Documentação:** `docs/README.md` (índice), `docs/architecture.md` e um `.md` por funcionalidade em `docs/features/`. Mantenha-os atualizados ao mudar regras de negócio.

## 2. Tecnologias, comandos e mapa de estrutura

### a) Stack tecnológico

- **Frontend:** React 19, TypeScript, Vite 6, Tailwind CSS 4, lucide-react (ícones), react-i18next (i18n)
- **Backend:** Rust (edition 2021), Tauri 2 (engine Wry), tokio (async, full), reqwest (HTTP client), axum (HTTP server, feature opcional), tokio-tungstenite (WebSocket), serde/serde_json, sodiumoxide (criptografia de contas), windows-sys (APIs Win32)
- **Plugins Tauri:** autostart, updater, process, window-state, single-instance
- **Gerenciador de pacotes:** Bun (frontend), Cargo (backend)

### b) Comandos de build/run/test

```bash
# Frontend (dev, fora do shell Tauri)
bun install
bun run dev          # Vite dev server em localhost:1420

# Build completo (TS check + bundle)
bun run build

# Preview do build
bun run preview

# App Tauri completo (dev, abre janela nativa)
bun run tauri dev

# Build de produção do app desktop
bun run tauri build

# Extrair chaves de tradução
bun scripts/i18n/extract-keys.ts

# Backend Rust isolado
cd src-tauri && cargo build
cd src-tauri && cargo build --features webserver,nexus

# Executável de release (o que o dono roda para testar)
bun run tauri build --no-bundle   # -> src-tauri/target/release/roblox-account-manager.exe
```

### c) Mapa de estrutura do projeto

```
src/                          - Frontend React/TypeScript
  App.tsx                     - Componente raiz, roteamento de telas/diálogos
  store.tsx                   - Estado global (contas, settings, dialogs), ~3000 linhas
  types.ts                    - Tipos TS compartilhados
  featureFlags.ts             - Feature toggles (ex: ENABLE_NEXUS)
  repo.ts                     - Endereço deste repositório (travado por repoOwnership.test.ts)
  theme.ts, themeFonts.ts, fontPresets.ts - Sistema de temas/fontes
  components/
    ChooseGameScreen.tsx      - Tela "Choose Game": Favorites, Games, Recent, Servers, Friends, Follow, Console, Windows
    accounts/                 - Lista de contas, linhas, chips, painel de UMA conta (DetailSidebar → SingleSelectSidebar). O painel de multi-seleção foi apagado: ação em lote fica na BottomActionBar e na Choose Game
    dialogs/                  - Modais: AddAccount, Import, Botting (Auto Rejoin), Afk, Session, Backups, Generator, Nexus, Scripts, Versions, AccountUtils, AccountFields, ThemeEditor, Update
    settings/                 - Abas: General, Developer, WebServer, Watcher, Account Generator, Isolation, Versions, Optimization, Misc
    server-list/              - Diálogo Server List: Games, Servers, Favorites, Recent
    servers/                  - Aba Servers da Choose Game (varredura, preferência de servidor, região)
    friends/                  - Aba Friends da Choose Game
    session/                  - Painel de Sessão (fila de launch, contas em jogo)
    signup/                   - Criação de contas no navegador
    menus/                    - Context menus
    layout/                   - Chrome do app: TitleBar, Toolbar, StatusBar, BottomActionBar, PasswordScreen, EncryptionSetupScreen, VaultKeyBanner, SafeModeBanner, AppErrorBoundary
    ui/                       - Componentes UI genéricos sem lógica de negócio
  hooks/                      - useSettings, usePrompt, useModalClose, useEscapeStack, useJoinOnlineWarning, useGameIdentity, useCopyCredentialWarning
  scripting/                  - Sandbox de execução de scripts custom (security.ts, workerSource.ts)
  utils/                      - cookies.ts (parseImportLine), toastTone.ts, platform.ts, robloxErrors.ts, afkBeep.ts
  dev/harness/                - Dublês do Tauri para `bun run dev:ui` (cenários)
  i18n/                       - Config react-i18next

src-tauri/src/                - Backend Rust
  main.rs, lib.rs             - Entry point, setup de stores/comandos/tray icon
  webview_recovery.rs         - Safe mode de vídeo do WebView2 (janela em branco)
  data/
    accounts.rs, accounts/    - Store de contas: model, persistência encriptada, comandos add/remove/update
    crypto.rs                 - Init de criptografia (sodiumoxide) — tocar com cuidado, afeta formato do arquivo de contas
    vault_key.rs              - Chave do aparelho (AccountData.key) — tocar com cuidado: errada, tranca o usuário fora das contas
    settings.rs, settings/    - Settings: parsing INI (RAMSettings.ini), defaults, tema, pasta de dados (paths.rs)
    scripts.rs                - Store de scripts custom do usuário
    versions.rs               - Catálogo de versões Roblox instaladas
  api/
    auth.rs                   - Autenticação Roblox (login, refresh de token, cookies)
    http_client.rs            - Clientes HTTP com teto de tempo — cliente novo sai de `http_client::builder()`
    endpoints.rs              - Host das APIs (`endpoints::host`), o que permite mockar nos testes
    roblox.rs, roblox/*.rs    - Cliente da API Roblox (users, economy, thumbnails, private_links/VIP, etc.)
    batch.rs                  - Operações em lote + cache de imagens
    server.rs, server/        - Servidor HTTP local opcional (axum, feature `webserver`): handlers, middleware de auth, state
  commands/                   - Comandos Tauri (IPC frontend→backend), `include!`-ados em lib.rs
    launch.rs, launch_shared.rs - Lançamento do Roblox com credenciais/place/job ID, fila de launch
    botting.rs                - Auto Rejoin (nome interno botting): timers de rejoin, papéis main/alt (player/bot no código)
    afk.rs                    - AFK mode: tecla periódica na janela de cada conta
    account_api.rs            - Comandos de API por conta (leitura: read_without_refresh; ação: run_with_session_retry)
    account_helpers.rs        - get_cookie e helpers de conta
    backups.rs                - Backups dos dados (criar, listar, restaurar, apagar)
    image_cache.rs            - Avatares e ícones em lote (cache)
    platform_info.rs          - get_platform_capabilities
    services.rs               - Liga/desliga web server e Nexus (com stub sem a feature); exporta o Nexus.lua embutido (`export_nexus_lua`)
    isolation.rs              - Isolamento pré-launch: cache wipe, limpeza de registro, spoof de MachineGuid/MAC
    versions.rs               - Instalação/seleção de versão Roblox
    watcher.rs                - Monitoramento de processo: timeout, memória, detecção de beta
    diagnostics.rs            - Diagnóstico de mutex, detecção de app legado
    generators.rs             - Gerador de contas (BloxGen)
    updater.rs                - Checagem de atualização do app
  platform/
    windows/                  - Windows-only: launch e canal (launch.rs), Multi Roblox (core.rs, singleton.rs), tracking de processo, isolation, registro, janelas, versões, otimização, SendInput do AFK (input.rs — o único ponto)
    macos/                    - Implementação macOS (parcial)
  chromium/                   - Chromium via CDP: manager, commands, cdp.rs, download de binário, criação de contas (signup.rs, signup_session.rs)
  nexus/                      - Servidor WebSocket (feature `nexus`) para integração com Nexus.lua

scripts/                      - Scripts de build/teste (extração de chaves i18n, mapa de suítes test-suites.ts)
src-tauri/tauri.conf.json     - Config do app Tauri: janela 1100x700 sem decoração, bundle targets, chave pública do updater
src-tauri/Cargo.toml          - Dependências Rust e feature flags (nexus, webserver)
package.json                  - Scripts npm/Bun e dependências frontend
tsconfig.json                 - TS strict mode, target ES2020, JSX React
```
