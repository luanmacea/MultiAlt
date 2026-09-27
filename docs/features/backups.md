# Backups e pasta de dados

## Objetivo

Guardar cópias dos dados do usuário (contas, settings, scripts, temas, catálogo de versões e lista do Nexus) e conseguir voltar uma delas **de dentro do app**, sem copiar arquivo na mão. Resolve o caso "apaguei tudo sem querer" sem precisar recadastrar conta por conta.

Anda junto com a mudança de pasta: os dados saíram de "ao lado do executável" para a pasta do usuário, então o `.exe` pode ser movido à vontade (ver [architecture.md](../architecture.md#arquivos-de-persistência)).

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Comandos, formato do zip, retenção, restauração | [commands/backups.rs](../../src-tauri/src/commands/backups.rs) |
| Resolução da pasta de dados + migração | [data/settings/paths.rs](../../src-tauri/src/data/settings/paths.rs) (`get_runtime_data_dir`, `decide_data_dir`, `migrate_data_files`) |
| Registro dos comandos | [lib.rs](../../src-tauri/src/lib.rs) |
| Diálogo | [BackupsDialog.tsx](../../src/components/dialogs/BackupsDialog.tsx) |
| Entrada na UI | [MiscellaneousTab.tsx](../../src/components/settings/MiscellaneousTab.tsx) → seção "Data" → "Manage" |
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
- O catálogo de versões é restaurado no caminho real de `get_versions_catalog_path()`, que fica fora da pasta de dados no modo portátil.
- O rótulo é saneado para virar nome de arquivo (barra, `..`, caminho absoluto, controles, tamanho), mas o rótulo **exibido** é o que o usuário digitou, guardado no manifesto.

## Pasta de dados

Ordem de resolução (ver [architecture.md](../architecture.md#arquivos-de-persistência)): `RAM_DATA_DIR` → `portable.txt` ao lado do exe → `%LOCALAPPDATA%\Roblox Account Manager` → pasta do exe.

Na primeira execução, o que estava ao lado do executável é **copiado** para a pasta nova: nunca sobrescreve arquivo já existente no destino e **nunca apaga a origem**, então voltar para uma versão antiga do app continua funcionando.

## Armadilhas / cuidados

- **O zip leva o `AccountData.key` junto com o `AccountData.json`** (os dois estão em `DATA_FILES`). Tem que levar: o vault é cifrado por uma chave mestra aleatória, então um backup sem a chave dele é um backup que **não restaura** — nem com senha, porque não existe senha nesse modo. O custo é honesto: num backup vazado o que protege o vault deixa de ser o DPAPI (que só abre no perfil de origem) e passa a ser o embrulho do **hash do aparelho**, que alguém com o zip pode atacar sabendo o nome da máquina e do usuário. Ainda é muito melhor que o formato anterior (JSON puro, cookies legíveis sem esforço nenhum), mas quem guarda backup em OneDrive/Drive deve usar **senha** (Pass Lock) — aí a chave não está no zip. Ver [accounts.md](accounts.md#a-chave-do-aparelho-accountdatakey).
- Backup **não** é criptografado além do que já estava. Em particular, um `AccountData.json.bak` deixado pela migração para o formato cifrado é o arquivo **em texto puro** — ele não entra no zip (não está em `DATA_FILES`), mas continua na pasta de dados até alguém apagar.
- Restaurar com o Roblox aberto pode falhar em arquivo em uso — a troca é atômica por arquivo e falha alto em vez de truncar, mas o resultado fica parcial (`restored`/`skipped` mostram o que entrou).
- `open_backups_folder` é Windows-only.
- O modo portátil coloca os dados junto do `.exe`: mover ou atualizar a pasta leva/deixa os dados junto. O diálogo avisa isso. **Mas levar os arquivos para outra máquina não abre as contas**: a chave do vault é presa ao aparelho de origem. Ver [accounts.md](accounts.md#a-chave-do-aparelho-accountdatakey) — em mais de um PC, o caminho é senha (Pass Lock).
- **Restaurar contas ou a chave tranca a gravação até reiniciar, e o default é trancado.** Avisar em `restartReasons` não bastava: o app continuava utilizável e **um único launch** (`mark_used` → `save`) sobrescrevia o vault restaurado com o segredo da sessão anterior — lockout no boot seguinte. A primeira versão desta trava era opt-in por ramo, e um ramo ficou de fora (o `Err` de `is_encrypted()`, quando o `fs::read` falha porque o antivírus está segurando o arquivo recém-substituído): ali a restauração era desfeita **em silêncio**, e o arquivo continuava abrindo, então o usuário só descobria pelas contas velhas.
  Agora: `restore_touches_accounts` (qualquer `AccountData.json` ou `AccountData.key` restaurado) **tranca**, e `AccountStore::allow_writes_after_reload` é o **único** ponto que destranca — chamado só no caminho que releu o arquivo com sucesso ("texto puro, sem senha na sessão"). Assim um ramo novo, ou um erro que ninguém previu, nasce seguro.
