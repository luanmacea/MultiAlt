# Avatares grátis (montar e distribuir entre contas)

## Objetivo

Dar visual às contas secundárias **sem gastar Robux**. O usuário monta (à mão ou sorteando) alguns avatares só com **itens oficiais gratuitos do Roblox**, salva, e depois distribui esses avatares entre as contas que escolher. Para cada conta, o app **pega de graça as peças que faltam** e **veste** o avatar.

Especificação: [docs/superpowers/specs/2026-10-02-free-avatars-design.md](../superpowers/specs/2026-10-02-free-avatars-design.md).

**O que não é:** loja. O módulo nunca compra nada pago, nunca resolve captcha e não mexe em conta que o Roblox pediu verificação. Fora de escopo, de propósito: itens UGC (de criador que não é o Roblox), roupas em camadas (layered clothing, asset types 64–72), emotes/animações e escalas do corpo.

## Onde fica o código

| Arquivo | Papel |
|---|---|
| [api/roblox/avatar_catalog.rs](../../src-tauri/src/api/roblox/avatar_catalog.rs) | Busca do catálogo gratuito (`search_free_official_items`), detalhes colecionáveis (`collectible_details`), peças de um bundle com o tipo de cada uma (`bundle_asset_ids` → `BundleAsset { id, asset_type }`), posse (`owns_item`) e o resgate (`claim_free_item` → `ClaimOutcome`) |
| [data/avatars.rs](../../src-tauri/src/data/avatars.rs) | `AvatarStore`: avatares salvos em `RAMAvatars.json`, validação (`validate_avatar`) |
| [commands/avatars.rs](../../src-tauri/src/commands/avatars.rs) | Comandos, o lote (`avatar_apply_batch`, `apply_avatar_to_account`, `assign_avatars`), evento `avatar-batch-state`, cancelamento, guarda de execução única |
| [api/batch.rs](../../src-tauri/src/api/batch.rs) | `ImageCache::invalidate_targets`: esquece o headshot em cache das contas que trocaram de avatar |
| [avatarBuilder.ts](../../src/avatarBuilder.ts) | Lógica pura do construtor: categorias, seleção, sorteio, paleta de pele, validação do rascunho |
| [pages/AvatarsPage.tsx](../../src/components/pages/AvatarsPage.tsx) | A casca da tela: catálogo, avatares salvos, ouvinte do `avatar-batch-state`, comandos |
| [pages/avatars/](../../src/components/pages/avatars) | `BuildTab` (Montar), `DistributeTab` (Distribuir), `AccountPicker` (contas dentro do diálogo), `BatchPanel` (progresso e resumo), `useAvatarDraft`, `useAvatarThumbs` (miniaturas com retentativa), `shared.tsx` |
| [store.tsx](../../src/store.tsx) | `refreshAvatarHeadshots` e `avatarsDialogOpen` |
| [layout/NavSidebar.tsx](../../src/components/layout/NavSidebar.tsx) | Item **Avatars** da barra lateral, que abre a página |
| [dev/harness/scenarios.ts](../../src/dev/harness/scenarios.ts) | Cenário `avatars` do harness (`bun run dev:ui`, `?scenario=avatars&accounts=6`) |

## Endpoints usados

Todos montados por `endpoints::host(...)` (nunca literal `https://*.roblox.com`).

| Host | Rota | Para quê | Login |
|---|---|---|---|
| `catalog` | `GET /v2/search/items/details?Category=1&CreatorName=Roblox&MaxPrice=0&MinPrice=0&SalesTypeFilter=1&Limit=120` | Catálogo gratuito oficial (paginado por cursor) | não |
| `catalog` | `GET /v1/bundles/{id}/details` | As peças (assets) de um bundle de corpo/cabeça | não |
| `apis` | `POST /marketplace-items/v1/items/details` | `collectibleProductId`, preço e criador, em blocos de 50 | não |
| `apis` | `POST /marketplace-sales/v1/item/{collectibleItemId}/purchase-item` | O resgate do item gratuito | cookie + CSRF |
| `inventory` | `GET /v1/users/{uid}/items/{Asset\|Bundle}/{id}/is-owned` | A conta já tem o item? | cookie |
| `avatar` | `set-player-avatar-type`, `set-body-colors`, `v2/avatar/set-wearing-assets` | Vestir (é o `set_avatar` que já existia; resposta não 2xx do `set-wearing-assets` vira erro `Failed to wear assets (status N)`) | cookie + CSRF |
| `thumbnails` | lote com tipos `Asset` e `BundleThumbnail` (`batch_thumbnails`) | Ícones dos itens na tela | não |

Os endpoints `marketplace-*` **não são documentados** pelo Roblox. Se mudarem, o lote falha por peça com a mensagem do Roblox, sem gastar nada.

## Segurança: nunca gasta Robux

Duas travas independentes, ambas no backend:

1. **Detalhes frescos, antes de qualquer pedido de compra.** O `collectible_details` é consultado de novo para cada peça que falta, e o resgate só sai se **preço == 0 e criador == 1 (Roblox)**. Diferente disso, `claim_free_item` devolve `NotFree` **sem fazer requisição nenhuma** (`ClaimOutcome::NotFree`); a peça é pulada.
2. **`expectedPrice: 0` no corpo do resgate.** Se o preço mudou entre a consulta e o pedido, o Roblox recusa com `PriceMismatch` e nada é cobrado (também vira `NotFree`).

Além disso, a lista do catálogo que chega à tela **nunca tem item pago**: `catalog_item_from_json` descarta preço ≠ 0, criador ≠ 1 e tipos fora da lista permitida, mesmo que a busca já filtre.

Mais regras:

- **Desafio = conta pulada.** Resposta 403 com cabeçalho `rblx-challenge-id` (captcha ou 2 etapas) vira `ChallengeRequired` — a conta para ali, fica marcada "Verification required — skipped" e o lote segue para a próxima. **Não há tentativa de resolver** o desafio.
- **Leituras de cookie nunca renovam a sessão.** O lote usa `get_cookie` direto (um cookie vencido só falha aquela conta) e a posse usa uma leitura simples: nada de `run_with_session_retry` / `refresh_account_session`, que derrubariam as sessões abertas da conta (regra crítica do projeto).
- **Uma conta de cada vez**, e **7 s de pausa entre dois resgates** na mesma conta (`CLAIM_PAUSE`; o limite do Roblox é de cerca de 9 compras por minuto por usuário). Só conta tentativa de resgate: peça já possuída não paga a pausa.
- **Um lote por vez** (`try_begin_avatar_batch` + `AvatarBatchRunGuard`): a tela pode remontar e disparar outro; o segundo é recusado. As recusas do `avatar_apply_batch` saem em inglês (`An avatar batch is already running`, `No account selected`, `No saved avatar selected`) e a tela passa pelo `t()` — o inglês é a chave do catálogo.
- Nada aqui fecha, mata ou toca em cliente Roblox aberto.

## Tipos permitidos

- **Assets** (`ALLOWED_ASSET_TYPES`): camiseta (2), chapéu (8), camisa (11), calça (12), cabelo (41) e acessórios de rosto/pescoço/ombro/frente/costas/cintura (42–47).
- **Bundles** (`ALLOWED_BUNDLE_TYPES`): pacote de corpo (1) e cabeça dinâmica (4). Os outros tipos de bundle (animações) ficam de fora.
- O rosto clássico grátis (asset 18) não aparece: o rosto vem com o pacote de corpo/cabeça.

Em 02/10/2026 a busca devolvia 238 itens em 3 páginas (sem login), 186 assets e 52 bundles; o catálogo muda com o tempo.

## Fluxo

### Catálogo

`avatar_free_catalog` pagina a busca (pausa de 1,5 s entre páginas, teto de 20 páginas para um cursor que nunca acaba não prender o app) e guarda o resultado **em memória por 6 h**. A tela agrupa por categoria (`groupByCategory`).

### Montar (aba Build)

- Categorias: Cabelo, Chapéu, Acessório, Camisa, Calça, Camiseta, Corpo, Cabeça. **Uma peça por categoria; Acessório aceita até 3.**
- Camisa, Calça e Corpo são **obrigatórios** (`REQUIRED_CATEGORIES`); o avatar salvo tem de ter de 1 a 12 itens, nome de 1 a 60 caracteres e nenhum `id` repetido (**nem entre asset e bundle** — o backend compara só o número, então a seleção nunca junta os dois).
- **Sortear** tudo ou só uma categoria (`randomizeSelection`): as obrigatórias saem sempre preenchidas quando o catálogo tem item; as opcionais ficam vazias em ~40% das vezes; acessório leva 0 a 2 itens distintos. A pele sorteia de uma paleta curta de 8 tons (`SKIN_COLORS`, ids BrickColor).
- A prévia é a grade dos ícones escolhidos: o Roblox não renderiza o corpo inteiro de um avatar que não está em nenhuma conta.
- **Salvar** (`avatar_save`), **carregar** para editar e **apagar** (`avatar_delete`, com confirmação).
- Miniaturas (`useAvatarThumbs`): 150×150, pedidas sob demanda, uma vez por item; o Roblox às vezes responde `Pending` enquanto gera a imagem, e aí o hook **tenta de novo até 3 vezes, a cada 2,5 s**, antes de desistir e mostrar "sem imagem".

### Distribuir (aba Distribute)

1. O usuário marca os avatares salvos que entram no sorteio (ou "todos") e as **contas** — o `AccountPicker` dentro do diálogo nasce com a seleção da lista principal ao abrir, mas dá para marcar e desmarcar ali mesmo.
2. `avatar_apply_batch { userIds, avatarIds }` valida (precisa de conta e de avatar), tira repetidos e chama `assign_avatars`: **embaralha os avatares e reparte em rodízio**, de modo que nenhum avatar se repete enquanto houver outro sem uso; acabado o baralho, embaralha de novo. Função pura, com o gerador de números passado de fora.
3. Para cada conta, em sequência (`apply_avatar_to_account`):
   1. cada peça do avatar: `owns_item`; se já possui, vai direto para o que se veste. Falha ao consultar o inventário conta como "não tem" e tenta o resgate (a trava 2 protege de qualquer jeito);
   2. peça sem `collectibleItemId`, ou sem detalhes, vira `missing` e fica de fora;
   3. `collectible_details` → pausa de 7 s se já houve resgate nesta conta → `claim_free_item`: `Claimed` (conta em `claimed`), `AlreadyOwned` (veste), `NotFree`/`Failed` (`missing`), `ChallengeRequired` (**para a conta**, `skipped` / `challenge`);
   4. vestir: bundle vira os assets dele (`bundle_asset_ids`, sem o `UserOutfit`, cada um com o `assetType`), repetidos saem, e `set_avatar` recebe `playerAvatarType: R15`, os assets e, se há pele, `bodyColors` com a mesma cor nas seis partes. **Peça que não foi pega fica de fora do que se veste**; sem nenhum asset, a conta falha com "nothing to wear".
   5. **Cabeça vence corpo** (`head_bundle_wins`): o pacote de corpo (bundle tipo 1) e o de cabeça dinâmica (tipo 4) trazem, os dois, peças da mesma parte (a cabeça, asset type 17). Para o mesmo `assetType`, sai a peça do pacote de corpo e fica a do pacote de cabeça; o resto do corpo (tronco, braços, pernas) continua. Peça sem `assetType` (0) nunca entra no conflito.
   6. resultado: `set-wearing-assets` recusado (não 2xx) → `failed` com `Failed to wear assets (status N)`; cada id que volta em `invalidAssetIds` soma em `missing` (não foi vestido); **todos** recusados → `failed` / `Roblox refused every asset`; senão `ok`.
4. Erro de uma conta (cookie vencido, avatar que falha) **não para o lote**.
5. **Cancelar** (`avatar_cancel_batch`) vale a partir do próximo passo: a conta em andamento para antes da próxima peça **e antes do próximo resgate** — a flag é olhada de novo depois da pausa de 7 s, então nenhum pedido de resgate sai depois do Cancel —, e as seguintes saem como `skipped` / `cancelled`.

### Progresso

O backend publica o retrato completo a cada passo pelo evento `avatar-batch-state` (padrão do `friend-link-state`): `{ running, total, done, currentUserId, accounts[{ userId, avatarId, status, reason, claimed, missing }] }`, com `status` em `ok | skipped | failed` e `reason` em `challenge | cancelled | texto livre`. `get_avatar_batch_state` devolve o retrato atual; a tela pode fechar e reabrir no meio do lote e retoma de onde está (o ouvinte vive enquanto o diálogo existe, não só aberto).

### Foto da conta depois do lote

Ao fim do lote a tela chama `refreshAvatarHeadshots(userIds)` no store, que **nunca apaga o headshot antigo** (a conta não fica sem foto no meio). Até 3 tentativas, 3 s entre elas: a cada uma, `invalidate_avatar_headshots` esquece o cache do backend (`ImageCache::invalidate_targets`, por prefixo `{id}:{tipo}:` — o `1:` não pega o `11:`) e `loadAvatarIds` busca de novo; só entram na tentativa seguinte as contas cujo headshot ainda veio como `Pending`. Antes, a foto antiga ficava até 6 h.

## Comandos

| Comando | O que faz |
|---|---|
| `avatar_free_catalog` | Catálogo gratuito oficial, com cache de 6 h em memória. |
| `avatar_list_saved`, `avatar_save`, `avatar_delete` | CRUD dos avatares salvos. |
| `avatar_apply_batch` | O lote (uma conta por vez); só responde no fim, com o retrato final. |
| `avatar_cancel_batch` | Pede o cancelamento do lote em andamento. |
| `get_avatar_batch_state` | Retrato atual do lote. |
| `invalidate_avatar_headshots` | Esquece o headshot em cache das contas dadas. |

## Persistência

`RAMAvatars.json` na pasta de dados: lista de `SavedAvatar { id, name, items[{ id, kind, typeId, name, collectibleItemId }], skinColor }` (camelCase). Segue o molde do `RAMScripts.json`:

- Gravação **atômica** (`.tmp` + rename) e só troca a memória depois de gravar com sucesso.
- **Latch de arquivo ilegível:** JSON corrompido ou arquivo maior que 4 MB não é carregado, a store sobe vazia e **recusa toda gravação** até o arquivo ser corrigido/restaurado e o app reiniciado — a lista vazia em memória nunca sobrescreve os avatares do usuário.
- Limites: 100 avatares, id de até 96 caracteres (`[A-Za-z0-9_-]`), nome de até 60, 12 itens por avatar. O mesmo id substitui o avatar existente na mesma posição.
- Está em `DATA_FILES` (migração de pasta de dados e backup) e em `RELOADLESS_FILES` ([commands/backups.rs](../../src-tauri/src/commands/backups.rs)): restaurar um backup com o arquivo pede reinício, porque a store não tem recarga.

## Limitações conhecidas

- Os endpoints `marketplace-*` são **sem contrato**: podem mudar sem aviso.
- **Roblox pode exigir captcha na compra** em conta nova ou suspeita. Não há contorno: a conta é pulada e o usuário resolve pelo site, se quiser.
- Aplicar avatar em lote é ação de conta; o ritmo (7 s entre resgates, uma conta por vez) existe para não parecer abuso.

## Testes

Suíte `avatars` (`bun run t avatars`):

- `avatar_catalog_tests` (Rust, wiremock) — busca paginada que filtra pago/UGC/tipos excluídos, `collectible_details`, `owns_item`, e o resgate: `Claimed`, `PriceMismatch` → `NotFree`, 403 com challenge → `ChallengeRequired`, e que preço ≠ 0 ou criador ≠ 1 nem chega a pedir.
- `avatar_store_tests` — validação, upsert/delete, persistência e latch do arquivo ilegível.
- `avatar_batch_tests` — `assign_avatars` espalha sem repetir, JSON de vestir, a conta pula peça já possuída, nunca chama `purchase-item` para item com preço ≠ 0, para no desafio sem vestir nada, expande bundles em assets, cabeça vence corpo na mesma parte, `invalidAssetIds` vira `missing` (todos recusados = `failed`), wearing recusado = `failed`, e respeita o cancelamento (inclusive o que chega logo antes de um resgate).
- `avatar_games_extra_tests` (suíte `api`) — `set_avatar` com `set-wearing-assets` recusado devolve erro.
- `avatar_cache_invalidation_tests` — `invalidate_targets` esquece só o tipo e o alvo certos (`11:` não é `1:`).
- `avatarBuilder.test.ts`, `AvatarsPage.test.tsx` e `pages/avatars/` (incluindo `useAvatarThumbs.test.tsx`) — sorteio respeitando categorias obrigatórias, validação do rascunho, as duas abas, o seletor de contas, o painel do lote, a retentativa das miniaturas, a recusa do backend traduzida e o início recusado que não repete o aviso de fim do lote anterior.
- `store.test.ts` (bloco `refreshAvatarHeadshots`) — a foto da conta depois do lote.

Resgate e vestimenta de verdade ficam fora de teste automático: precisam de uma conta real. No navegador, `?scenario=avatars&accounts=6` entrega o catálogo, dois avatares salvos e um lote em que a 1ª conta resgata 3 peças, a 2ª esbarra na verificação e as demais vestem sem resgatar nada. O teste real é do dono, numa alt, com o `.exe` gerado.
