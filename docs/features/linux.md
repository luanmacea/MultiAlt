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
- Pendente: capacidades em `platform_info.rs` e telas (agente da UI); o cookie
  fica em texto puro no perfil (0600) porque o Sober o lê de lá; fallback sem
  `pasta` para uma conta só; validar com o dono num Linux com Sober + `passt`
  (bug conhecido do concorrente: com dois abertos o mais novo às vezes fecha
  em 1-3 min).

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
