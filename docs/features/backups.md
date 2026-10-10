# Backups e pasta de dados

## Objetivo

Guardar cópias dos dados do usuário (contas, settings, scripts, temas, avatares salvos, favoritos com os servidores VIP, jogos e servidores recentes, presets de launch, catálogo de versões e lista do Nexus) e conseguir voltar uma delas **de dentro do app**, sem copiar arquivo na mão. Resolve o caso "apaguei tudo sem querer" sem precisar recadastrar conta por conta.

Anda junto com a mudança de pasta: os dados saíram de "ao lado do executável" para a pasta do usuário, então o `.exe` pode ser movido à vontade (ver [architecture.md](../architecture.md#arquivos-de-persistência)).

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Comandos, formato do zip, retenção, restauração | [commands/backups.rs](../../src-tauri/src/commands/backups.rs) |
| Resolução da pasta de dados + migração | [data/settings/paths.rs](../../src-tauri/src/data/settings/paths.rs) (`get_runtime_data_dir`, `decide_data_dir`, `migrate_data_files`) |
| Registro dos comandos | [lib.rs](../../src-tauri/src/lib.rs) |
| Tela | [BackupsTab.tsx](../../src/components/settings/BackupsTab.tsx) — seção **Backups** da página Settings (até 03/10/2026 era o diálogo `BackupsDialog`, aberto por "Manage" em Misc > Data) |
| Entrada na UI | Settings › **Backups** (logo abaixo de General, [tabs.tsx](../../src/components/settings/tabs.tsx)). A seção só lê a pasta quando aparece (`active`), e relê a cada vez que volta a aparecer |
| Tipos no frontend | [types.ts](../../src/types.ts) (`BackupEntry`, `RestoreReport`, `BackupsInfo`) |

## Fluxo

1. **Criar** (`create_backup`, rótulo opcional): zipa os arquivos que existirem na pasta de dados + `RAMVersions.json` + um `backup-manifest.json` (formato `ram-backup-v1`, data UTC, rótulo, se é automático, versão do app, lista de arquivos com tamanho). Nome: `backup-YYYYMMDD-HHMMSS[-rótulo saneado].zip`, em `<pasta de dados>/backups/`.
2. **Listar** (`list_backups`): mais recente primeiro. Zip ilegível **continua aparecendo**, com `valid: false` (o usuário precisa saber que ele existe, mesmo quebrado) — só pode ser apagado.
3. **Restaurar** (`restore_backup`): cria antes um **backup automático do estado atual** (é o `safetyBackupId` do relatório), depois grava arquivo por arquivo com troca atômica (`atomic_replace`). Entradas fora da allowlist ou que tentem escapar da pasta (zip-slip) entram em `skipped` e não são gravadas.
4. **Apagar** (`delete_backup`) e **abrir a pasta** (`open_backups_folder`).

## Regras de negócio

- **Retenção:** no máximo 10 backups **automáticos** (os de segurança criados antes de restaurar). Backup criado pelo usuário nunca é apagado pelo app; zip ilegível também não (não dá para afirmar que era automático).
- **O que exige reiniciar o app**, informado em `requiresRestart` + `restartReasons` e mostrado na tela:
  - settings, tema, presets, scripts e catálogo de versões — os stores são construídos no startup e não têm recarga; sem reiniciar, a memória antiga sobrescreveria o que foi restaurado na gravação seguinte;
  - `AccountData.json` restaurado **criptografado** — a chave em memória é a da senha/chave antiga. Com o formato cifrado por padrão, este é agora o caso **normal**: praticamente toda restauração de contas pede reinício;
  - `AccountData.json` em texto puro **com senha configurada** — a próxima gravação recriptografaria com a senha da sessão.
  - Só o caso "texto puro, sem senha na sessão" recarrega na hora (`accountsReloaded: true`), e ele só aparece restaurando backup antigo, de antes da criptografia por padrão.
- **Favoritos, VIPs e recentes** (`RAMGameLists.json`, desde 03/10/2026 — antes moravam só no `localStorage` do WebView e ficavam fora do zip) **não** pedem reinício: a store relê o disco a cada leitura. Durante a restauração o [BackupsTab](../../src/components/settings/BackupsTab.tsx) para o espelho do frontend (`pauseGameListsMirror`) e, no fim, hidrata de novo — por **união** com o cache local: o que o backup tinha volta, o que foi adicionado depois dele fica. Backup antigo, sem o arquivo, não mexe nas listas atuais. Ver [server-list.md](server-list.md#onde-as-listas-moram).
- **Presets de launch** (`RAMLaunchPresets.json`, desde a ideia 13) também não pedem reinício: o `LaunchPresetStore` relê o disco a cada leitura, e o agendador lê dele a cada passada. Ver [presets.md](presets.md).
- O catálogo de versões é restaurado no caminho real de `get_versions_catalog_path()`, que fica fora da pasta de dados no modo portátil.
- O rótulo é saneado para virar nome de arquivo (barra, `..`, caminho absoluto, controles, tamanho), mas o rótulo **exibido** é o que o usuário digitou, guardado no manifesto.

## Pasta de dados

Ordem de resolução (ver [architecture.md](../architecture.md#arquivos-de-persistência)): `RAM_DATA_DIR` → `portable.txt` ao lado do exe → `%LOCALAPPDATA%\Roblox Account Manager` → pasta do exe.

Na primeira execução, o que estava ao lado do executável é **copiado** para a pasta nova: nunca sobrescreve arquivo já existente no destino e **nunca apaga a origem**, então voltar para uma versão antiga do app continua funcionando.

## Armadilhas / cuidados

- **O zip leva o `AccountData.key` junto com o `AccountData.json`** (os dois estão em `DATA_FILES`). Tem que levar: o vault é cifrado por uma chave mestra aleatória, então um backup sem a chave dele é um backup que **não restaura** — nem com senha, porque não existe senha nesse modo. O custo é honesto: num backup vazado o que protege o vault deixa de ser o DPAPI (que só abre no perfil de origem) e passa a ser o embrulho do **hash do aparelho**, que alguém com o zip pode atacar sabendo o nome da máquina e do usuário. Ainda é muito melhor que o formato anterior (JSON puro, cookies legíveis sem esforço nenhum), mas quem guarda backup em OneDrive/Drive deve usar **senha** (Pass Lock) — aí a chave não está no zip. O diálogo de backups diz isso ao lado do botão de criar enquanto o app estiver sem senha (`accountsEncrypted !== true`), porque é ali que o zip nasce e é dali que se abre a pasta para copiá-lo para a nuvem. Ver [accounts.md](accounts.md#a-chave-do-aparelho-accountdatakey).
- Backup **não** é criptografado além do que já estava. Em particular, um `AccountData.json.bak` deixado pela migração para o formato cifrado é o arquivo **em texto puro** — ele não entra no zip (não está em `DATA_FILES`), mas continua na pasta de dados até alguém apagar.
- Restaurar com o Roblox aberto pode falhar em arquivo em uso — a troca é atômica por arquivo e falha alto em vez de truncar, mas o resultado fica parcial (`restored`/`skipped` mostram o que entrou).
- `open_backups_folder` é Windows-only.
- O modo portátil coloca os dados junto do `.exe`: mover ou atualizar a pasta leva/deixa os dados junto. O diálogo avisa isso. **Mas levar os arquivos para outra máquina não abre as contas**: a chave do vault é presa ao aparelho de origem. Ver [accounts.md](accounts.md#a-chave-do-aparelho-accountdatakey) — em mais de um PC, o caminho é senha (Pass Lock).
- **Restaurar contas ou a chave tranca a gravação até reiniciar, e o default é trancado.** Avisar em `restartReasons` não bastava: o app continuava utilizável e **um único launch** (`mark_used` → `save`) sobrescrevia o vault restaurado com o segredo da sessão anterior — lockout no boot seguinte. A primeira versão desta trava era opt-in por ramo, e um ramo ficou de fora (o `Err` de `is_encrypted()`, quando o `fs::read` falha porque o antivírus está segurando o arquivo recém-substituído): ali a restauração era desfeita **em silêncio**, e o arquivo continuava abrindo, então o usuário só descobria pelas contas velhas.
  Agora a trava é ligada **antes de extrair** — só depois de extrair se sabe o que o zip mexeu, e nesse intervalo um ciclo de Auto Rejoin gravaria com o segredo antigo. E ligar a trava **espera a gravação que já estiver em andamento**: `save_locked` confere a trava no começo e só publica o arquivo no fim (fsync + troca atômica), então um ciclo de Auto Rejoin que passou pela checagem com a trava aberta terminava **depois** da extração e punha o vault de antes por cima do restaurado. `lock_writes_until_restart` toma o mutex de contas — que toda gravação segura da checagem até a troca do arquivo — antes de ligar a trava. Depois da extração, `restore_keeps_writes_locked` decide se ela continua: extração que **falhou** mantém trancado (sem lista confiável de arquivos, não se decide nada), e `restore_touches_accounts` (qualquer `AccountData.json` ou `AccountData.key` restaurado) **mantém trancado**, e quem destranca é **`AccountStore::mark_memory_fresh`**, chamado de dentro do `load()` quando ele lê o arquivo com sucesso. A regra é essa e não "o ramo lembrou de destrancar": o latch existe para impedir que memória **velha** sobrescreva o arquivo, e depois de um `load()` bem-sucedido a memória **é** o arquivo.
  Quem restaurou só tema/settings destranca logo depois, para não ficar somente-leitura à toa — **mas só se não havia trava ao entrar** (`restore_may_release_lock`, com `writes_locked()` lido **antes** de trancar). Sem essa condição, restaurar um backup com `AccountData.key`, **não reiniciar** e restaurar em seguida um que só tem `RAMSettings.ini` soltava a trava da primeira restauração — com a memória velha e o segredo da sessão anterior. O launch seguinte regravaria o vault com o master antigo e o `.key` por cima do restaurado, matando justamente o estado que a primeira restauração trouxe.
  A primeira versão da inversão destrancava só no ramo, depois do `load()`, e isso trancou o próprio caminho que devia recarregar: um `AccountData.json` em **texto puro** restaurado passa por `migrate_plain_vault` → `save_locked`, e o latch recusava — o dono recebia "não foi possível reler" **depois** de a leitura funcionar, e a mensagem convidava a restaurar de novo.
