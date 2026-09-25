# Desenvolvimento

## Pré-requisitos

- **Bun** (gerenciador de pacotes e runner do frontend; lockfile [bun.lock](../bun.lock)).
- **Rust** estável + toolchain para Tauri 2 (a CI usa `dtolnay/rust-toolchain@stable` em Windows).
- Windows é a plataforma principal; várias funções (launch, isolamento, registro, watcher de memória) só compilam/funcionam em `target_os = "windows"`.

## Comandos

Scripts definidos em [package.json](../package.json) e hooks de build em [tauri.conf.json](../src-tauri/tauri.conf.json):

| Comando | O que faz |
|---|---|
| `bun install` | Instala dependências do frontend. |
| `bun run dev` | Vite dev server em `http://localhost:1420` (`strictPort`, ver [vite.config.ts](../vite.config.ts)). Só o frontend — `invoke` falha fora do shell Tauri. |
| `bun run build` | `tsc && vite build` → gera `dist/`. |
| `bun run preview` | Serve o build de `dist/`. |
| `bun run tauri dev` | Abre o app nativo; roda `bun run dev` antes (`beforeDevCommand`). |
| `bun run tauri build` | Build de produção; roda `bun run build` antes (`beforeBuildCommand`), gera artefatos do updater (`createUpdaterArtifacts: true`). |
| `bun run i18n:extract` | Alias de `bun scripts/i18n/extract-keys.ts`. |
| `cd src-tauri && cargo build` | Backend com features default (`nexus` + `webserver`). |
| `cd src-tauri && cargo build --no-default-features` | Backend "standard", sem Nexus nem WebServer. |
| `cd src-tauri && cargo build --no-default-features --features webserver` | Só uma das features. |
| `bun run test` | Testes do frontend (vitest + happy-dom). |
| `bun run test:coverage` | Idem, com cobertura (v8). |
| `bun run typecheck` | `tsc --noEmit`. |
| `bun run test:rust` | `cd src-tauri && cargo test --all-features`. |
| `bun run check` | Portão único antes de commit/PR: typecheck + testes do frontend + testes do Rust. |

### O que a CI verifica

[.github/workflows/ci.yml](../.github/workflows/ci.yml) (push/PR na branch `v4`, runner Windows):

1. `bun install --frozen-lockfile`
2. `bun run build` com `VITE_ENABLE_NEXUS=true` e `VITE_ENABLE_WEBSERVER=true`
3. `bun run build` com ambos `false`
4. `cargo check --locked` (features default)
5. `cargo check --locked --no-default-features`

Antes de abrir PR, rode ao menos `bun run build` e `cargo check` nas duas configurações.

## Testes

### Suítes por funcionalidade (o dia a dia)

Rodar os ~1.800 testes a cada edição é lento. O mapa em [scripts/test-suites.ts](../scripts/test-suites.ts) divide-os por funcionalidade:

```bash
bun run t --list      # lista as suítes e o que cada uma cobre
bun run t launch      # só o que cobre launch (frontend + backend)
bun run t join-links  # links de convite/VIP
bun run t --audit     # acusa teste que não está em nenhuma suíte
```

Fluxo: enquanto mexe numa funcionalidade, rode **só a suíte dela**; antes de commitar, rode `bun run check` (typecheck + auditoria + vitest + `cargo test --all-features`). Só a suíte completa pega quebra cruzada entre áreas.

Ao criar um `mod ..._tests` ou um `*.test.ts(x)` novo, **encaixe-o numa suíte** — `bun run check` roda a auditoria e falha se algo ficar órfão.

### Onde ficam

- **Rust:** módulos `#[cfg(test)] mod <nome>_tests` **dentro** de cada arquivo fonte. Os submódulos são incluídos com `include!()` (não são módulos de verdade), então testes em `src-tauri/tests/` não conseguem enxergar o código — e **cada `mod` de teste precisa de um nome único** em todo o projeto.
- **Frontend:** arquivos `src/**/*.test.ts` (vitest, ambiente `happy-dom`).

### Testes com HTTP mockado (API do Roblox)

As URLs da API passam por [api/endpoints.rs](../src-tauri/src/api/endpoints.rs) (`endpoints::host("auth")` → `https://auth.roblox.com`). Em teste, `set_base_for_tests` aponta todos os subdomínios para um servidor [wiremock](https://docs.rs/wiremock) local — é assim que `validate_cookie`, `get_csrf_token`, o retry de 429 e a resolução de share links são testados sem rede e sem conta real.

**Regra:** nunca escreva `https://<algo>.roblox.com` direto num arquivo de `api/`; use `endpoints::host`. Caso contrário aquele caminho deixa de ser testável.

### O que os testes protegem (regressões já vividas)

| Teste | Protege |
|---|---|
| `launch_url_tests` ([platform/windows/launch.rs](../src-tauri/src/platform/windows/launch.rs)) | canal fixado em `production`/`LIVE`, chave do registro e formato da URL de launch — a tela de atualização do Roblox fechando clientes |
| `middleware_tests` ([api/server/middleware.rs](../src-tauri/src/api/server/middleware.rs)), `password_tests` | bloqueio de requisições vindas de páginas web e exigência de senha |
| `save_should_*` ([data/accounts/store.rs](../src-tauri/src/data/accounts/store.rs)) | escrita atômica e recusa de sobrescrever contas trancadas |
| `join_link_tests` / `join_link_http_tests` ([api/roblox/join_links.rs](../src-tauri/src/api/roblox/join_links.rs)) | formatos de link aceitos e resolução de convites |
| `scriptsRedact.test.ts` | segredos (senha do webserver, API key) fora do snapshot dos scripts |

Ao corrigir um bug, **escreva primeiro o teste que falha** — em especial nos itens acima, que já voltaram uma vez.

### Se o `cargo test` não rodar nenhum teste

Sintoma: o binário de teste sai com `0xC0000139` (`STATUS_ENTRYPOINT_NOT_FOUND`) e nenhum teste aparece. Causa: o binário de teste importa `comctl32!TaskDialogIndirect` (via `tauri` → `muda`), mas só o binário do app carrega o manifesto do Common-Controls 6.0 que resolve esse símbolo — o `cargo test` não.

Contorno pontual: gerar um `<exe>.manifest` ao lado do binário em `src-tauri/target/debug/deps/` declarando `Microsoft.Windows.Common-Controls` 6.0.0.0 (some a cada relink). Fix definitivo, se voltar a incomodar: `println!("cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:...")` no [build.rs](../src-tauri/build.rs) — só para os alvos de teste, para não conflitar com o manifesto que o `tauri-build` embute no app.

**Nunca** referencie `build_router` nem os handlers `/LaunchAccount`/`/FollowUser` a partir de um teste: isso linka o runtime do wry no binário de teste e reproduz o problema.

### Não coberto por testes

Tudo que depende de Win32/estado global: thread do mutex do Multi Roblox, isolamento pré-launch, cancelamento de launch, guarda de reuso de PID e fechamento de contas bot. Esses continuam exigindo teste manual com o app aberto.

## Validando a UI no navegador

```bash
bun run dev:ui        # frontend real em localhost:1420, lado Tauri dublado
```

`UI_HARNESS=1` troca `@tauri-apps/api/{core,event,window}` pelos dublês de [src/dev/harness/](../src/dev/harness/) (alias no [vite.config.ts](../vite.config.ts)). O app roda inteiro no navegador, sem compilar o Rust e sem tocar em conta nenhuma.

### Cenários

Escolha pela URL: `http://localhost:1420/?scenario=servers-big-game&accounts=6`. `window.__harness.scenarios` lista os nomes.

| Cenário | Para quê |
|---|---|
| `default` | App destrancado, contas e settings |
| `servers-big-game` | Jogo grande: páginas e páginas de servidores cheios antes de aparecer um que caiba o lote |
| `servers-no-fit` | Nenhum servidor cabe: a lista tem que abrir pelos que levam mais contas |
| `servers-truncated` | O backend diz que existem servidores que cabem, mas a página recebida foi cortada antes deles |
| `servers-page-limit` | Varredura parada pelo limite de páginas |
| `friends-online` | Amigos por conta, com uma conta falhando |
| `launch-queue` | Fila de launch e contas em jogo |

Um cenário entrega os mesmos dados que o backend entregaria, **inclusive na ordem ruim** — quem tem que se virar é a UI. Ele nunca implementa o comportamento que está sendo testado.

### Fluxo com agentes

1. Um agente por área. Cada um abre o seu `?scenario=`, dirige a tela e **só relata**: o que fez, o que esperava, o que viu.
2. Nenhum agente corrige nada — o relatório é entrada, não veredito.
3. Cada achado é **reproduzido e validado** antes de virar mudança: pode ser bug, cenário irreal ou expectativa errada.
4. Correção na ordem de gravidade, com o teste que falha primeiro.
5. **Bug confirmado vira teste** na suíte da funcionalidade. O harness acha; quem impede a volta é o teste.

O harness **não substitui** `bun run check`.

## Features

### Cargo ([Cargo.toml](../src-tauri/Cargo.toml))

| Feature | Default | Compila |
|---|---|---|
| `nexus` | sim | módulo [nexus/](../src-tauri/src/nexus) (WebSocket para Nexus.lua) |
| `webserver` | sim | [api/server/](../src-tauri/src/api/server) (axum, dependência opcional) |

Comandos relacionados em [services.rs](../src-tauri/src/commands/services.rs) têm stub `#[cfg(not(feature = ...))]` — ao adicionar um comando novo dessas áreas, crie as duas versões.

### Vite ([featureFlags.ts](../src/featureFlags.ts))

`VITE_ENABLE_NEXUS` e `VITE_ENABLE_WEBSERVER` (default `true`) escondem a UI correspondente. Mantenha-as coerentes com as features do Cargo no build que você distribui — nada no código amarra uma à outra.

## Dados em desenvolvimento

Os arquivos de dados (`AccountData.json`, `RAMSettings.ini`, `RAMScripts.json`, ...) ficam na **pasta de dados do usuário** — `%LOCALAPPDATA%\Roblox Account Manager` no Windows (ver [architecture.md](architecture.md#arquivos-de-persistência)). Isso vale também em `tauri dev`: a mesma pasta do app instalado.

Para testar do zero **sem tocar nos seus dados reais**, aponte outra pasta:

```bash
# PowerShell
$env:RAM_DATA_DIR = "$env:TEMP\ram-dev"; bun run tauri dev
```

`RAMSettings.ini` é recriado com defaults e, como não existia, `EncryptionOnboardingState` e `FirstRunWalkthroughState` ficam `pending` (o app abre o onboarding).

## i18n

- Configuração em [src/i18n/index.ts](../src/i18n/index.ts): idiomas suportados `en` e `de`, fallback `en`, `keySeparator: false` e `nsSeparator: false` — **a chave é a própria frase em inglês**.
- Arquivos: [src/locales/en/common.json](../src/locales/en/common.json) e [src/locales/de/common.json](../src/locales/de/common.json).
- Helpers em [src/i18n/text.ts](../src/i18n/text.ts): `useTr()` (hook), `tr()` (fora de componentes) e `trNode()` (traduz texto dentro de fragments JSX). Ambos usam `defaultValue: text`, então uma chave ausente aparece em inglês.
- Idioma vem de `General.Language` (normalizado: começa com `de` → `de`, senão `en`).

### Extração de chaves

`bun scripts/i18n/extract-keys.ts` ([extract-keys.ts](../scripts/i18n/extract-keys.ts)):

1. Varre `src/**/*.ts(x)` (ignora pastas `locales` e `i18n`).
2. Captura strings em `t("...")`/`tr("...")`, props `label|description|placeholder|suffix|title|tooltip|alt|aria-label="..."`, objetos `{ label: "..." }`, fragments `label={<>Texto<Badge/></>}` e filhos de `SectionLabel`, `SectionHeader`, `WarningBadge`, `UtilButton`.
3. Descarta strings que parecem URLs, caminhos, hashes, IDs numéricos, template strings (`${`) ou sem letras.
4. Adiciona as chaves faltantes em `en/common.json` com valor = chave. **Não remove** chaves antigas e **não mexe** em `de`.

A tradução para outros idiomas é sincronizada pelo Crowdin ([crowdin.yml](../crowdin.yml): fonte `src/locales/en/common.json`, destino `src/locales/%two_letters_code%/common.json`; workflow [crowdin-sync.yml](../.github/workflows/crowdin-sync.yml)).

Regra prática: sempre escreva textos de UI via `t(...)`/`tr(...)` ou numa das props reconhecidas, com interpolação no formato `{{nome}}` (nunca template string), e rode o extrator.

## Convenções

- **TypeScript estrito**: [tsconfig.json](../tsconfig.json) com `strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`; target ES2020. `bun run build` falha com variáveis não usadas.
- **Estado global** em um único Context ([store.tsx](../src/store.tsx)); componentes usam `useStore()`. Diálogos são abertos por flags na store (`setSettingsOpen`, `setServerListOpen`, ...).
- **Settings no frontend**: leia com `store.settings?.Section?.Key` (sempre string) ou, em telas de configuração, com o hook [useSettings.ts](../src/hooks/useSettings.ts) (`get/getBool/getNumber/set`), que agrupa gravações com debounce de 160 ms.
- **Erros** do backend são `String`; no frontend vão para `store.setError` (faixa vermelha em [App.tsx](../src/App.tsx)) ou `store.addToast`.
- **Commits** majoritariamente em português, curtos (ex.: `juste de tempo no join`, `multiplos servidores vips`); também há commits em inglês. Mensagens do log de launch em Rust também estão em português (ex.: `"Alvo resolvido: servidor privado/VIP"` em [launch.rs](../src-tauri/src/commands/launch.rs)).
- **PRs**: [scripts/pr-flow.ps1](../scripts/pr-flow.ps1) automatiza o fluxo com `gh`; base default `v4`.

## Como adicionar um novo comando Tauri (ponta a ponta)

Exemplo: comando `get_account_note(user_id) -> String`.

1. **Escolha o arquivo.**
   - Lógica de conta/rede por conta → um arquivo em [src-tauri/src/commands/](../src-tauri/src/commands). Esses arquivos são `include!`-ados em [lib.rs](../src-tauri/src/lib.rs), então a função fica na raiz do crate, sem `pub`, e já enxerga `AccountStore`, `SettingsStore`, `api`, `platform`, `Emitter` etc.
   - Persistência pura → junto da store em `data/<área>/commands.rs`, como `pub fn` (será referenciado com caminho, ex.: `data::accounts::get_accounts`).
   - Se criar um arquivo novo em `commands/`, adicione `include!("commands/novo.rs");` em [lib.rs](../src-tauri/src/lib.rs).

2. **Escreva a função.**

   ```rust
   #[tauri::command]
   async fn get_account_note(
       state: tauri::State<'_, AccountStore>,
       user_id: i64,
   ) -> Result<String, String> {
       let account = get_account(state.inner(), user_id)?;
       Ok(account.fields.get("Note").cloned().unwrap_or_default())
   }
   ```

   - Retorne sempre `Result<T, String>` com `T: Serialize`.
   - Se chamar a API do Roblox com o cookie da conta, envolva em `run_with_session_retry(state.inner(), user_id, |cookie| async move { ... })` ([account_api.rs](../src-tauri/src/commands/account_api.rs)) para ganhar refresh automático de sessão (ver [authentication.md](features/authentication.md)).
   - Para progresso/notificações assíncronas, receba `app: tauri::AppHandle` e use `app.emit("meu-evento", payload)`.
   - Código específico de SO: use `#[cfg(target_os = "windows")]` e forneça um caminho `#[cfg(not(target_os = "windows"))]` que retorne erro, como em [diagnostics.rs](../src-tauri/src/commands/diagnostics.rs). Código dependente de feature: crie a versão `#[cfg(not(feature = "..."))]`, como em [services.rs](../src-tauri/src/commands/services.rs).

3. **Registre** o nome em `tauri::generate_handler![...]` em [lib.rs](../src-tauri/src/lib.rs). Sem isso o `invoke` falha em runtime (não em compilação).

4. **Chame no frontend** com argumentos em camelCase:

   ```ts
   const note = await invoke<string>("get_account_note", { userId });
   ```

   Normalmente isso vira uma função na [store.tsx](../src/store.tsx) ou fica no componente que usa. Tipos compartilhados vão em [types.ts](../src/types.ts).

5. **Eventos**: se emitiu evento, registre o `listen` num `useEffect` com cleanup e adicione-o na tabela de [architecture.md](architecture.md#eventos-backend--frontend). Como `listen()` é assíncrono, use o padrão com flag `disposed` (como em [store.tsx](../src/store.tsx) e [NexusDialog.tsx](../src/components/dialogs/NexusDialog.tsx)): se o efeito já foi desmontado quando a promise resolver, chame o `unlisten` na hora em vez de guardá-lo — senão o listener vaza. Dentro de handlers de longa duração, leia estado via `ref` (ex.: `accountsRef`) para não usar valores velhos capturados no closure.

6. **Opcional — expor para scripts**: se scripts do usuário devem poder chamar o comando, adicione-o a `SCRIPT_INVOKE_COMMANDS` em [ScriptsDialog.tsx](../src/components/dialogs/ScriptsDialog.tsx) (allowlist; ver [scripts.md](features/scripts.md)).

7. **Textos novos** na UI → `t("...")` + `bun run i18n:extract`.

8. **Verifique**: `bun run build`, `cargo check` e `cargo check --no-default-features`.
