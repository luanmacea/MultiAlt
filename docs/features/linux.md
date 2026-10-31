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

## Onde parou (31/10/2026)

Pausado a pedido do dono ("vai demorar mais do que eu esperava"). Nada de
código do app mudou ainda: a branch só tem este documento e o workflow
`.github/workflows/linux-check.yml`.

### O que o CI mostrou

O workflow `Linux check` (ubuntu-22.04, roda a cada push na `linux`) faz
`cargo check` da edição padrão e da completa. **As duas falham** (exit 101):
o backend nunca foi escrito para compilar fora do Windows/macOS. Primeiros
erros vistos (lista cortada pelo limite da anotação):

- `src/commands/memory_ceiling.rs:303-304` — `LazyLock` e `Mutex` sem import
  fora do Windows (o `use` deve estar atrás de `cfg(windows)`).
- `src/commands/botting.rs` (dezenas) — o Auto Rejoin chama funções que só
  existem nos submódulos de Windows: `ensure_multi_roblox_enabled`,
  `get_or_create_browser_tracker_id`, `is_429_related_error`,
  `wait_for_new_roblox_pid`, `apply_windows_post_launch_profile`,
  `emit_botting_status`, `wait_for_launch_slot`, `backoff_delay_seconds`...
  Provavelmente um `include!` de submódulo do botting está atrás de
  `cfg(windows)` e o resto do arquivo não.
- Só na edição padrão: `patch_client_settings_for_launch` (botting.rs:361).
- Avisos: `python/devpipe.rs:12` e `commands/discord_status.rs:16`
  (imports sem uso fora do Windows).

O último push (`31cac65`) pede ao CI um **resumo por arquivo** (total de
erros, erros por arquivo e os 25 primeiros). Ele rodou, mas a leitura não
foi feita (a API do GitHub sem token tem limite de 60 consultas por hora).

### Como ler o resultado do CI sem `gh`

O log do Actions exige token; as **anotações** não. O workflow põe os erros
numa anotação `::error` por edição:

```bash
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/actions/runs?branch=linux&per_page=1"   # id do run
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/actions/runs/<run>/jobs"                # id do job
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/check-runs/<job>/annotations"           # os erros
```

### Próximos passos

1. Ler o resumo por arquivo do run do `31cac65` (ou fazer um push e ler o novo).
2. **Compilar no Linux** (estimativa 1–3 h): dividir os arquivos entre 3–4
   agentes (Sonnet, um conjunto de arquivos por agente, sem dois mexendo no
   mesmo). Regra: no Linux o caminho de Windows vira stub que devolve erro ou
   nada — nunca mudar o comportamento no Windows. `bun run check` continua
   tendo que passar no Windows a cada passo.
3. **`platform/linux/` com o Sober** (3–5 h): perfil por conta, gravar o
   cookie, `unshare` + `pasta`, deep link, rastrear/fechar processo.
4. **Tela e pacote** (~2 h): esconder o que é só do Windows
   (`get_platform_capabilities` já existe para isso) e gerar AppImage/deb no CI.
5. **Teste de verdade** — o gargalo: aqui só há Windows (o WSL só tem a
   distro do Docker). Precisa de um Linux com o Sober: PC, máquina virtual,
   alguém do Discord, ou instalar Ubuntu no WSL (só com ok do dono; não se sabe
   se o Sober abre no WSLg). Nada é "pronto" sem esse teste.

Para retomar: `git switch linux`, ler este arquivo, e trazer a `develop`
(`git merge develop`) antes de começar, porque ela continua andando.

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
