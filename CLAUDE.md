# CLAUDE.md

## 1. Visão geral do projeto

Roblox Account Manager 4 é um gerenciador desktop de múltiplas contas Roblox — reescrita em Tauri 2 (Rust + React/TypeScript) de uma versão anterior. Permite adicionar/gerenciar várias contas (alts), lançar múltiplos clientes Roblox simultaneamente, customizar versões instaladas do Roblox, automatizar fluxos de "botting" (auto-rejoin) e gerenciar servidores VIP/privados por conta.

**Estado atual:** em desenvolvimento ativo. Plataforma primária é Windows (integração profunda com APIs Win32: registro, processos, CryptoAPI); suporte a macOS é parcial.

**Decisões de arquitetura relevantes:**
- Divisão clara IPC: frontend (React) só fala com o backend via comandos Tauri (`src-tauri/src/commands/*`), nunca acessa arquivos ou rede diretamente.
- Estado de contas/settings no backend usa stores em `Arc<Mutex<_>>`, persistidos em arquivo (contas encriptadas com sodiumoxide, settings em INI, scripts/versões em JSON).
- Existe um servidor HTTP local opcional (feature `webserver`, baseado em axum) e um servidor WebSocket (feature `nexus`) para integração com scripts Lua externos (Nexus.lua) — ambos atrás de feature flags no Cargo, não sempre compilados.
- Lógica específica de OS isolada em `platform/windows/` e `platform/macos/` para manter o resto do backend portável.
- "Isolamento pré-launch" (cache wipe, limpeza de registro, MAC rotation) existe deliberadamente para permitir múltiplas contas rodando sem colisão de sessão/cache do Roblox — é tratado como feature central, não hack pontual.
- i18n via react-i18next com chaves extraídas por script próprio (`scripts/i18n/extract-keys.ts`) e geridas via Crowdin.

**Convenções:**
- Frontend em TypeScript estrito (tsconfig strict mode); estado global em padrão Zustand-like (`src/store.tsx`).
- Comentários/commits do projeto majoritariamente em português (ver histórico de commits), apesar do código em inglês.
- Servidores VIP/privados usam Job ID com prefixo `vip:` ou decodificação de URL para acesso.

**Limitações conhecidas:** suporte macOS incompleto; funcionalidades de isolamento/registro são Windows-only.

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
```

### c) Mapa de estrutura do projeto

```
src/                          - Frontend React/TypeScript
  App.tsx                     - Componente raiz, roteamento de modais/dialogs
  store.tsx                   - Estado global (contas, settings, dialogs)
  types.ts                    - Tipos TS compartilhados
  featureFlags.ts             - Feature toggles (ex: ENABLE_NEXUS)
  theme.ts, themeFonts.ts, fontPresets.ts - Sistema de temas/fontes
  components/
    accounts/                 - Lista de contas, linhas, chips, sidebars (single/multi-select)
    dialogs/                  - Modais: AddAccount, Botting, Nexus, Scripts, Versions, Utils
    settings/                 - Abas de settings: General, Developer, Isolation, Watcher, WebServer, Versions
    server-list/              - Browser de servidores: Games, Servers, Favorites, Recent
    menus/                    - Context menus
    layout/                   - Chrome do app: TitleBar, Toolbar, StatusBar, PasswordScreen, EncryptionSetup
    ui/                       - Componentes UI genéricos sem lógica de negócio
  hooks/                      - useSettings, usePrompt, useModalClose, useJoinOnlineWarning
  scripting/                  - Sandbox de execução de scripts custom (security.ts, workerSource.ts)
  i18n/                       - Config react-i18next

src-tauri/src/                - Backend Rust
  main.rs, lib.rs              - Entry point, setup de stores/comandos/tray icon
  data/
    accounts.rs, accounts/    - Store de contas: model, persistência encriptada, comandos add/remove/update
    crypto.rs                 - Init de criptografia (sodiumoxide) — tocar com cuidado, afeta formato do arquivo de contas
    settings.rs, settings/    - Settings: parsing INI (RAMSettings.ini), defaults, tema
    scripts.rs                - Store de scripts custom do usuário
    versions.rs               - Catálogo de versões Roblox instaladas
  api/
    auth.rs                   - Autenticação Roblox (login, refresh de token, cookies)
    roblox.rs, roblox/*.rs    - Cliente da API Roblox (users, economy, thumbnails, private_links/VIP, etc.)
    batch.rs                  - Operações em lote + cache de imagens
    server.rs, server/        - Servidor HTTP local opcional (axum, feature `webserver`): handlers, middleware de auth, state
  commands/                   - Comandos Tauri (IPC frontend→backend)
    launch.rs, launch_shared.rs - Lançamento do Roblox com credenciais/place/job ID
    botting.rs                - Modo botting: timers de auto-rejoin, exemptions
    isolation.rs              - Isolamento pré-launch: cache wipe, limpeza de registro, proteção de processo
    versions.rs               - Instalação/seleção de versão Roblox
    watcher.rs                - Monitoramento de processo: timeout, memória, detecção de beta
    diagnostics.rs            - Diagnóstico de mutex, detecção de app legado
    generators.rs             - Geração de código (Nexus.lua, links rbx-player)
    updater.rs                - Checagem de atualização do app
  platform/
    windows/                  - Implementação Windows-only: launch (proxy/VPN), tracking de processo, isolation, registry, windowing
    macos/                    - Implementação macOS (parcial)
  chromium/                   - Controle de Chromium via CDP: manager, commands, download de binário
  nexus/                      - Servidor WebSocket (feature `nexus`) para integração com Nexus.lua

scripts/                      - Scripts utilitários de build (ex: extração de chaves i18n)
tauri.conf.json               - Config do app Tauri: janela 1100x700 sem decoração, bundle targets, chave pública do updater
src-tauri/Cargo.toml          - Dependências Rust e feature flags (nexus, webserver)
package.json                  - Scripts npm/Bun e dependências frontend
tsconfig.json                 - TS strict mode, target ES2020, JSX React
```
