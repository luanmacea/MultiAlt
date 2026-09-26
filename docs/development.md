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
| `tour` | **Tudo povoado** — favoritos com VIP, busca de jogos, scripts, versões, backups, contas em jogo. É o cenário de revisão de interface: tela vazia esconde onde o botão está |
| `servers-big-game` | Jogo grande: páginas e páginas de servidores cheios antes de aparecer um que caiba o lote |
| `servers-no-fit` | Nenhum servidor cabe: a lista tem que abrir pelos que levam mais contas |
| `servers-truncated` | O backend diz que existem servidores que cabem, mas a página recebida foi cortada antes deles |
| `servers-page-limit` | Varredura parada pelo limite de páginas |
| `servers-real-place` | Réplica com **dados reais** capturados da API (4 páginas do place 15101393044), inclusive com os Job IDs repetidos entre páginas |
| `friends-online` | Amigos por conta, com uma conta falhando |
| `launch-queue` | Fila de launch e contas em jogo |

Um cenário entrega os mesmos dados que o backend entregaria, **inclusive na ordem ruim** — quem tem que se virar é a UI. Ele nunca implementa o comportamento que está sendo testado.

Jogos do harness ficam em [games.ts](../src/dev/harness/games.ts): três places conhecidos (Jailbreak, Blox Fruits, Steal a Brainrot) com nome e um ícone SVG embutido. Place fora dessa lista devolve vazio **de propósito** — é assim que se vê na tela o estado "não sei que jogo é esse". Antes o `batched_get_game_icon` devolvia sempre `null` e nenhuma tela era vista com ícone carregado. O `update_setting` do harness guarda o valor em memória: sem isso, tela que relê settings depois de gravar volta a ver o valor antigo e parece bug (a persistência de verdade é do INI, coberta pelos testes Rust).

### Fluxo com agentes

1. Um agente por área. Cada um abre o seu `?scenario=`, dirige a tela e **só relata**: o que fez, o que esperava, o que viu.
2. Nenhum agente corrige nada — o relatório é entrada, não veredito.
3. Cada achado é **reproduzido e validado** antes de virar mudança: pode ser bug, cenário irreal ou expectativa errada.
4. Correção na ordem de gravidade, com o teste que falha primeiro.
5. **Bug confirmado vira teste** na suíte da funcionalidade. O harness acha; quem impede a volta é o teste.

O harness **não substitui** `bun run check`.

### Armadilha: cache de pré-empacotamento

O alias troca um **pacote** (`@tauri-apps/api/...`) por um arquivo nosso, e o Vite
pré-empacota pacote em `node_modules/.vite/deps` — cache que não invalida quando o
dublê muda. Sem o `optimizeDeps.exclude` que o [vite.config.ts](../vite.config.ts)
declara no modo harness, o navegador recebe um harness velho sem avisar ninguém, e
o agente relata uma tela que não existe mais. Se desconfiar, apague `node_modules/.vite`.

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

- Configuração em [src/i18n/index.ts](../src/i18n/index.ts): idiomas suportados `en`, `de` e `pt` (português do Brasil), fallback `en`, `keySeparator: false` e `nsSeparator: false` — **a chave é a própria frase em inglês**.
- Arquivos: [en](../src/locales/en/common.json) (1584 chaves, fonte), [pt](../src/locales/pt/common.json) (completo) e [de](../src/locales/de/common.json) (parcial — o que falta cai no inglês).
- Helpers em [src/i18n/text.ts](../src/i18n/text.ts): `useTr()` (hook), `tr()` (fora de componentes) e `trNode()` (traduz texto dentro de fragments JSX). Ambos usam `defaultValue: text`, então uma chave ausente aparece em inglês.
- Idioma vem de `General.Language` (normalizado: começa com `de` → `de`; `pt`/`portug` → `pt`; senão `en`). O padrão continua `en` — não há detecção de locale do sistema, de propósito: o app é usado fora do Brasil.
- [src/i18n/locales.test.ts](../src/i18n/locales.test.ts) trava o contrato do catálogo: `pt` cobre o `en` inteiro na mesma ordem, sem chave inventada nem valor vazio, `{{placeholders}}` idênticos em `pt` e `de`, e nada igual ao inglês fora da lista de jargão (`IDENTICAL_BY_DESIGN`).

### Glossário pt-BR

- **Botão = infinitivo** ("Adicionar", "Salvar"); **resultado = particípio** ("Conta adicionada", "Job ID copiado"); só a primeira maiúscula em rótulo; tratamento "você".
- **Não se traduz**: `Roblox`, `Job ID`, `Place ID`, `Universe ID`, `Cookie`, `Fast Flags`, `Web Server`, `Botting Mode`, `Nexus`, `Watcher`, `alt`, `place`, `job`, `loop`, `rejoin`, nome de arquivo/caminho/URL/código, nome de tema e de fonte.
- Termos fixos: account → conta · launch → iniciar · settings → configurações · aged/idle → sem uso · Player Accounts → Contas de jogador · asset → item · General/Developer/Optimization/Misc/Isolation → Geral/Desenvolvedor/Otimização/Diversos/Isolamento.
- Rótulo curto (<20 caracteres no inglês) não passa de +30% em português: trunca na tela.

### Tradução não pode mudar comportamento

O texto exibido nunca é valor de negócio: `<Select>` guarda `value` cru (`"idle"`, `"normal"`) e traduz só o `label`; nome de grupo, fase do Botting e seção/chave do INI são comparados no literal inglês. Ao mexer em tradução, mantenha isso.

O caso que já morde: `addToast` deduz o tom da mensagem pelo texto, e a maioria dos call sites entrega a frase **já traduzida** (`addToast(tr("..."))`). Por isso o heurístico vive em [src/utils/toastTone.ts](../src/utils/toastTone.ts) com marcadores dos dois idiomas completos, e um teste garante que nenhuma tradução apague o tom que o inglês indica.

### Extração de chaves

`bun scripts/i18n/extract-keys.ts` ([extract-keys.ts](../scripts/i18n/extract-keys.ts)):

1. Varre `src/**/*.ts(x)` (ignora pastas `locales` e `i18n`).
2. Captura strings em `t("...")`/`tr("...")`, props `label|description|placeholder|suffix|title|tooltip|alt|aria-label="..."`, objetos `{ label: "..." }`, fragments `label={<>Texto<Badge/></>}` e filhos de `SectionLabel`, `SectionHeader`, `WarningBadge`, `UtilButton`.
3. Descarta strings que parecem URLs, caminhos, hashes, IDs numéricos, template strings (`${`) ou sem letras.
4. Adiciona as chaves faltantes em `en/common.json` com valor = chave. **Não remove** chaves antigas e **não mexe** em `de`/`pt`.

O extrator já reconhece, além do literal direto: componente com atributos
(`<UtilButton onClick={...}>Texto</UtilButton>`), prop com expressão
(`description={cond ? "A" : "B"}`) e ramo de ternário dentro da chamada
(`t(cond ? "A" : "B")` — só o que vem depois de `?`, `:` ou `||`, porque o
literal de comparação é identificador interno). Uma frase que **cita** uma URL
também conta: o filtro só descarta a string que é URL inteira.

O extrator **não varre arquivo de teste**: fixture como
`<MenuItemView item={{ label: "First" }}>` virava chave no catálogo e ia para o
Crowdin como se alguém fosse traduzir.

O que ele ainda não pode ver é chave montada em variável (`t(cat)` com `cat`
vindo de um array, `t(status)`): essas entram no catálogo à mão. Se a sua string
não aparece traduzida, confira primeiro se a chave existe em
`src/locales/en/common.json` — `t()` sem chave devolve o inglês em silêncio, em
todos os idiomas. [src/i18n/reachesTheScreen.test.tsx](../src/i18n/reachesTheScreen.test.tsx)
renderiza em português os pontos que já falharam assim.

A tradução para outros idiomas é sincronizada pelo Crowdin ([crowdin.yml](../crowdin.yml): fonte `src/locales/en/common.json`, destino `src/locales/%two_letters_code%/common.json`; workflow [crowdin-sync.yml](../.github/workflows/crowdin-sync.yml)).

Regra prática: sempre escreva textos de UI via `t(...)`/`tr(...)` ou numa das props reconhecidas, com interpolação no formato `{{nome}}` (nunca template string), e rode o extrator.

## Escape, foco e teclado

- **`Esc` passa por uma pilha LIFO** ([useEscapeStack.ts](../src/hooks/useEscapeStack.ts)): existe **um** listener em `window`, e só o topo da pilha recebe a tecla. Quem monta depois fica no topo — popover sobre diálogo sobre tela —, então a ordem de registro não importa mais. Antes eram 26 handlers em 23 arquivos, nenhum interrompendo a propagação: um `Esc` fechava o diálogo **e** a tela de trás.
- **Diálogo não escreve handler de `Esc`:** use [useModalClose](../src/hooks/useModalClose.ts), que já resolve isso. Se precisar agir antes de fechar (o editor de temas reverte a pré-visualização), passe `onEscape` no 4º parâmetro.
- **Handler de campo que trata `Esc` chama `preventDefault()`** — é o sinal de "já tratei" que a pilha respeita. Sem isso, reverter um campo numérico fechava o diálogo junto.
- **Controle customizado precisa de semântica**: `role` + `aria-*` + `tabIndex` + teclado. O `Toggle` é o exemplo — um `<div onClick>` ali deixava as 9 telas de Settings inoperáveis por teclado. E todo controle novo precisa de foco visível; o anel usa `var(--input-focus)`.

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
