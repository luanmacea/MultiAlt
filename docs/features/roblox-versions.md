# Versões do Roblox (catálogo, instalação e override por conta)

## Objetivo

Permitir instalar builds específicas do Roblox Player (direto do CDN oficial) numa pasta gerenciada pelo app, escolher uma versão padrão e/ou uma versão por conta, e lançar a conta com essa build via old join. Exclusivo de Windows (instalação e launch versionado). A versão **por conta** existe no backend, mas nenhuma tela a define (ver a regra "Override por conta" abaixo).

## Onde fica o código

| Arquivo | Papel |
|---|---|
| [data/versions.rs](../../src-tauri/src/data/versions.rs) | `VersionEntry`, `VersionsCatalogStore` (list/find/upsert/remove/set_label/touch_launched), caminhos `RAMVersions.json` e `RobloxVersions` |
| [commands/versions.rs](../../src-tauri/src/commands/versions.rs) | Comandos `versions_list_installed`, `versions_list_remote`, `versions_install`, `versions_uninstall`, `versions_set_default`, `versions_set_account_override`, `versions_set_label`, `versions_open_folder` |
| [platform/windows/versions.rs](../../src-tauri/src/platform/windows/versions.rs) | `fetch_remote_catalog`, `install_version`, `uninstall_version`, `resolve_roblox_install_path` |
| [platform/windows/tracker.rs](../../src-tauri/src/platform/windows/tracker.rs) | `running_version_keys` (guarda de versão concorrente) |
| [launch.rs](../../src-tauri/src/commands/launch.rs) | Uso no launch único e múltiplo |

## Fluxo

### Listar versões remotas
`versions_list_remote` → `fetch_remote_catalog`: busca `https://weao.xyz/api/versions/current` (obrigatório) e `https://weao.xyz/api/versions/past` (opcional; falha vira `pastError`). Extrai `Windows`/`Mac` + `…Response.version` + `…Date`, sempre com `channel: "LIVE"`.

### Instalar
1. `versions_install(installId, channel, versionHash, label)`; canal vazio → `LIVE`; hash obrigatório.
2. Valida nomes: canal `[A-Za-z0-9_.-]{1,64}` (não `.`/`..`), hash `version-<hex>`.
3. Destino: `%LOCALAPPDATA%\Roblox Account Manager\RobloxVersions\<canal>\<hash>`; staging em `<hash>.staging-<ms>` (apagado automaticamente em erro).
4. Manifesto `<hash>-rbxPkgManifest.txt` de `https://setup-aws.rbxcdn.com/` (LIVE) ou `…/channel/<canal>/`; se falhar num canal não-LIVE, tenta `…/channel/common/`.
5. Baixa pacotes em paralelo (`MaxParallelDownloads`, 1–8), verifica hash (MD5 se 32 hex, SHA-256 se 64) e extrai cada zip na subpasta mapeada (`EXTRACT_ROOTS`, ex.: `content-textures3.zip` → `PlatformContent/pc/textures/`).
6. Grava `AppSettings.xml`; exige `RobloxPlayerBeta.exe` no staging.
7. Troca atômica: pasta existente vira `.old-<ms>`, staging é renomeado para o destino; em erro restaura o antigo.
8. `upsert` no catálogo e evento final `version-install-progress` `ready` (estágios: `resolving` → `installing` por pacote → `ready`).

### Resolver qual instalação usar (`resolve_roblox_install_path`)

Precedência:
1. **Override da conta** (`account.fields["RobloxVersion"]`, formato `<canal>:<hash>`): precisa existir no catálogo e ter `RobloxPlayerBeta.exe`, senão **erro** (não cai para o próximo nível). As mensagens de erro mandam "clear the per-account override" — e isso só se faz editando o campo à mão (ver "Override por conta" abaixo).
2. **`Versions.DefaultVersion`**: mesma validação e erro.
3. **Versão do catálogo usada mais recentemente** (`last_launched_at`, ou `installed_at`) que tenha o exe.
4. **Instalação do sistema** (`get_roblox_path`: a pasta da **última build resolvida** por qualquer consulta de canal — `cached_production_player_dir`, que apesar do nome também guarda a build do canal do registro —, senão a do `HKCR/roblox/DefaultIcon`, senão a pasta `version-*` mais recente da primeira pasta `Versions` que tiver alguma, nesta ordem: `%LOCALAPPDATA%/Roblox`, `Bloxstrap`, `Fishstrap`, `Voidstrap` — `candidate_versions_dirs` em [core.rs](../../src-tauri/src/platform/windows/core.rs); a instalação oficial ganha de qualquer bootstrapper) → `version_id = None`. No spawn essa pasta é só a reserva: pelo protocolo, `launch_url` abre a build de **produção**; no old join, `default_player_dir` troca pela pasta da build do canal **lido** do registro, instalando-a se faltar, e só devolve esta pasta se nem isso der (ver [launch.md](launch.md#canal-do-roblox-e-a-tela-de-atualização-causa-raiz-e-fix)). O app não fixa canal: só o lê — a única escrita é o reparo de canal morto descrito lá.

```mermaid
sequenceDiagram
    participant L as launch_*
    participant R as resolve_roblox_install_path
    participant C as VersionsCatalogStore
    L->>R: RobloxVersion da conta
    alt override definido
        R->>C: find(canal:hash) (erro se ausente)
    else DefaultVersion definido
        R->>C: find(DefaultVersion) (erro se ausente)
    else catálogo não vazio
        R->>C: mais recente com exe
    else
        R->>R: get_roblox_path() (última build resolvida, HKCR ou pasta Versions; version_id=None)
    end
    R-->>L: (base_path, version_id)
    alt version_id.is_some()
        L->>L: old join em base_path (pasta do catálogo, sem consultar canal)
    else version_id = None e old join
        L->>L: default_player_dir(base_path) → build do canal lido do registro
    else version_id = None e protocolo
        L->>L: launch_url → build de produção (o registro não é lido)
    end
```

## Regras de negócio

- **ID de versão** = `"<canal>:<hash>"` (`VersionEntry::version_id`).
- **Versão do catálogo ⇒ old join**: qualquer `version_id` resolvido força `RobloxPlayerBeta.exe --app -t -j` na pasta da versão (exceto quando `Isolation.Mode = Full` sem versão do catálogo, ver [launch.md](launch.md)).
- **Clientes concorrentes devem compartilhar a mesma versão**: o tracker guarda o `version_id` de cada PID e de cada launch pendente; se algum for diferente do da nova conta, o launch único falha e o multi pula a conta (`version-conflict` no evento). A mensagem que o usuário lê vem de `version_conflict_message` e **lista as versões abertas**. Instalação do sistema conta como a "versão" `None` e aparece na lista como `system install`.
- **Uninstall:** recusado se a versão estiver rodando; apaga a pasta, remove do catálogo, limpa `DefaultVersion` se for ela e remove `RobloxVersion` de toda conta que a usava.
- **Override por conta:** `versions_set_account_override(userId, versionId|null)`; vazio/null remove o campo. O comando está registrado, mas **nenhuma tela o chama**: o seletor do painel da conta ("Roblox Version (all accounts)") grava a `Versions.DefaultVersion` global. Hoje o `RobloxVersion` só é definido ou limpo em View/Edit Fields (com Developer Mode), por script (`update_account`) ou pelo web server (`SetField`/`RemoveField`).
- **`touch_launched`** atualiza `last_launched_at` quando o PID é detectado (influencia o passo 3 da precedência).
- **Catálogo** persistido em `%LOCALAPPDATA%\Roblox Account Manager\RAMVersions.json` (escrita via `.json.tmp` + `MoveFileExW` atômico). Na primeira execução, copia `RAMVersions.json` legado de ao lado do exe se existir.
- **Isolamento Full nunca apaga** `%LOCALAPPDATA%\Roblox Account Manager\RobloxVersions`.

## Configurações relacionadas

Seção `[Versions]`:

| Chave | Default | Efeito |
|---|---|---|
| `DefaultVersion` | vazio | `canal:hash` usado quando a conta não tem override |
| `MaxParallelDownloads` | `4` | Downloads simultâneos na instalação (clamp 1–8) |
| `CatalogCacheMinutes` | `10` | Criado nos defaults; não é lido em lugar nenhum |
| `PreferOldJoinForVersioned` | `true` | Exibido na UI ([VersionsTab.tsx](../../src/components/settings/VersionsTab.tsx)); **ignorado pelo backend** (versionado sempre usa old join) |
| `ShowPreReleaseVersions` | `false` | Toggle na UI ([VersionsTab.tsx](../../src/components/settings/VersionsTab.tsx)); não lido pelo backend |

Campo por conta: `RobloxVersion` (`canal:hash`).

## Armadilhas / cuidados

- Se existir **qualquer** versão no catálogo, contas sem override e sem `DefaultVersion` usam a versão do catálogo mais recente (passo 3), não a instalação do sistema. Isso muda o modo de launch para old join sem o usuário perceber.
- Misturar contas com versões diferentes num mesmo lote faz as divergentes serem puladas.
- O web server ignora o catálogo: pelo protocolo abre a build de produção (`launch_url`); com `UseOldJoin`, a build do canal lido do registro (`launch_old_join` → `default_player_dir`). O Auto Rejoin **usa** o catálogo (`RobloxVersion`/`DefaultVersion`, mesma `resolve_roblox_install_path` do launch) e reporta a versão resolvida ao tracker (`track_with_version`, só no ramo old join — ver [botting.md](botting.md)), então ele também participa da guarda de versão concorrente do lado de quem lança depois dele; mas o próprio Auto Rejoin não checa conflito antes de lançar.
- Versões do catálogo **não** passam pelo casamento build × canal (ver [launch.md](launch.md#canal-do-roblox-e-a-tela-de-atualização-causa-raiz-e-fix)): o old join executa a build instalada como está.
- Uma build antiga do catálogo pode ser rejeitada pelos servidores do Roblox (exigir update); nesse caso o cliente abre o instalador próprio.
- Sem `weao.xyz` no ar, a lista remota falha (a instalação por hash manual continua funcionando via CDN).
