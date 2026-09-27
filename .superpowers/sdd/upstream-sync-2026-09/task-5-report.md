# Task 5 — presença com cookie de quem está olhando

Origem: `niccsprojects@c82a438`, `niccsprojects@ab724d2` (reimplementado, não `git apply`).

## O que mudou, por arquivo

Um único arquivo tocado: `src-tauri/src/commands/account_api.rs`.

Neste fork a parte do upstream `c82a438` (fazer `api::roblox::get_presence` aceitar
`security_token: Option<&str>`) **já existia** — é `get_presence_as` em
`src-tauri/src/api/roblox/social_presence.rs`, com `get_presence` como atalho para
`get_presence_as(None, ...)`. Não toquei nesse arquivo. Só faltava o lado do
comando Tauri (equivalente ao `ab724d2`).

- **`pick_viewer_cookie(accounts: &[data::accounts::Account]) -> Option<String>`**
  (nova, função pura): escolhe o cookie de "quem está olhando" — prefere uma
  conta `valid == true` com `security_token` não vazio; se nenhuma válida
  sobrar, cai para qualquer conta com cookie não vazio; lista vazia ou só
  contas sem cookie → `None`. Comentário no código registra a ressalva pedida
  pelo plano: o `gameId` das outras contas passa a depender da
  privacidade/config de amigos de quem foi escolhida como viewer.
- **`presence_with_viewer_cookie(state: &AccountStore, user_ids: &[i64], viewer_user_id: Option<i64>)`**
  (nova, função interna testável sem `tauri::State`, no mesmo padrão de
  `run_with_session_retry`/`read_without_refresh`): resolve o cookie (viewer
  explícito via `get_cookie`, senão `pick_viewer_cookie` no store inteiro),
  tenta `get_presence_as(Some(cookie), ids)` e, se falhar, cai para
  `get_presence(ids)` sem cookie. **Nunca** usa `run_with_session_retry` —
  comentado explicitamente no código citando a Global Constraint 2.
- **Comando `get_presence`**: ganhou `state: tauri::State<'_, AccountStore>` e
  `viewer_user_id: Option<i64>` (opcional — o padrão existente de
  `Option<i64>` sem esse argumento no frontend já é usado em outros comandos,
  ex. `get_universe_places`); o corpo agora só chama
  `presence_with_viewer_cookie(state.inner(), &user_ids, viewer_user_id)`.

Nenhum outro arquivo precisou mudar:
- `src/store.tsx` (dots Online/In Game da lista) e
  `src/components/ChooseGameScreen.tsx` (Follow a Player) continuam chamando
  `get_presence` só com `userIds` — o backend escolhe o viewer sozinho, sem
  exigir opt-in do frontend. O Follow a Player deve passar a trazer `gameId`
  com mais frequência (hoje cai no fluxo "servidor não visível" mesmo quando
  o alvo está com o servidor público, porque a chamada saía sem cookie).
- `src-tauri/src/api/roblox/social_presence.rs`: já tinha `get_presence_as` e
  os testes de cookie (`presence_cookie_tests`); não mexi.
- Nenhum texto de UI novo → sem `t()`/`i18n:extract` a rodar.

## Teste que falhou primeiro, e a sabotagem

Segui TDD por partes (função pura e depois o fluxo com HTTP mockado):

1. Escrevi 4 testes puros de `pick_viewer_cookie` em `account_api_tests`
   (`pick_viewer_cookie_prefers_a_valid_account_over_an_invalid_one`,
   `..._falls_back_to_an_invalid_account_when_none_is_valid`,
   `..._skips_accounts_with_an_empty_token`,
   `..._on_an_empty_store_is_none`) — falhavam por erro de compilação (função
   não existia). Implementei `pick_viewer_cookie` e os 4 passaram.
2. Escrevi 4 testes de integração com `wiremock` em `account_api_http_tests`
   para `presence_with_viewer_cookie` (auto-escolha de conta válida, viewer
   explícito vence a escolha automática, fallback para anônimo quando a
   chamada autenticada falha, store vazio → anônimo direto) — de novo,
   falhavam por compilação até eu escrever `presence_with_viewer_cookie` e o
   novo corpo de `get_presence`. Depois de implementado, os 8 testes (4 + 4)
   passaram.
3. **Sabotagem 1** (regra do plano — Global Constraint 3): troquei o corpo de
   `presence_with_viewer_cookie` para ignorar o cookie resolvido e sempre
   chamar `get_presence` anônimo (`let _ = cookie; api::roblox::get_presence(...)`).
   Resultado: `presence_with_viewer_cookie_uses_a_valid_accounts_cookie_when_none_is_named`
   e `..._prefers_the_explicit_viewer_over_the_stores_pick` **reprovaram** (a
   chamada bateu num mock que não existe sem cookie, erro
   `"Failed to get presence (status 404)"`). Desfiz a sabotagem.
4. **Sabotagem 2**: troquei `pick_viewer_cookie` para não filtrar mais por
   `valid` (`accounts.iter().find(|a| !a.security_token.trim().is_empty())`).
   Resultado: `pick_viewer_cookie_prefers_a_valid_account_over_an_invalid_one`
   **reprovou** (`left: Some("INVALID-TOKEN"), right: Some("VALID-TOKEN")`).
   Desfiz a sabotagem.

Depois de desfazer as duas sabotagens, rodei os 8 testes de novo — todos
verdes.

## Fallback sem cookie — como fiquei seguro

- O helper tenta a chamada autenticada primeiro e só usa `if let Ok(...)`
  para aceitar o resultado; qualquer `Err` (cookie morto, rede, 500, o que
  for) cai silenciosamente para `api::roblox::get_presence(user_ids)` sem
  cookie, cobrindo exatamente o cenário do plano ("conta com cookie morto
  apaga a presença de todas"). Coberto pelo teste
  `presence_with_viewer_cookie_falls_back_to_anonymous_when_the_authenticated_call_fails`,
  que mocka a rota autenticada como 500 e a rota anônima (sem header
  `cookie`, verificado via matcher customizado `Request`) como sucesso — e
  confirma que o resultado vem da rota anônima.
- Store vazio (nenhuma conta cadastrada) também cai direto para o anônimo,
  sem tentar `get_cookie`/painc — coberto por
  `presence_with_viewer_cookie_is_anonymous_when_the_store_is_empty`.
- `viewer_user_id` explícito que não existe no store (`get_cookie` retorna
  `Err`) também cai para `pick_viewer_cookie` do store inteiro em vez de
  propagar o erro — comportamento coberto implicitamente pelo `.and_then(...).ok()`
  em `presence_with_viewer_cookie` (não escrevi um teste dedicado para esse
  caso específico porque o comportamento de `get_cookie` já está coberto em
  `account_helpers_tests`, e a composição via `.ok()` é trivial o bastante
  para não justificar mais um teste de integração; sinalizado aqui para quem
  revisar).

## `bun run check`

Rodei em duas etapas: primeiro `bun run t accounts`, `bun run t api` e
`bun run t friends` durante a iteração (todos verdes), depois o `bun run
check` completo antes do commit:

```
$ bun run typecheck && bun run test:audit && bun run test && bun run test:rust
...
Auditoria ok: toda suíte de teste está mapeada em scripts/test-suites.ts
...
 Test Files  68 passed (68)
      Tests  1372 passed (1372)
...
running 1362 tests
...
test result: ok. 1362 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.17s
...
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s   (security_regression_scripts_store)
[exited with code 0]
```

`bun run check` saiu com **código 0**: typecheck limpo, auditoria de suítes
ok, vitest 68 arquivos/1372 testes verdes, `cargo test --all-features` 1362
verdes/0 falhas — **incluindo** os 4 testes de
`platform::windows::singleton_event_tests` citados como flaky na tarefa
(`a_named_event_of_ours_is_found_closed_and_disappears`,
`closing_leaves_similar_names_untouched`,
`the_event_type_index_is_discovered_and_matches_one_of_our_events`,
`the_handle_snapshot_is_readable_and_lists_this_process`): passaram nesta
execução, não precisei aplicar a tolerância.

## O que ficou de fora

- Nenhum ajuste em `src/store.tsx` ou `ChooseGameScreen.tsx`: o plano só pede
  que o comando aceite `viewer_user_id` opcional e escolha sozinho quando
  ausente — não pede (nem faz sentido, dado que a escolha automática já
  resolve o caso de uso) que o frontend passe explicitamente qual conta usar
  como viewer.
- Não toquei `docs/features/accounts.md`, `docs/features/friends.md` nem
  `docs/features/ui-layout.md`: a única regra de negócio nova
  (auto-seleção de viewer + fallback) fica inteiramente dentro do comando
  `get_presence`, que hoje não tem uma seção própria em nenhum desses
  documentos (o texto existente sobre `get_presence`/`get_presence_as` em
  `friends.md` é sobre outro call site, dentro de `friends.rs`, que não
  mudou). Se um revisor achar que vale uma linha de doc, é barato adicionar
  depois — deixei de fora para não misturar uma tarefa de doc com o escopo
  exato pedido.
- Não escrevi um teste de integração dedicado para "viewer_user_id aponta
  para conta inexistente" (ver seção de fallback acima) — comportamento
  coberto por composição de partes já testadas individualmente
  (`get_cookie` erro conhecido + `pick_viewer_cookie` já testado).
