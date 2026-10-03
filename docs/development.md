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
| `bun run check` | Portão único antes de commit/PR: typecheck + auditoria das suítes (`test:audit`) + testes do frontend + testes do Rust (`cargo test --all-features`). |

### O que a CI verifica

[.github/workflows/ci.yml](../.github/workflows/ci.yml) (push e PR nas branches `main` e `develop`, só quando mudam `src/`, `src-tauri/`, `package.json`, `vite.config.ts` ou o próprio workflow; runner Windows, com cache do Rust):

**Cada commit é verificado uma vez só.** Com um PR aberto da branch, o evento `pull_request` já roda o CI **no mesmo commit** que o `push` — eram dois runs de ~20 min medindo a mesma coisa (visto no `af99c73`, 28/09/2026). O job `guard` (segundos, no runner Linux) consulta se há PR aberto para a branch e, se houver, pula o run do `push`: quem vale é o evento do PR, que é o check que libera o merge. Um `concurrency` por branch/PR também cancela o run anterior quando chega um commit novo — o resultado do commit já substituído não interessa.


1. `bun install --frozen-lockfile`
2. `bun run typecheck`
3. `bun run test` (vitest)
4. `cargo test --locked --all-features`
5. `bun run build` com `VITE_ENABLE_NEXUS=true` e `VITE_ENABLE_WEBSERVER=true`
6. `bun run build` com ambos `false`
7. `cargo check --locked` (features default)
8. `cargo check --locked --no-default-features`

A CI roda o mesmo portão do `bun run check`, em passos separados (typecheck, auditoria das suítes, vitest, `cargo test`). Antes de commitar, `bun run check`; mudança em feature do Cargo pede também o `cargo check --no-default-features`.

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

O **teto de tempo** dos clientes fica em [api/http_client.rs](../src-tauri/src/api/http_client.rs) (`builder()` para chamadas de API, `download_builder()` para o download de build). Em teste, `http_client::test_support::shorten` encurta o teto para provar que uma chamada **pendurada** é cortada sem o teste levar 30 s — é como `http_timeout_tests` cobre o pedido de auth ticket.

**Regra:** cliente novo em `api/` (ou em qualquer caminho de launch) sai de `http_client::builder()`, nunca de `reqwest::Client::new()` — este último não tem teto nenhum. E erro de transporte vira texto com `http_client::describe_error`, senão o timeout chega na tela como `error sending request for url (…)`.

### O que os testes protegem (regressões já vividas)

| Teste | Protege |
|---|---|
| `launch_url_tests`, `channel_follow_tests`, `channel_build_pairing_tests` ([platform/windows/launch.rs](../src-tauri/src/platform/windows/launch.rs)) | a build aberta casando com o canal que o cliente consulta: URL de launch sempre com `channel:` vazio (= produção) e o protocolo abrindo a build de produção; o old join **seguindo** o canal do registro (o app não fixa canal); o mapeamento canal → endpoint de versão/CDN (`production` = `LIVE`), o nome da chave do registro e o formato da URL — a tela de atualização do Roblox fechando clientes |
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

Tudo que depende de Win32/estado global: thread do mutex do Multi Roblox, isolamento pré-launch, cancelamento de launch, guarda de reuso de PID e fechamento das alts ao parar o Auto Rejoin. Esses continuam exigindo teste manual com o app aberto.

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
| `friend-link` | Make Friends em andamento, com uma conta falhando |
| `console-history` | Linhas de launch, Auto Rejoin e Watcher chegando aos poucos no Console |
| `groups` | Contas em grupos nomeados (um com prefixo numérico, um com vírgula no nome) para ver cabeçalhos e arrastar a ordem |
| `launch-queue` | Fila de launch e contas em jogo |
| `vault-key-warning-locked` | Tela de senha com a faixa vermelha do `.key` (`writeFailed`) — para ver se o rodapé cabe e se a pílula de minimizar/fechar não cobre o texto |
| `vault-key-warning-setup` | A mesma faixa (`migrationFailed`) na tela de criptografia da primeira execução, onde o rodapé são os botões Continue/Cancel |
| `afk-mode` | AFK mode desligado, como num INI novo (sem tecla escolhida, intervalo 10, bipe desligado), quatro contas com cliente aberto; no ciclo automático a 2ª volta com `focusDenied` |
| `afk-mode-running` | AFK mode já rodando ao abrir (há 65 min, ou `&afkSince=<min>`), com uma conta em `focusDenied` e outra sem janela (`noWindow`) |

Um cenário entrega os mesmos dados que o backend entregaria, **inclusive na ordem ruim** — quem tem que se virar é a UI. Ele nunca implementa o comportamento que está sendo testado.

Jogos do harness ficam em [games.ts](../src/dev/harness/games.ts): três places conhecidos (Jailbreak, Blox Fruits, Steal a Brainrot) com nome e um ícone SVG embutido. Place fora dessa lista devolve vazio **de propósito** — é assim que se vê na tela o estado "não sei que jogo é esse". Antes o `batched_get_game_icon` devolvia sempre `null` e nenhuma tela era vista com ícone carregado. O `update_setting` do harness guarda o valor em memória: sem isso, tela que relê settings depois de gravar volta a ver o valor antigo e parece bug (a persistência de verdade é do INI, coberta pelos testes Rust).

### Fluxo com agentes

1. Um agente por área. Cada um abre o seu `?scenario=`, dirige a tela e **só relata**: o que fez, o que esperava, o que viu.
2. Nenhum agente corrige nada — o relatório é entrada, não veredito.
3. Cada achado é **reproduzido e validado** antes de virar mudança: pode ser bug, cenário irreal ou expectativa errada.
4. Correção na ordem de gravidade, com o teste que falha primeiro.
5. **Bug confirmado vira teste** na suíte da funcionalidade. O harness acha; quem impede a volta é o teste.

O harness **não substitui** `bun run check`.

### Atualizacoes: este repositorio, com chave propria

O app **era bifurcado** de `niccsprojects/Roblox-Account-Manager` e continuava amarrado a ele: o updater lia o manifesto daquele repositorio e a chave publica em `tauri.conf.json` era a de la. O banner "atualizacao disponivel" anunciava a versao **do outro projeto** e instalar teria substituido este app pelo binario deles — com assinatura valida, porque a chave conferia. Nao era um botao sobrando.

Como funciona agora:

1. **Manifesto**: `https://raw.githubusercontent.com/luanmacea/roblox-account-manager/update-manifests/<canal>/latest.json`. Canais: `stable`, `beta`, `stable-nexus-ws`, `beta-nexus-ws` ([updater.rs](../src-tauri/src/commands/updater.rs), `resolve_manifest_channel`). Quem esta no `beta` consulta **tambem** o `stable`, porque uma estavel mais nova vence a beta. ⚠️ Um canal sem release publicada responde **404**, e isso e "nada para atualizar", nao falha — foi esse o bug de 28/09/2026: o `stable` nunca existiu, o 404 dele subia, e "Procurar agora" falhava **sempre** no canal beta (`primary_channel_result` / `fallback_channel_result`, cobertos por `updater_tests`).
2. **Publicacao**: o trabalho do dia a dia vive na branch `develop` (o CI roda nela); a `main` e a branch de **release** e nao recebe commit direto. O workflow [release-v4.yml](../.github/workflows/release-v4.yml) roda a cada push em `main` — ou seja, uma vez por merge aprovado (o runner nasce limpo, entao ele e o CI usam `Swatinem/rust-cache` — sem isso cada release recompila a arvore inteira duas vezes), calcula a versao, compila as duas variantes (padrao e Nexus+WebServer), cria a release e escreve o `latest.json` no branch `update-manifests` ([generate-update-manifest.mjs](../.github/scripts/generate-update-manifest.mjs), que cria o branch se ele ainda nao existir). Commit com `[skip release]` na mensagem nao publica.
3. **Assinatura**: o updater do Tauri so aceita manifesto assinado pela chave privada correspondente a `pubkey` do `tauri.conf.json`. A chave deste projeto foi gerada em 26/09/2026 e mora **fora do repositorio**, na pasta `.tauri` do perfil do usuario Windows: `roblox-account-manager.key` (privada), `roblox-account-manager.key.pub` (publica) e `roblox-account-manager.password.txt` (a senha dela). O `.gitignore` barra `*.key`. **Perder a chave ou a senha = nao conseguir mais publicar atualizacao para quem ja instalou** — a saida seria distribuir um instalador novo a mao. ⚠️ A chave **precisa** de senha: gerada com `--password ""`, o `tauri signer` produz um arquivo que ele mesmo depois recusa com "Wrong password for that key" (verificado em 26/09/2026, tanto pela variavel `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` quanto pelo `-p`). Por isso a senha aleatoria guardada ao lado da chave.
4. **Segredos que o repositorio precisa** (Settings › Secrets and variables › Actions): `TAURI_SIGNING_PRIVATE_KEY` (conteudo do arquivo `.key`), `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (conteudo do `.password.txt`) e `TAURI_SIGNING_PUBLIC_KEY` (conteudo do `.key.pub`, que o workflow injeta no `tauri.conf.json` antes de compilar).
5. **Build local**: sem `TAURI_SIGNING_PRIVATE_KEY` no ambiente, `bun run tauri build` grava o instalador e **depois** sai com erro na assinatura do artefato de update. Desde 03/10/2026 o `bundle.targets` do [tauri.conf.json](../src-tauri/tauri.conf.json) é só `msi`, o único instalador publicado: o build local grava o `bundle/msi/*.msi` (antes era só o NSIS). Se o `light.exe` falhar com ICE38 citando `roblox_account_manager_lib.dll`, é uma DLL de outro build sobrando em `target/release/` — tire-a e rode de novo. Ver a regra de build no [CLAUDE.md](../CLAUDE.md).
6. **Instalacao silenciosa, com a tela do proprio app** (03/10/2026): `plugins.updater.windows.installMode` e `quiet` no [tauri.conf.json](../src-tauri/tauri.conf.json). Antes era o padrao (`passive`), e o "Install & Restart" abria a janela nativa do MSI com uma barra de progresso, que parecia o app "baixando de novo" depois do download. O plugin **fecha o app** antes de chamar o instalador (`std::process::exit`), entao o app nao tem como desenhar nada durante a instalacao. O que existe no lugar:
   - o download avisa a tela aos poucos (`update-download-progress`, no maximo a cada 100 ms, `DownloadProgressThrottle`). Ate aqui o callback era vazio e a barra ficava parada ate o fim;
   - a fase "Installing" do [UpdateDialog](../src/components/dialogs/UpdateDialog.tsx) fica 1,5 s na tela (`INSTALL_HANDOFF_DELAY_MS`) com "o RAM vai fechar e abrir de novo sozinho", e so entao instala;
   - antes de instalar, o app anota de qual versao saiu e para qual vai ([updateHandoff.ts](../src/updateHandoff.ts), `localStorage`). Ao abrir, [useUpdateHandoffToast](../src/hooks/useUpdateHandoffToast.ts) le a anotacao uma vez: voltou na versao nova = "Updated to vX ✨"; voltou na antiga = "a atualizacao nao terminou";
   - sem janela, um erro de instalacao tambem nao aparece. O MSI grava um log detalhado em `RAMUpdateInstall.log`, na pasta de dados (`update_installer_args`; o NSIS nao entende `/l*v` e nao recebe nada). O app volta sozinho no fim porque o updater manda `AUTOLAUNCHAPP=True` (MSI) ou `/R` (NSIS).
   - Fica um intervalo de alguns segundos sem nada na tela entre o app fechar e reabrir. Cobrir isso pediria uma janela separada rodando durante a instalacao, e o instalador nao pode trocar um arquivo em uso.

**Numero da versao:** a serie e o major do `package.json` — hoje `0.x`, porque o app ainda esta antes da primeira versao completamente corrigida (as releases `v4.x` eram testes e carregavam o numero herdado do projeto original). O workflow **nao** commita o numero de volta: o `package.json` fica parado e cada release soma um patch a partir da tag mais alta da serie (`[bump:minor]` na mensagem do commit, ou o rotulo `bump:minor` no PR, soma um minor). Para escolher um numero — a 1.0.0, quando chegar a hora —, ponha-o no `package.json`, no `tauri.conf.json` e no `Cargo.toml`: se ele for maior que todas as tags da serie, a release sai exatamente com ele. Regra em [release-version.mjs](../.github/scripts/release-version.mjs), testada na suite `release` (`bun run t release`). ⚠️ O updater so troca por versao **maior**: quem tem uma `4.x` instalada nao recebe a `0.x` sozinho — instala a `0.x` por cima, uma vez.

**Quais arquivos a release publica:** o **MSI** das duas variantes, mais as assinaturas (os arquivos do updater), e uma copia do MSI padrao com nome fixo, `MultiAlt-Setup.msi` (`Roblox-Account-Manager-Setup.msi` antes da troca de nome) — e para ela que o botao de download do README aponta, por `/releases/latest/download/`. Por isso a release **nao** sai como pre-lancamento (`prerelease: false`): o GitHub nao conta pre-lancamento como "latest", e o link quebraria. O canal (beta/stable) continua no titulo, no texto e no manifesto do updater. **O setup NSIS (`.exe`) saiu em 03/10/2026** (decisao do dono: levava marcacao heuristica — ver a tabela abaixo); quem instalou por ele deixou de receber atualizacao, e o README orienta a instalar o MSI uma vez (a desinstalacao do NSIS nao apaga a pasta de dados). **O portatil tambem saiu da release em 03/10/2026** (decisao do dono: o `.exe` solto leva 1/75 de um motor de ML — `Wacatac.B!ml`, Microsoft, na v0.1.3 — enquanto o MSI sai 0/75; ver a tabela abaixo). O codigo dele nao foi apagado: fica atras do interruptor `PUBLISH_PORTABLE: "false"` no `env` do job `release` ([release-v4.yml](../.github/workflows/release-v4.yml)). **Para religar, troque para `"true"`** (e o teste que trava o padrao, [release-workflow.test.mjs](../.github/scripts/release-workflow.test.mjs), suite `release`): copia, renomeio, upload e a linha do guia da release ja checam o interruptor, e o site mostra a linha "Portable" da tabela de downloads sozinho quando o arquivo existe na release. O **portatil** e o `.exe` solto (sem instalar): sem admin, mas nao passa pelo updater e nao cria atalho no Menu Iniciar. **Texto da release (passo "Finalize release notes"), para quem nao e tecnico** (pedido do dono, 03/10/2026): abre com o link do `MultiAlt-Setup.msi` e tres frases curtas; os outros arquivos ficam num bloco recolhido (o portatil so aparece com o interruptor ligado). O "## What's Changed" vem da secao **`## What's new`** do PR da `develop` para a `main` — escreva ali, em frases curtas e simples, o que muda para quem usa (sem nome de arquivo, sem jargao). A lista automatica do GitHub (titulos dos PRs) fica num bloco recolhido "Technical details"; PR sem a secao cai na lista automatica. A janela de atualizacao do app (`notesForUpdateDialog` em `releaseNotes.ts`) tira o bloco de download e os blocos recolhidos; a pagina "What's new" do app (`changelogNotes`, mesmo arquivo) mostra so o que esta sob o titulo `## What's Changed` — por isso o titulo fica, mesmo com a lista simples (ver [ui-layout.md](features/ui-layout.md#página-whats-new-changelogpage)).

**Dependencias saem otimizadas mesmo em debug** (`[profile.dev.package."*"] opt-level = 3` no [Cargo.toml](../src-tauri/Cargo.toml)). Motivo medido em 28/09/2026: o Argon2 em Rust puro leva **5,4 s por derivacao sem otimizacao** contra **0,3 s com ela** — 17x. Como `cargo test` roda em debug, a suite Rust levava 18 dos 24 minutos do CI. Com a mudanca: **230 s -> 41 s** local, mais rapido ate do que era com o libsodium. O nosso codigo segue sem otimizacao (compila rapido, debug bom). O libsodium nao sofria disso por ser C pre-compilado; qualquer cripto em Rust puro sofre.

⚠️ O primeiro build depois desta mudanca (ou apos limpar o cache) e **mais lento**, porque compila todas as dependencias otimizadas. O `Swatinem/rust-cache` guarda isso e os seguintes ganham.

**O MSI e per-user: sem admin, e com um template WiX proprio.** O MSI do Tauri e per-machine por padrao (instala em `Program Files`, pede elevacao ao instalar **e a cada atualizacao**). Desde 28/09/2026 o projeto usa um template proprio, [src-tauri/wix-peruser.wxs](../src-tauri/wix-peruser.wxs), apontado por `bundle.windows.wix.template` no [tauri.conf.json](../src-tauri/tauri.conf.json). Resultado medido: **0/75 no VirusTotal, limpo no Defender, sem prompt de admin e com auto-update funcionando** — as tres coisas que nenhum formato entregava junto.

O template saiu do oficial do Tauri **2.10.0** (`crates/tauri-bundler/src/bundle/windows/msi/main.wxs`, tag `tauri-cli-v2.10.0`) com **oito** mudancas, cada uma por um motivo concreto:

| # | Mudanca | Por que |
|---|---|---|
| 1 | `InstallScope` `perMachine` → `perUser` | e o que tira a elevacao |
| 2 | Diretorio `ProgramFiles` → `LocalAppDataFolder\Programs\<produto>` | per-user nao pode escrever em `Program Files` |
| 3 | Componente do binario: `File KeyPath="no"` + `RegistryValue` HKCU `KeyPath="yes"` | **ICE38**: componente que instala no perfil do usuario precisa de ancora HKCU, nao arquivo |
| 4 | `RemoveFolder` da `ProgramsFolder` no uninstall | **ICE64**: pasta no perfil precisa sair na desinstalacao |
| 5 | Removida a busca `PrevInstallDir*` que reusava o diretorio de uma instalacao anterior | uma instalacao **per-machine** anterior deixa `C:\Program Files\...` em `HKCU`; o MSI per-user tentava escrever la e falhava com *"Error writing to file / Verify that you have access to that directory"*. Num instalador per-user o diretorio e fixo, nunca herdado |
| 6 | `<CreateShortcuts>NOT WIX_UPGRADE_DETECTED</CreateShortcuts>` | na atualizacao o MSI **reescrevia** o atalho da area de trabalho: o `.lnk` virava outro arquivo e o Windows jogava o icone para outra posicao. O template oficial ja pulava o `RemoveShortcuts` no upgrade, mas recriava no `CreateShortcuts`. Medido em 03/10/2026 instalando por cima de verdade: o ID do arquivo do atalho (`fsutil file queryfileid`) mudava a cada atualizacao e, com a condicao, ficou o mesmo. Igual ao NSIS (`/UPDATE` nao mexe em atalho): quem apagou o atalho nao o ganha de volta numa atualizacao |
| 7 | Propriedade `LEGACYNAMEINSTALLED`, componente `LegacyNameCleanup` e `OR LEGACYNAMEINSTALLED` no `CreateShortcuts` | troca de nome para MultiAlt (03/10/2026): vindo do nome antigo, a versao velha sai **sem** apagar os proprios atalhos (o `RemoveShortcuts` dela pula na atualizacao) e a nova pulava a criacao (mudanca 6) — sobrava um `Roblox Account Manager.lnk` quebrado e nenhum atalho novo. Agora, quando acha a chave `HKCU\Software\<fabricante>\Roblox Account Manager`, o MSI apaga os `.lnk` e as pastas do nome antigo e cria os novos. Testado atualizando de verdade (03/10/2026): dados intactos (hash dos arquivos da pasta de dados igual), atalhos novos validos, pasta antiga removida, um produto so em Aplicativos; a chave antiga sai junto, entao nas atualizacoes seguintes vale de novo a mudanca 6. O codigo de atualizacao fica fixo em `bundle.windows.wix.upgradeCode`: o Tauri o calcula a partir do nome do produto. Ver [rebrand-multialt.md](rebrand-multialt.md) |
| 8 | Atalho do Menu Iniciar **sem** `Icon="ProductIcon"` | o `ProductIcon` fica em `%APPDATA%\Microsoft\Installer\{codigo do produto}`, e o codigo muda a cada versao; como a atualizacao mantem os atalhos (mudanca 6), o atalho ficava apontando para um icone apagado e a barra de tarefas — que usa este atalho pelo `AppUserModel.ID` — mostrava o icone generico (visto na 0.1.9 → 0.1.10). Sem `Icon`, o Windows tira o icone do exe. Quem ja tem o atalho quebrado continua com ele ate reinstalar (o upgrade nao recria atalhos); o teste fica em `src/appIdentity.test.ts` |

As quatro primeiras foram pegas por validacao (o `light.exe` recusa) — a quinta e a sexta so apareceram **instalando de verdade** (a quinta numa maquina que ja tinha a versao per-machine; a sexta atualizando por cima). Scan nenhum pegaria.

⚠️ **Manutencao:** este template esta preso ao Tauri **2.10.0**. Quando o Tauri subir de versao, o template oficial pode mudar e o nosso fica para tras **em silencio** (o build continua passando). Ao atualizar o Tauri: baixar o `main.wxs` da tag nova, comparar com o nosso, reaplicar as oito mudancas, e **testar a instalacao de verdade** — nao so o build.

⚠️ **Troca de per-machine para per-user:** o Windows trata os dois como apps diferentes. Quem tem a versao antiga instalada precisa desinstala-la antes, senao fica com duas copias. O valor `InstallDir` em `HKCU\Software\<fabricante>\<produto>` sobrevive a desinstalacao e deve ser apagado junto.

**O MSI e o download recomendado**, porque e o unico que junta as tres coisas: limpo, sem admin e com auto-update.

**Medicao dos arquivos publicados na v0.1.3-beta** (os tres baixados da release e escaneados nos dois motores):

| Arquivo | VirusTotal | Defender local |
|---|---|---|
| **MSI** (recomendado) | **0/75** | limpo |
| setup NSIS | 2/75 (APEX + Sophos, os dois de ML generico) | limpo |
| portatil | 1/75 (`Wacatac.B!ml`, Microsoft) | limpo |

Duas licoes desta medicao, que contrariam o que se supunha antes:

1. **O motor da Microsoft no VirusTotal e mais severo que o Defender local.** O portatil passou limpo no `MpCmdRun` da maquina e mesmo assim levou `Wacatac.B!ml` no VirusTotal. O que o usuario ve no dia a dia e o Defender local — mas o numero que ele ve no VirusTotal e outro. Reportar sempre os dois.
2. **A mesma fonte com flags diferentes recebe veredito diferente.** O portatil publicado e a variante `--no-default-features`; o build local com as features completas deu 0/75. Binarios quase iguais, veredito diferente do mesmo modelo. Perseguir esses numeros e alvo movel: o que da para fazer e manter o **download recomendado** limpo e medir a cada release.

**Historico do falso positivo (tudo medido, 27-28/09/2026):**

| Arquivo | Antes | Depois |
|---|---|---|
| `.exe` do app | 1/75 `Trojan:Win32/Wacatac.B!ml` (Microsoft) | **0/75**, Defender limpo |
| MSI | 0/61, mas pedia admin | **0/75**, sem admin |
| Setup NSIS | 3/71 | 1/75 (so o APEX, no empacotador) |

Duas causas separadas, achadas por bisseccao com `bun run vt`:

1. **A marcacao da Microsoft vinha da criptografia**, nao do resto do app: o `sodiumoxide` embute a biblioteca C **libsodium** no binario, e esse blob e o padrao nº 1 que modelos de ML associam a ransomware. O commit `2b3b660` virava 0/75 em 1/75. Resolvido trocando por criptografia em **Rust puro** (RustCrypto), no mesmo formato — ver [features/accounts.md](features/accounts.md). **Nao foi preciso remover a criptografia nem assinar o codigo.**
2. **O 1/75 que sobra no NSIS e do empacotador**, nao do nosso codigo (o `.exe` de dentro e 0/75). E um motor de ML obscuro (APEX) com rotulo generico. Perseguir isso e alvo movel; o MSI sai 0/75 e cobre quem se incomoda.

Tambem aplicado: `webviewInstallMode: embedBootstrapper` no [tauri.conf.json](../src-tauri/tauri.conf.json), que tirou o download-e-executa da instalacao (o NSIS caiu de 3/71 para 1/71 sozinho com isso).

**Assinatura de codigo (Authenticode) foi avaliada e descartada** a pedido do dono. Ela resolveria o aviso do SmartScreen ("Fornecedor desconhecido"), que **continua aparecendo** por reputacao zero — isso nao tem a ver com virus. O SignPath Foundation (gratuito para open source) foi analisado: alem do certificado sair em nome da fundacao e nao do dono, a clausula *"no hacking tools"* provavelmente barraria o app (ele fecha o mutex de instancia unica do Roblox e falsifica MachineGuid/MAC no isolamento).

**Conferir um build no VirusTotal:** `bun run vt <arquivo>` ([virustotal.ts](../scripts/virustotal.ts)) manda o arquivo e mostra quem marcou; `--rescan` forca uma analise nova com os motores de hoje, que e o jeito de separar "o binario mudou" de "o modelo do antivirus mudou". A chave da API sai de uma conta gratuita e mora **fora do repositorio**, em `%USERPROFILE%/.tauri/virustotal.key` (ou na variavel `VT_API_KEY`). Conta gratuita aceita 4 pedidos por minuto e 500 por dia. Subir um arquivo ao VirusTotal o compartilha com os antivirus parceiros — vale para instalador publicado, nao para arquivo com dado do usuario.

**`bun run scan` termina com um resumo por arquivo** (Defender e VirusTotal: `limpo`, `MARCOU` ou `inconclusivo`) e sai com 0 so quando os dois motores rodaram limpos em todo arquivo; 2 = algum marcou, 1 = algum motor nao rodou. Motor que nao rodou **nunca** conta como limpo: ate 29/09/2026 o resumo dizia "tudo limpo" com o script do Defender quebrado (travessao UTF-8 num `.ps1` sem BOM, que o PowerShell 5.1 le na pagina de codigo do sistema) e com o setup NSIS marcado 1/75 — o `virustotal.ts` saia 0 em qualquer veredito. Travado em [scanVerdict.test.ts](../scripts/scanVerdict.test.ts) (suite `scan`), que tambem exige o `defender-scan.ps1` so em ASCII.

**O que ja foi medido (28/09/2026):** comparando o build limpo de 25/09 com o marcado de 28/09 pelo `pefile`, o perfil de comportamento e **o mesmo**: as duas tem as mesmas 16 APIs que pesam em heuristica (`SendInput`, `SetForegroundWindow`, `GetAsyncKeyState`, `TerminateProcess`, `NtQuerySystemInformation`, `DuplicateHandle`, `RegSetValueExW`…), mesma entropia por secao, mesmo linker. As unicas importacoes novas sao `FlushFileBuffers`, `OpenEventW`, `RemoveDirectoryW` e `MessageBoxW`. Ou seja: **nao ha o que "consertar" no binario** — o `Trojan:Win32/Wacatac.B!ml` e um veredito de modelo de aprendizado de maquina (o sufixo `!ml`), que muda sozinho ao longo do tempo. O conserto duravel e assinatura de codigo (Authenticode); ha opcao gratuita para projetos open source (SignPath Foundation).

⚠️ Detalhe do manifesto do updater: o app procura primeiro a chave do **proprio formato** (`windows-x86_64-msi`, conferido no tauri-plugin-updater 2.10) e so depois a generica `windows-x86_64`. Desde 03/10/2026 o manifesto tem **so** a do MSI ([update-manifest-platforms.mjs](../.github/scripts/update-manifest-platforms.mjs), aplicado pelo `generate-update-manifest.mjs`, suite `release`): a generica seria a de um app de formato desconhecido — o portatil, ou um NSIS antigo — e mandar o MSI para ele instalaria uma segunda copia ao lado da atual. Ate 03/10/2026 a generica apontava para o setup `.exe`.

O endereco do projeto vive em um lugar por lado — [src/repo.ts](../src/repo.ts) e o `REPO_URL` de [services.rs](../src-tauri/src/commands/services.rs) — e [repoOwnership.test.ts](../src/repoOwnership.test.ts) varre `src/`, `src-tauri/src/` e `.github/` reprovando qualquer volta do endereco antigo.

### Piso de legibilidade do texto

Nada de interface abaixo de **11px**, e prosa (descrição, dica, ajuda) em **12px**. O app tem tamanho em pixel absoluto espalhado pelo JSX (`text-[11px]`, `text-[12px]`…), então **não existe alavanca global**: mudar a fonte do `:root` não mexe em nada, e `zoom` no `body` quebraria os menus de contexto, que posicionam por `clientX/clientY`. A escala pequena foi subida de uma vez (9 e 10 → 11, 11 → 12) a pedido do dono, que não conseguia ler certas descrições sem se aproximar da tela.

[textSize.test.ts](../src/components/ui/textSize.test.ts) varre o `src/` e reprova qualquer `text-[<11px]` novo — bloco antigo copiado com `text-[10px]` é pego ali, não meses depois na tela de alguém.

Texto maior cabe menos: ao mexer nisso, meça a tela. Duas colunas de largura fixa precisaram crescer junto (o `step` do console, 104 → 112px, e o Job ID da aba Servers, 250 → 264px), senão passavam a cortar conteúdo que antes cabia inteiro.

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

Os arquivos de dados (`AccountData.json`, `AccountData.key`, `RAMSettings.ini`, `RAMScripts.json`, ...) ficam na **pasta de dados do usuário** — `%LOCALAPPDATA%/Roblox Account Manager` no Windows (ver [architecture.md](architecture.md#arquivos-de-persistência)). Isso vale também em `tauri dev`: a mesma pasta do app instalado, **com as contas reais**. Não existe pasta separada para debug.

Para testar do zero **sem tocar nos seus dados reais**, aponte outra pasta:

```bash
# PowerShell
$env:RAM_DATA_DIR = "$env:TEMP/ram-dev"; bun run tauri dev
```

`RAMSettings.ini` é recriado com defaults e, como não existia, `EncryptionOnboardingState` e `FirstRunWalkthroughState` ficam `pending` (o app abre o onboarding).

O `RAM_DATA_DIR` não isola tudo: o catálogo de versões (`RAMVersions.json`, `RobloxVersions/`) e o `IsolationBackup/` continuam em `%LOCALAPPDATA%/Roblox Account Manager`, e o Roblox em si (registro, instalação, clientes) é o da máquina.

## i18n

- Configuração em [src/i18n/index.ts](../src/i18n/index.ts): idiomas suportados `en`, `de`, `pt` (português do Brasil) e `es` (espanhol neutro da América Latina), fallback `en`, `keySeparator: false` e `nsSeparator: false` — **a chave é a própria frase em inglês**.
- Arquivos: [en](../src/locales/en/common.json) (a fonte; 1695 chaves em 27/09/2026 — o número sobe a cada `i18n:extract`), [pt](../src/locales/pt/common.json) (completo), [es](../src/locales/es/common.json) (completo) e [de](../src/locales/de/common.json) (parcial — o que falta cai no inglês).
- Helpers em [src/i18n/text.ts](../src/i18n/text.ts): `useTr()` (hook), `tr()` (fora de componentes) e `trNode()` (traduz texto dentro de fragments JSX). Ambos usam `defaultValue: text`, então uma chave ausente aparece em inglês.
- Idioma vem de `General.Language` (normalizado: começa com `de` → `de`; `pt`/`portug` → `pt`; `es`/`spanish` → `es`; senão `en`). O padrão continua `en` — não há detecção de locale do sistema, de propósito: o app é usado fora do Brasil.
- [src/i18n/locales.test.ts](../src/i18n/locales.test.ts) trava o contrato do catálogo: cada idioma completo (`pt` e `es`, lista `COMPLETE_CATALOGS`) cobre o `en` inteiro na mesma ordem, sem chave inventada nem valor vazio, e nada igual ao inglês fora da sua lista de jargão (`IDENTICAL_BY_DESIGN_PT`, `IDENTICAL_BY_DESIGN_ES`); `{{placeholders}}` idênticos em `pt`, `es` e `de`. Idioma completo novo entra em `COMPLETE_CATALOGS` com a sua própria lista.

### Glossário pt-BR

- **Botão = infinitivo** ("Adicionar", "Salvar"); **resultado = particípio** ("Conta adicionada", "Job ID copiado"); só a primeira maiúscula em rótulo; tratamento "você".
- **Não se traduz**: `Roblox`, `Job ID`, `Place ID`, `Universe ID`, `Cookie`, `Fast Flags`, `Web Server`, `Auto Rejoin`, `Nexus`, `Watcher`, `main`, `alt`, `place`, `job`, `loop`, `rejoin`, nome de arquivo/caminho/URL/código, nome de tema e de fonte.
- Termos fixos: account → conta · launch → iniciar · settings → configurações · aged/idle → sem uso · Main Accounts → Contas main · asset → item · General/Developer/Optimization/Misc/Isolation → Geral/Desenvolvedor/Otimização/Diversos/Isolamento.
- Rótulo curto (<20 caracteres no inglês) não passa de +30% em português: trunca na tela.

### Glossário es

- Espanhol neutro da América Latina (o público do Roblox), tratamento **"tú"** — nunca voseo nem "vosotros"; vocabulário latino (computadora, archivo, hacer clic).
- Mesma regra de botão/resultado do pt: "Guardar" / "Configuración guardada"; só a primeira maiúscula em rótulo.
- **Não se traduz** o mesmo que no pt (`Auto Rejoin`, `Job ID`, `Place ID`, `Nexus`, `Watcher`, `main`, `alt`, nome de tema e de fonte...).
- Termos fixos: account → cuenta · launch → iniciar · alias → apodo · settings → configuración · isolation → aislamiento · backup → copia de seguridad · encryption → cifrado.
- Tom do toast: a frase de erro precisa de "error", "falló", "fallido/a" ou "no se pudo"; a de sucesso, "guardad", "actualizad" ou "iniciad"; a de aviso, "advertencia" ou "aviso" (ver [toastTone.ts](../src/utils/toastTone.ts)).

### Tradução não pode mudar comportamento

O texto exibido nunca é valor de negócio: `<Select>` guarda `value` cru (`"idle"`, `"normal"`) e traduz só o `label`; nome de grupo, fase do Auto Rejoin e seção/chave do INI são comparados no literal inglês. Ao mexer em tradução, mantenha isso.

O caso que já morde: `addToast` deduz o tom da mensagem pelo texto, e a maioria dos call sites entrega a frase **já traduzida** (`addToast(tr("..."))`). Por isso o heurístico vive em [src/utils/toastTone.ts](../src/utils/toastTone.ts) com marcadores de cada idioma (`pt`, `es` e `de`), e um teste garante que nenhuma tradução apague o tom que o inglês indica.

### Extração de chaves

`bun scripts/i18n/extract-keys.ts` ([extract-keys.ts](../scripts/i18n/extract-keys.ts)):

1. Varre `src/**/*.ts(x)` (ignora pastas `locales` e `i18n`).
2. Captura strings em `t("...")`/`tr("...")`, props `label|description|placeholder|suffix|title|tooltip|alt|aria-label="..."`, objetos `{ label: "..." }`, fragments `label={<>Texto<Badge/></>}` e filhos de `SectionLabel`, `SectionHeader`, `WarningBadge`, `UtilButton`.
3. Descarta strings que parecem URLs, caminhos, hashes, IDs numéricos, template strings (`${`) ou sem letras.
4. Adiciona as chaves faltantes em `en/common.json` com valor = chave. **Não remove** chaves antigas e **não mexe** em `de`/`pt`/`es`.

O extrator já reconhece, além do literal direto: componente com atributos
(`<UtilButton onClick={...}>Texto</UtilButton>`), prop com expressão
(`description={cond ? "A" : "B"}`) e ramo de ternário dentro da chamada
(`t(cond ? "A" : "B")` — só o que vem depois de `?`, `:` ou `||`, porque o
literal de comparação é identificador interno). Uma frase que **cita** uma URL
também conta: o filtro só descarta a string que é URL inteira.

O extrator **não varre arquivo de teste**: fixture como
`<MenuItemView item={{ label: "First" }}>` virava chave no catálogo como se
alguém fosse traduzir.

O que ele ainda não pode ver é chave montada em variável (`t(cat)` com `cat`
vindo de um array, `t(status)`): essas entram no catálogo à mão. Se a sua string
não aparece traduzida, confira primeiro se a chave existe em
`src/locales/en/common.json` — `t()` sem chave devolve o inglês em silêncio, em
todos os idiomas. [src/i18n/reachesTheScreen.test.tsx](../src/i18n/reachesTheScreen.test.tsx)
renderiza em português os pontos que já falharam assim.

As traduções ficam no próprio repositório: `src/locales/<idioma>/common.json`, com o inglês como fonte. O português e o espanhol acompanham cada mudança — o `locales.test.ts` exige os catálogos pt e es com todas as chaves do inglês, na mesma ordem e sem chave a mais —, e o alemão é parcial. O Crowdin que o projeto original usava (workflow `crowdin-sync.yml` e `crowdin.yml`) foi removido em 28/09/2026: este repositório não tem projeto lá, e o workflow falhava em todo push na `main`.

Regra prática: sempre escreva textos de UI via `t(...)`/`tr(...)` ou numa das props reconhecidas, com interpolação no formato `{{nome}}` (nunca template string), e rode o extrator.

## Escape, foco e teclado

- **`Esc` passa por uma pilha LIFO** ([useEscapeStack.ts](../src/hooks/useEscapeStack.ts)): existe **um** listener em `window`, e só o topo da pilha recebe a tecla. Quem monta depois fica no topo — popover sobre diálogo sobre tela —, então a ordem de registro não importa mais. Antes eram 26 handlers em 23 arquivos, nenhum interrompendo a propagação: um `Esc` fechava o diálogo **e** a tela de trás.
- **Diálogo não escreve handler de `Esc`:** use [useModalClose](../src/hooks/useModalClose.ts), que já resolve isso. Se precisar agir antes de fechar (o editor de temas reverte a pré-visualização), passe `onEscape` no 4º parâmetro.
- **Fundo de modal não usa `onClick` cru:** use [useBackdropClose](../src/hooks/useBackdropClose.ts) (`<div {...backdropClose}>`), que só fecha quando o botão desce **e** sobe no fundo. Com `onClick`, selecionar texto num campo arrastando para fora fechava o modal. A guarda em `useBackdropClose.test.tsx` varre `src/`.
- **Handler de campo que trata `Esc` chama `preventDefault()`** — é o sinal de "já tratei" que a pilha respeita. Sem isso, reverter um campo numérico fechava o diálogo junto.
- **Controle customizado precisa de semântica**: `role` + `aria-*` + `tabIndex` + teclado. O `Toggle` é o exemplo — um `<div onClick>` ali deixava as 9 telas de Settings inoperáveis por teclado. E todo controle novo precisa de foco visível; o anel usa `var(--input-focus)`.

## Convenções

- **TypeScript estrito**: [tsconfig.json](../tsconfig.json) com `strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`; target ES2020. `bun run build` falha com variáveis não usadas.
- **Estado global** em um único Context ([store.tsx](../src/store.tsx)); componentes usam `useStore()`. Diálogos são abertos por flags na store (`setSettingsOpen`, `setServerListOpen`, ...).
- **Settings no frontend**: leia com `store.settings?.Section?.Key` (sempre string) ou, em telas de configuração, com o hook [useSettings.ts](../src/hooks/useSettings.ts) (`get/getBool/getNumber/set`), que agrupa gravações com debounce de 160 ms.
- **Erros** do backend são `String`; no frontend vão para `store.setError` (faixa vermelha em [App.tsx](../src/App.tsx)) ou `store.addToast`.
- **Commits** majoritariamente em português, curtos (ex.: `juste de tempo no join`, `multiplos servidores vips`); também há commits em inglês. Mensagens do log de launch em Rust também estão em português (ex.: `"Alvo resolvido: servidor privado/VIP"` em [launch.rs](../src-tauri/src/commands/launch.rs)).
- **PRs**: [scripts/pr-flow.ps1](../scripts/pr-flow.ps1) automatiza o fluxo com `gh`, com base default `develop` (onde o trabalho vive; a `main` é só release). E PR só com pedido do dono (regra de Git do [CLAUDE.md](../CLAUDE.md)).

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
   - Se chamar a API do Roblox com o cookie da conta **para ler** (ou para uma ação não crítica), use `read_without_refresh(state.inner(), user_id, |cookie| async move { ... })` ([account_api.rs](../src-tauri/src/commands/account_api.rs)): pega o cookie e chama a API direto, e cookie vencido vira erro na tela. **Não use `run_with_session_retry`** (nem `refresh_account_session`) nesse caso — é regra crítica do `CLAUDE.md`: no primeiro 401 o refresh chama `signoutfromallsessionsandreauthenticate`, que desloga a conta de **todas** as sessões e derruba os clientes Roblox abertos dela. O retry fica para o caminho crítico que já o usa (auth ticket e private join do launch e do Auto Rejoin) e para ações que a pessoa pediu explicitamente naquela conta, aceitando esse custo. Comando de leitura novo em `account_api.rs` entra na lista `LEITURAS` do teste `read_only_retry_tests`, que reprova se ele passar a renovar sessão (ver [authentication.md](features/authentication.md#regras-de-negócio)).
   - Para progresso/notificações assíncronas, receba `app: tauri::AppHandle` e use `app.emit("meu-evento", payload)`.
   - Código específico de SO: use `#[cfg(target_os = "windows")]` e forneça um caminho `#[cfg(not(target_os = "windows"))]` que retorne erro, como em [diagnostics.rs](../src-tauri/src/commands/diagnostics.rs). Código dependente de feature: crie a versão `#[cfg(not(feature = "..."))]`, como em [services.rs](../src-tauri/src/commands/services.rs).

3. **Registre** o nome em `tauri::generate_handler![...]` em [lib.rs](../src-tauri/src/lib.rs). Sem isso o `invoke` falha em runtime (não em compilação).

4. **Chame no frontend** com argumentos em camelCase:

   ```ts
   const note = await invoke<string>("get_account_note", { userId });
   ```

   Normalmente isso vira uma função na [store.tsx](../src/store.tsx) ou fica no componente que usa. Tipos compartilhados vão em [types.ts](../src/types.ts).

5. **Eventos**: se emitiu evento, registre o `listen` num `useEffect` com cleanup e adicione-o na tabela de [architecture.md](architecture.md#eventos-backend--frontend). Como `listen()` é assíncrono, use o padrão com flag `disposed` (como em [store.tsx](../src/store.tsx) e [NexusPage.tsx](../src/components/pages/NexusPage.tsx)): se o efeito já foi desmontado quando a promise resolver, chame o `unlisten` na hora em vez de guardá-lo — senão o listener vaza. Dentro de handlers de longa duração, leia estado via `ref` (ex.: `accountsRef`) para não usar valores velhos capturados no closure.

6. **Opcional — expor para scripts**: se scripts do usuário devem poder chamar o comando, adicione-o a `SCRIPT_INVOKE_COMMANDS` em [ScriptsPage.tsx](../src/components/pages/ScriptsPage.tsx) (allowlist; ver [scripts.md](features/scripts.md)).

7. **Textos novos** na UI → `t("...")` + `bun run i18n:extract`.

8. **Verifique**: `bun run build`, `cargo check` e `cargo check --no-default-features`.
