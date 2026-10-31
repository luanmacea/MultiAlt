# Linux (branch `linux`, pausado em 31/10/2026)

Trabalho isolado na branch `linux` desde 31/10/2026 (pedido do dono): nada
daqui vai para a `develop` até funcionar de verdade num Linux.

## O problema

O Roblox não tem cliente para Linux, e o Wine é bloqueado pelo anti-cheat
desde 2023. O que roda hoje é a versão de **Android** do Roblox, por um destes:

- **Sober** (`org.vinegarhq.Sober`, Flatpak, código fechado) — o mais usado;
- **Cordial** / **Stacked** (GPL, código aberto) — mais novo.

## O que os concorrentes fazem (estudo de 31/10/2026, só leitura)

Clonados em `RAM-repositorios-diversos/` (nunca executar nada de lá; a ideia
é reescrita aqui, não copiada).

### broreirik/RAM-Linux (C#, GPL, Sober)

- **Uma pasta por conta** (`~/.local/share/ram-linux/profiles/<conta>`),
  usada como `HOME` do Sober: o Flatpak dá a cada conta o seu
  `~/.var/app/org.vinegarhq.Sober`, com login e dados próprios.
- **Cookie é um arquivo**: `<HOME>/.var/app/org.vinegarhq.Sober/data/sober/cookies`,
  conteúdo `.ROBLOSECURITY=<valor>` (permissão 600). Escrever o cookie que o
  MultiAlt já guarda entra na conta, sem login no cliente.
- **Perfil novo copia** `data/sober/{packages,assets,exe,asset_overlay}`,
  `data/sober/state` e `config/sober/config.json` do Sober principal (cópia
  de verdade: hardlink trava com "another instance is already modifying").
- **Várias ao mesmo tempo**: o Sober recusa rodar duas vezes na mesma rede
  ("frozen instance"). Cada cliente roda em
  `unshare --user --map-current-user --net` e ganha internet pelo `pasta`
  (pacote `passt`): `pasta --config-net <pid>`. Com uid 0 o áudio quebra.
- **Comando**: `flatpak run --branch=stable --arch=x86_64 --command=sober org.vinegarhq.Sober -- <uri>`.
- **Entrar no jogo**: `roblox://experiences/start?placeId=P`
  (+ `&gameInstanceId=<job>`, ou `&linkCode=<código>` para servidor privado).
- Bug conhecido: com dois abertos, o mais novo às vezes fecha em 1–3 min
  (`SL_RESULT_SUCCESS`, áudio do Sober).

### DamnShabu/roblox-manager (Rust, MIT, Stacked/Cordial)

- Login por **Quick Login** (`auth-token-service/v1/login/create` → código
  em roblox.com/crossdevicelogin → `auth.roblox.com/v2/login` com
  `ctype: "AuthToken"`), cookie guardado no Secret Service.
- O cookie vai para o cliente pelo **Secret Service** no formato do Cordial
  (`org.cordial.Session`, itens `identity` e `cookies`), com
  `CORDIAL_SECRET_STORE=keyring`; perfil `--profile rbxmgr-<userId>`.
  Antes de abrir, confere `users/authenticated` contra o id da conta.
- Entra com `--join-url roblox://experiences/start?...`; **não** entra em
  servidor privado nem em "seguir usuário".
- FPS/qualidade por variáveis `CORDIAL_*`; esconde/mostra janela com
  SIGUSR1/SIGUSR2; não posiciona janela.
- Acompanha processo por `pgrep -a -f cordial-run` e `--profile`.

## Caminho escolhido

Começar pelo **Sober** (mais usuários, cookie por arquivo, servidor privado
funciona). Cordial depois, como segunda opção.

1. Fazer o backend **compilar** no Linux (workflow `linux-check.yml`, só
   nesta branch).
2. `platform/linux/`: perfil por conta, gravar o cookie, abrir o Sober com
   `unshare` + `pasta`, deep link do jogo, rastrear e fechar o processo.
3. Tela: esconder o que é só do Windows (grade, Live, AFK, isolamento,
   versões, Multi Roblox).
4. Pacote (AppImage/deb) e teste num Linux de verdade com o dono.

O que fica de fora no começo: grade de janelas, Live, AFK, isolamento,
versões do Roblox, patch de settings, volume por cliente.

## Backend do Sober (fase 1, 31/10/2026)

Escrito, mas **nunca executado num Linux de verdade** (aqui só há Windows) e
ainda **não ligado** ao `commands/launch.rs`. A decisão é toda pura e testada no
Windows; a camada de SO é fina.

### `src-tauri/src/platform/sober.rs` (sem `cfg`, testes `sober_tests`, suíte `linux`)

- **Caminhos**: `profile_dir(<dados>, userId)` = `<dados>/linux-profiles/<userId>`
  (id <= 0 é recusado); `sober_paths(home)` dá `.var/app/org.vinegarhq.Sober/`
  com `data/sober/cookies`, `config/sober/config.json` e `data/sober/sober_logs`.
- **Cookie**: `cookie_file_content` -> `.ROBLOSECURITY=<valor>` (aceita o valor
  puro ou já com o nome; recusa vazio, espaço, `;` e quebra de linha);
  `write_cookie` grava por arquivo temporário + rename, 0600 já na criação.
- **Perfil novo**: `COPY_ENTRIES` (packages, assets, exe, asset_overlay, state,
  config.json) copiados de verdade do Sober principal, para `<destino>.partial`
  e renomeados; só copia o que o perfil ainda não tem (estado da conta nunca é
  sobrescrito). O `cookies` do principal **nunca** entra. Symlink da origem é
  pulado, não seguido. `ensure_profile`, `remove_profile`.
- **Segurança**: tudo que escreve/apaga passa por `ensure_safe_path(raiz, caminho)`
  (dentro de `linux-profiles`, sem `..`, sem symlink em nenhum pedaço);
  `remove_inside` é a única função que apaga e recusa a própria raiz. Do HOME
  real só lê.
- **Deep link** (`DeepLink::from_launch`): `roblox://experiences/start?placeId=P`,
  `&gameInstanceId=<job>`, ou `&linkCode=<código>` (servidor privado; o job é
  ignorado), `&launchData=`. Recebe os campos **já resolvidos** por
  `resolve_launch_job` (`launch_shared.rs`), que tira o prefixo `vip:` e lê o
  código do link. Privado sem código cai no jogo comum com aviso.
- **argv**: `sober_command`, `isolated_command` (`unshare --user
  --map-current-user --net -- sh -c <WAIT_FOR_ROUTE_SCRIPT> sh <comando>`, sem
  `--fork`: o pid é o mesmo de `unshare` a cliente), `pasta_command(pid)`.
  O script espera até 15 s pela rota padrão e abre mesmo assim (regra nº 1).
- **Ambiente** (`sober_env_plan`): `HOME` = perfil; `FLATPAK_USER_DIR` fixado no
  real (senão o perfil não enxerga o Sober instalado por usuário); `XAUTHORITY`
  do HOME real quando faltar; `XDG_*_HOME` removidos.
- **Settings `[Linux]`** (`plan_runner`): `PreferredRunner` (`sober` | `custom`;
  nome desconhecido ou `custom` sem comando cai no Sober com aviso, nunca
  recusa), `CustomLaunchCommand` (separado em argumentos sem shell; `{uri}`,
  `{profile}`, `{user}`; sem `{uri}` o link vai no fim; roda com o HOME real e
  `MULTIALT_USER_ID`/`MULTIALT_PROFILE_DIR`, sem rede isolada), `CustomProcessMatch`
  (`process_matches`) e `CustomLogDir` (`client_log_dir`).
- **/proc**: `parse_proc_stat`, `process_tree`, `parse_netns_id`.
- **Diagnóstico**: `Diagnostics::problems()` diz o que instalar (flatpak, Sober
  via flathub, `util-linux`, `passt`).

### `src-tauri/src/platform/linux/` (`cfg(target_os = "linux")`)

`launch_account(&LaunchRequest)` (síncrono; chamar de `spawn_blocking`): perfil,
cookie, processo com `HOME=<perfil>` e log em `<perfil>/launch.log`, espera o
filho entrar no netns próprio (`/proc/<pid>/ns/net` diferente do app, 5 s),
`pasta --config-net <pid>`; se o `pasta` falhar ou não existir, mata a árvore
do filho e devolve erro mandando instalar o `passt`. Fechar uma conta
(`tracker().kill_for_user`) manda TERM e depois KILL só na árvore do processo
dela mais quem está no netns dela; nunca varre o sistema por nome.
Espelha o macOS: `tracker()`, `get_roblox_pids` (só o que o app abriu),
`kill_process`, `kill_all_roblox` (só os do app), `enable/disable_multi_roblox`
(no-op: o multi é o netns), `get_roblox_path`, `generate_browser_tracker_id`.
Extras: `diagnose()`, `host_env()`.

### Fase 2: ligado (31/10/2026)

Compila no Linux (`cargo check` nas duas edições e `--tests`, 0 erros) e os 54
testes `sober_tests` passam no Linux, inclusive os de symlink e de permissão
0600. **Ainda nunca rodou com um Sober de verdade.**

- `commands/launch_linux.rs` (só Linux): `launch_account_linux` — frases do
  Console por `log_msg!` (en/pt/es/de em `src/locales`), checagem do que falta
  (Flatpak, Sober, `unshare`, `pasta`, com o que instalar), sessão válida,
  servidor privado, sorteio de servidor, `spawn_blocking` do
  `platform::linux::launch_account`, avisos (`SoberWarning`) como `warn`.
  Seguir usuário não existe no Sober: abre o jogo dele, com aviso.
- `commands/launch.rs`: o launch avulso e a fila (`run_launch_queue_unix`, a
  antiga `_macos` agora serve aos dois), cancelar, próxima conta, fechar uma
  conta, fechar todas (só as do app), instâncias abertas, multi, caminho do
  runner. O que é igual no macOS e no Linux usa `platform::unix_host`.
- `lib.rs`: ao fechar o app só solta o rastreio; **nenhum cliente é fechado**.
- Apagar conta (`remove_account`/`remove_accounts`) chama
  `sober::remove_profile` (só dentro de `linux-profiles`; pula conta com
  cliente aberto).
- **Sem `pasta`/`unshare` (regra nº 1)**: `sober::plan_network` decide. Sem
  outro cliente do app aberto, a conta abre **sem rede própria** (`flatpak run`
  direto, HOME do perfil) com um aviso traduzido de que a segunda conta ao mesmo
  tempo precisa do `passt`; com outro cliente aberto vira erro claro (o Sober
  recusaria como "frozen instance"). Se `unshare`/`pasta` existem mas falham, o
  mesmo vale (tenta de novo sem rede própria quando nada mais está aberto).
- `get_platform_capabilities` (Linux): `apply_linux_setup` preenche
  `detectedRunner`/`runnerPath` e `reasons`/`warnings` do diagnóstico. Sem
  Flatpak ou Sober bloqueia o launch (motivo depois do do Auto Rejoin); sem
  `passt` é só aviso. Runner `custom` não cobra nada do Sober. Watcher e
  Auto Rejoin seguem desligados até validar num Linux.
- Pendente: o cookie
  fica em texto puro no perfil (0600) porque o Sober o lê de lá; validar com o dono num Linux com Sober + `passt`
  (bug conhecido do concorrente: com dois abertos o mais novo às vezes fecha
  em 1-3 min).

## Tela no Linux

No Linux a interface **esconde** (não só desabilita) o que depende de Win32 —
branch `linux-ui`. O backend diz o que existe por `get_platform_capabilities`
([platform_info.rs](../../src-tauri/src/commands/platform_info.rs)); o frontend
lê com `platformFeatures(caps)` ([platform.ts](../../src/utils/platform.ts)).
Regra única: **campo ausente conta como suportado** (`!== false`), então
Windows, macOS, backend antigo e o harness antes de as capacidades chegarem não
mudam. Só o Linux manda `false`; o macOS segue mostrando tudo (este porte não
mexeu nele).

Campos novos (todos `bool`, camelCase no JSON): `supportsLiveView`,
`supportsAfkInput`, `supportsIsolation`, `supportsMultiRobloxMutex`,
`supportsVersions`, `supportsOptimization`, `supportsExternalClients`,
`supportsPython`. Os que já existiam (`supportsWindowControls`,
`supportsBotting`, `supportsWatcher`, `supportsClientSettings`,
`supportsLiveAudio`, `supportsMemoryTrim`) também vêm `false`.

| O que some no Linux | Onde | Por quê |
|---|---|---|
| página **Live** | NavSidebar | miniaturas do DWM |
| página **AFK Mode** (e o cartão "Keep accounts in game" da Session, o botão do Auto Rejoin da barra de ações, a seção Settings › Keep in game) | NavSidebar, SessionPage, BottomActionBar, tabs | tecla/clique é SendInput; o Auto Rejoin (`supportsBotting`) ainda não tem backend. A página volta sozinha se `supportsBotting` ou `supportsAfkInput` virar `true` (`afkPage`) |
| Settings › **Isolation**, **Versions**, **Optimization**, **Watcher** | `SETTINGS_TABS[].requires` ([tabs.tsx](../../src/components/settings/tabs.tsx)) | registro/MAC, catálogo e instalador do Roblox, prioridade/memória/FastFlags, varredura de processo — nem montam |
| Multi Roblox e "Auto Close Roblox for Multi Roblox" | Settings › Launch | travas (mutex/evento) do Roblox; no Linux cada conta roda no seu espaço de rede |
| modo **Python** da página Scripts | ScriptsPage | lê pixels da janela do Roblox (Win32) |
| clientes "abertos fora do app, conta desconhecida" | SessionPanel | adoção pela varredura do log |
| grade de janelas, volume por cliente, teto de memória | ConsolePage, Optimization, SessionPage | já eram `isWindows`/`supportsLiveAudio`/`supportsMemoryTrim`; seguem escondidos |

Continuam iguais: contas, cookies, servidores, amigos, grupos, avatares, alertas
do Discord, backups, temas, presets, scripts (JavaScript), Nexus e a lista
Session/Console. O modo de launch do Linux (Sober) é do backend: em
`build_platform_capabilities` o Linux declara `supportsSingleLaunch` e
`supportsMultiLaunch` verdadeiros e ecoa o runner configurado em
`detectedRunner` — **quem sondar o Sober de verdade troca isso** e preenche
`runnerPath`. SO desconhecido (`freebsd`...) continua sem launch.

### Settings › Linux

Seção só do Linux ([LinuxTab.tsx](../../src/components/settings/LinuxTab.tsx),
`requires: "linuxSection"`), logo depois de Launch. Texto de abertura: roda o
Roblox pelo Sober (Flatpak), com o comando para instalar o Sober e o `passt`, e
o aviso de que o que é só do Windows está escondido. Campos, todos em `[Linux]`
do `RAMSettings.ini` ([settings.md](settings.md#linux)):

- **Preferred runner** — `PreferredRunner`: `sober` (padrão) ou `custom`;
- **Custom launch command** — `CustomLaunchCommand`, só editável com o runner
  `custom` (o link do jogo entra no fim);
- **Process names** — `CustomProcessMatch` (lista separada por vírgula);
- **Log folder** — `CustomLogDir` (vazio = automático).

`EnableExperimentalMultiRbx` e `WindowControlBackend` seguem sem tela (a grade
de janelas não existe no Linux). As capacidades são relidas ao sair das Settings
(`reloadSettings`), então trocar o runner vale depois de sair da página.

### Como conferir

`bun run dev:ui` e abra `http://localhost:1420/?scenario=linux&accounts=4`: o
cenário manda as capacidades do Linux e um user agent de Linux. Testes:
`src/components/linuxGating.test.tsx`, `src/utils/platform.test.ts` e
`platform_info_tests` (suíte `diagnostics`).

## Onde está (31/10/2026, fim do dia)

Feito na branch `linux`, em cinco frentes com agentes em worktrees (`C:/mwt/lx`,
`ls`, `lu`, `lp`) e juntado aqui:

- **Compila no Linux** nas duas edições (`cargo check`, `--tests`) — seção
  "Compilação no Linux" abaixo. Conferir aqui no Windows pelo Docker
  (`docker/linux-check.Dockerfile`; o Docker Desktop está limitado a 8 GB pelo
  `%USERPROFILE%/.wslconfig`).
- **Sober ligado ao launch** (single e fila), fechar conta, saída do app e
  remoção da pasta da conta — seções "Backend do Sober". Uma conta sem `pasta`
  abre sem rede própria (regra nº 1); só a segunda ao mesmo tempo exige `passt`.
- **Tela** esconde o que é só do Windows e tem Settings › Linux, com o que falta
  instalar vindo do diagnóstico do backend.
- **Instaladores separados** (padrão e completa com Nexus; AppImage, .deb, .rpm)
  com canal de atualização próprio. O workflow `linux-build.yml` gera os pacotes
  a cada push na `linux` (artefatos do Actions, sem assinatura); o job de release
  segue **desligado** (`PUBLISH_LINUX: "false"`).

**O que falta (nada disto rodou num Linux de verdade):** instalar um pacote numa
distro com Flatpak + Sober + passt, entrar com uma conta, depois duas ao mesmo
tempo, servidor privado, fechar conta, atualizar. Só depois disso o
`PUBLISH_LINUX` liga e a branch vai para a `develop`. Auto Rejoin, monitor de
quedas (watcher), grade, Live e AFK continuam fora no Linux.

### Como ler o resultado do CI sem `gh`

O log do Actions exige token; as **anotações** e a lista de artefatos não:

```bash
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/actions/runs?branch=linux&per_page=4"   # runs
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/actions/runs/<run>/artifacts"           # pacotes
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/check-runs/<job>/annotations"           # erros
```

## Compilação no Linux

O backend compila no Linux nas duas edições (`cargo check` e `cargo check --tests`
com `--features standard` e `--all-features`), sem mudar nada no Windows. O que foi
feito (31/10/2026):

- `lib.rs`: os `use` de `Arc`, `Mutex`, `LazyLock`, `AtomicBool`, `AtomicU64` e
  `Ordering` perderam o `cfg(windows)` (vários comandos compartilhados usam os
  tipos; no Linux, `#[allow(unused_imports)]`).
- `commands/botting.rs`: `launch_account_for_cycle` e `run_botting_session` (o
  ciclo do Auto Rejoin, que chama `platform::windows`) agora têm `cfg(windows)`
  como os comandos que as usam; no Linux `start_botting_mode` já devolvia
  "Auto Rejoin is only supported on Windows".
- `commands/watcher.rs`: `only_launched_by_app` (função pura) saiu do `cfg(windows)`,
  porque um teste a usa em qualquer sistema.
- `commands/launch.rs`: removida uma linha `let _ = &settings;` que só existia
  fora do Windows e citava uma variável que não existe.
- `api/server/launch_patch.rs`: o atalho do servidor HTTP agora segue a função
  que ele chama (`cfg(any(windows, macos))`).
- `python/runs.rs` (teste): `creation_flags` só no Windows.

Stubs que o código de Linux (`platform/linux`, Sober) ainda precisa trocar por
coisa de verdade: o launch (`launch_multiple` devolve "only supported on Windows
and macOS"), o Auto Rejoin (`start_botting_mode`) e o rastreamento de processos.

## Instaladores Linux

Decisão do dono (31/10/2026): a versão Linux é um **instalador separado**, com as duas edições do Windows (padrão e completa com Nexus). Detalhes de build, CI e updater em [development.md](../development.md#linux-instaladores-separados-branch-linux-31102026); o resumo:

- **Formatos:** AppImage (qualquer distro), `.deb` e `.rpm`. Nomes: `MultiAlt_<v>_Linux_amd64.AppImage` / `.deb` / `..._x86_64.rpm` (padrão) e `MultiAlt_<v>_Full-Linux_...` (completa, só nos anexos da release).
- **Dependências do `.deb`:** `libwebkit2gtk-4.1-0` e `libgtk-3-0`; recomenda `flatpak` (o Sober roda por Flatpak), `passt` (o `pasta`, para várias contas ao mesmo tempo) e `libayatana-appindicator3-1` (bandeja). O AppImage não declara nada: quem usa precisa ter `flatpak` (e `passt`) instalados à mão.
- **Atualização própria:** canais `<release>-linux` (padrão) e `<release>-linux-nexus-ws` (completa), com as chaves `linux-x86_64-appimage|deb|rpm`. Instalação Linux nunca recebe o MSI, e o Windows nunca lê esses manifestos. O app escolhe o canal por `PLATFORM_CHANNEL_SUFFIX` ([updater.rs](../../src-tauri/src/commands/updater.rs)).
- **Release:** o job `linux` do `release-v4.yml` está **desligado** (`PUBLISH_LINUX: "false"`) até o Linux ser testado numa máquina de verdade. Enquanto isso, o workflow `linux-build.yml` (push na branch `linux` ou execução manual) gera os dois conjuntos como artefatos para baixar e testar, sem assinatura.
- **Local:** `docker/linux-bundle.Dockerfile` + `packaging/linux/build-bundles.sh` (comando em development.md). Só funciona quando o backend compilar no Linux.

### O que falta verificar (nada disto foi rodado num Linux de verdade)

- Que o `tauri build` gere de fato os três pacotes e as assinaturas (`.AppImage.sig`, `.deb.sig`, `.rpm.sig`) — o fluxo de nomes e do manifesto está coberto por teste, mas o bundler do Tauri só roda onde o backend compila.
- Que o AppImage abra sem FUSE (`APPIMAGE_EXTRACT_AND_RUN` só vale no CI) e que o `.deb` se instale em Ubuntu/Debian limpos com as dependências acima.
- A atualização de ponta a ponta (AppImage trocando o próprio arquivo; `.deb`/`.rpm` via `pkexec`).
- Se `deb.depends` duplicado com o que o Tauri já adiciona sozinho gera aviso do `dpkg-deb` (inofensivo, mas conferir).
