# Avatares grátis — design

Data: 02/10/2026. Status: aprovado em conversa, aguardando revisão da spec.

## Objetivo

Dar visual às contas secundárias sem gastar Robux: o dono monta (à mão ou
sorteando) alguns avatares só com **itens grátis oficiais do Roblox**, salva, e
depois distribui esses avatares aleatoriamente entre as contas selecionadas. O
app pega as peças que faltam em cada conta e veste.

## Decisões tomadas

- **Só itens oficiais do Roblox** (`CreatorName=Roblox`, preço 0). Nada de UGC.
- **Distribuição sorteia dos avatares salvos**, espalhando: cada avatar salvo é
  usado antes de qualquer um repetir. Não existe "gerar um inédito por conta".
- **Nunca gastar Robux**, com duas travas independentes (ver "Pegar a peça").
- Fora desta versão: UGC, animações, emotes, roupas em camadas (layered
  clothing, asset types 64–72), escalas do corpo.

## O que o catálogo oferece (medido em 02/10/2026)

`GET catalog /v2/search/items/details?Category=1&CreatorName=Roblox&MaxPrice=0&MinPrice=0&SalesTypeFilter=1&Limit=120`
devolve **238 itens em 3 páginas** (sem login): 186 assets e 52 bundles. Todos
têm `collectibleItemId`. Assets relevantes: chapéu (8) 36, cabelo (41) 68,
camisa (11) 14, calça (12) 8, camiseta (2) 2, acessórios de rosto/pescoço/
ombro/costas (42/43/44/46) 14. Bundles: pacotes de corpo (bundleType 1) 40 e
cabeças (bundleType 4) 7; os demais tipos (animações) ficam de fora.

Rosto clássico grátis (asset type 18) não aparece: o rosto vem com o pacote de
corpo/cabeça.

## Componentes

### Backend (Rust)

`src-tauri/src/api/roblox/avatar_catalog.rs` (novo, `include!`-ado em
`api/roblox.rs` como os vizinhos), todas as URLs via `endpoints::host(...)`:

- `search_free_official_items()` — pagina a busca acima (pausa de 1,5 s entre
  páginas), devolve `Vec<FreeCatalogItem>` com `id`, `item_type` (Asset/Bundle),
  `asset_type`/`bundle_type`, `name`, `collectible_item_id`, `creator_target_id`,
  `price`. Filtra fora preço ≠ 0, criador ≠ 1 e os tipos excluídos — mesmo que a
  busca já filtre, a lista que chega à UI nunca tem item pago.
- `collectible_details(ids)` — `POST apis /marketplace-items/v1/items/details`
  `{itemIds}` (sem login): devolve `collectibleProductId`, `price`, `creatorId`.
- `bundle_assets(bundle_id)` — `GET catalog /v1/bundles/{id}/details`: as peças
  de corpo/cabeça do pacote (sem o `UserOutfit`), que é o que se veste.
- `owns_item(cookie, user_id, kind, id)` — `GET inventory
  /v1/users/{uid}/items/{Asset|Bundle}/{id}/is-owned` com o cookie da própria
  conta (inventário privado responde 403 sem ele). Leitura: usa o caminho
  `read_without_refresh`, nunca `run_with_session_retry`.
- `claim_free_item(cookie, user_id, details)` — `POST apis
  /marketplace-sales/v1/item/{collectibleItemId}/purchase-item` com
  `{collectibleItemId, collectibleProductId, expectedCurrency:1,
  expectedPrice:0, expectedPurchaserId, expectedPurchaserType:"User",
  expectedSellerId, expectedSellerType:"User", idempotencyKey}` (uuid v4), CSRF
  com `send_with_csrf_retry`. Resultado tipado:
  `Claimed | AlreadyOwned | NotFree | ChallengeRequired | Failed(String)`.
  `ChallengeRequired` = 403 com cabeçalho `rblx-challenge-id` /
  `rblx-challenge-metadata` (captcha ou 2 etapas).

Store `src-tauri/src/data/avatars.rs` (novo): `RAMAvatars.json` na pasta de
dados, lista de `SavedAvatar { id, name, items: Vec<AvatarItemRef>,
body_colors: Option<BodyColors> }`. Entra em `DATA_FILES` (migração de pasta e
backup), como o `RAMScripts.json`.

Comandos `src-tauri/src/commands/avatars.rs` (novo):

- `avatar_free_catalog()` — catálogo, em cache na memória por 6 h.
- `avatar_list_saved` / `avatar_save` / `avatar_delete`.
- `avatar_apply_batch(user_ids, avatar_ids)` — o lote (abaixo). Guarda de
  execução única, como o `try_begin_friend_link`.
- `avatar_cancel_batch()`.

### Lote — o que acontece por conta

As contas rodam **uma de cada vez**. Antes de começar, `assign_avatars(user_ids,
avatar_ids, rng)` decide quem veste o quê: embaralha os avatares e reparte em
rodízio, de modo que nenhum avatar se repete enquanto houver outro sem uso
(função pura, testada).

Para cada conta:

1. Para cada peça do avatar (pacote = o bundle inteiro): `owns_item`; se já
   possui, pula.
2. Se não possui: `collectible_details` e confere **preço 0 e criador 1**
   (trava 1). Diferente disso → `NotFree`, peça pulada. Então
   `claim_free_item` com `expectedPrice: 0` (trava 2: o Roblox recusa com
   `PriceMismatch` se o preço mudou). Entre duas peças pegas na mesma conta,
   **7 s** de pausa (limite do Roblox: 9 compras por minuto por usuário).
3. `ChallengeRequired` em qualquer peça → a conta para ali, fica marcada
   "Roblox pediu verificação — pulada", e o lote segue para a próxima.
4. Vestir: monta o `avatar_json` (assets do avatar + peças dos pacotes + cores)
   e chama o `set_avatar` que já existe. Peça que não foi pega fica de fora do
   que se veste.
5. Atualizar a foto da conta: invalida a entrada da conta no `ImageCache` e no
   `store.avatarUrls` (hoje a foto antiga fica até 6 h).

Erros de uma conta não param o lote. Progresso pelo evento
`avatar-batch-state` (snapshot completo a cada passo, no padrão do
`friend-link-state`): conta atual, `i/N`, e por conta
`ok | skipped(motivo) | failed(motivo)` com a contagem de peças pegas. Cancelar
termina depois da peça atual.

### Frontend

`src/components/dialogs/AvatarsDialog.tsx` (novo), aberto por um botão na
barra de ferramentas. Duas áreas:

- **Montar**: categorias (Cabelo, Chapéu, Acessório, Camisa, Calça, Camiseta,
  Corpo, Cabeça) com os ícones dos itens (`get_asset_thumbnails` / thumbnails de
  bundle, 150x150). Uma peça por categoria (Acessório aceita até 3). Cores da
  pele: paleta curta de tons. **🎲 Sortear** tudo ou só uma categoria
  (categoria pode ficar vazia no sorteio, menos Camisa/Calça/Corpo). Prévia =
  grade com os ícones escolhidos — o Roblox não renderiza o corpo inteiro de um
  avatar que não está em conta nenhuma. Salvar com nome.
- **Distribuir**: lista dos avatares salvos (marcáveis), contas selecionadas na
  lista principal, botão "Aplicar avatares", progresso e resumo final.

Lógica pura em `src/avatarBuilder.ts` (+ teste): categorias, sorteio de
avatar, validação do avatar salvo.

## Testes

- Rust (wiremock): busca paginada filtrando pago/UGC/tipos excluídos;
  `collectible_details`; `owns_item`; `claim_free_item` → `Claimed`,
  `PriceMismatch`→`NotFree`, 403 com challenge → `ChallengeRequired`;
  o lote pula peça já possuída, nunca chama `purchase-item` para item com
  preço ≠ 0, e para a conta (sem parar o lote) no challenge. `assign_avatars`
  espalha sem repetir enquanto houver avatar livre.
- Frontend: `avatarBuilder` (sorteio respeita categorias obrigatórias);
  `AvatarsDialog` no harness com cenário `avatars` (catálogo, lote com uma
  conta pedindo verificação).
- Suíte nova `avatars` em `scripts/test-suites.ts`.
- Teste real: o dono, numa alt, com o `.exe` gerado.

## Riscos

- Roblox pode exigir captcha na compra para contas novas/suspeitas — por isso o
  `ChallengeRequired` existe e não há tentativa de resolver captcha.
- Os endpoints `marketplace-*` não são documentados oficialmente; se mudarem, o
  lote falha por peça com a mensagem do Roblox, sem gastar nada.
- Aplicar avatar em lote em muitas contas é ação de conta; o ritmo (7 s entre
  peças, uma conta por vez) é para não parecer abuso.

## Documentação

`docs/features/avatars.md` novo; `docs/features/accounts.md` (tabela de
comandos); `docs/README.md` (índice); `CLAUDE.md` (mapa: `avatars.rs`,
`avatar_catalog.rs`, `AvatarsDialog`).
