# Autenticação

## Objetivo

Encapsular a comunicação autenticada com o Roblox a partir do cookie `.ROBLOSECURITY` de cada conta: validar cookie, obter token CSRF, gerar auth ticket para o launch, renovar a sessão quando o Roblox a invalida e operações sensíveis (senha, e-mail, PIN, quick login, display name).

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Chamadas HTTP de auth | [api/auth.rs](../../src-tauri/src/api/auth.rs) |
| Retry de sessão, refresh, comandos | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs) |
| Leitura do cookie da store | [commands/account_helpers.rs](../../src-tauri/src/commands/account_helpers.rs) (`get_cookie`) |
| Retry de rede genérico (429/erros) | [api/roblox/http.rs](../../src-tauri/src/api/roblox/http.rs) (`send_with_retry`) |
| Auto-refresh no frontend | [store.tsx](../../src/store.tsx) (efeito com `AutoCookieRefresh`) |
| Refresh manual em lote | [BottomActionBar.tsx](../../src/components/layout/BottomActionBar.tsx) (`handleRefreshAll`) |
| Uso no launch | [commands/launch.rs](../../src-tauri/src/commands/launch.rs) |

## Fluxo

### Cliente HTTP de auth

`build_client()` em [auth.rs](../../src-tauri/src/api/auth.rs): **sem seguir redirects** (`Policy::none()`) e user-agent de Chrome 120 no Windows. O cookie vai no header `Cookie: .ROBLOSECURITY=<token>`.

### Validar cookie — `validate_cookie`

1. `GET https://www.roblox.com/my/account/json`.
2. Status não-2xx → `Err("Invalid cookie (status N)")`.
3. Corpo parseado em `AccountInfo` (`UserId`, `Name`, `DisplayName`, e-mail, idade...). Usado ao adicionar contas por cookie.

### CSRF — `get_csrf_token`

1. `POST https://auth.roblox.com/v1/authentication-ticket/` sem token, com `Referer` fixo e `RBXAuthenticationNegotiation: 1`.
2. Lê o header `x-csrf-token` da resposta (o Roblox responde 403 com o header).
3. Se o header não vier: `Err("[status reason] corpo")`.

Todas as operações mutáveis pedem um CSRF novo antes (não há cache global; o único reuso é por conta dentro de `make_selected_friends`).

### Auth ticket — `get_auth_ticket`

1. Obtém CSRF.
2. `POST .../authentication-ticket/` com `x-csrf-token`, `Referer`, `RBXAuthenticationNegotiation: 1` e corpo vazio.
3. Retorna o header `rbx-authentication-ticket`; ausência → erro com status e corpo.
4. O ticket é usado para montar o launch do cliente (ver [launch.md](launch.md)) e nos links `roblox-player://` do menu de contexto (modo desenvolvedor).

### Retry de sessão — `run_with_session_retry`

```
cookie = get_cookie(user_id)
r = operação(cookie)
se ok → retorna
se erro NÃO é de sessão → retorna erro
novo_cookie = refresh_account_session(user_id)   // falha → erro
r2 = operação(novo_cookie)
se r2 falha por sessão → "Roblox invalidated this session. Re-login required."
```

`is_auth_session_error` considera erro de sessão se a mensagem (minúscula) contém: `status 401`, `[401`, `unauthorized`, `not authenticated`, `user is not authenticated`, `invalid cookie`, `authorization has been denied`, `"code":9002` ou `code 9002`. **`token validation failed` não conta** como sessão morta: é CSRF vencido (403), e tratá-lo como sessão disparava um refresh que deslogava a conta em todo lugar.

### Refresh de sessão — `refresh_account_session` / comando `refresh_cookie`

1. `mark_refresh_attempt`: grava `LastAttemptedRefresh = agora` na conta (persistido).
2. `log_out_other_sessions`: CSRF + `POST https://www.roblox.com/authentication/signoutfromallsessionsandreauthenticate`. Aceita 2xx **ou** 3xx.
3. Extrai o novo `.ROBLOSECURITY` dos headers `set-cookie`.
4. Sem cookie novo (ou vazio) → `"Roblox invalidated this session. Re-login required."`.
5. `persist_cookie_update`: grava o novo `SecurityToken` e `Valid = true`.

### Auto-refresh (frontend)

A cada **5 minutos**, se `General.AutoCookieRefresh != "false"` e o app está desbloqueado, para cada conta:
- pula se `Fields.NoCookieRefresh == "true"`;
- pula se `LastUse` tem menos de **20 dias**;
- pula se `LastAttemptedRefresh` tem menos de **7 dias**;
- senão chama `refresh_cookie` e espera 5 s antes da próxima.

O "Refresh Cookies" manual da barra inferior faz o mesmo para as contas selecionadas, sem os filtros, com 2 s entre contas.

## Regras de negócio

- Os comandos que operam "como" uma conta usam `run_with_session_retry` — no máximo **um** refresh e **uma** nova tentativa por chamada. Exceção: `make_selected_friends` (não renova sessão, ver abaixo).
- **Regra: nunca use refresh de sessão (`run_with_session_retry` / `refresh_account_session`) em leituras ou ações não críticas.** O refresh desloga a conta de **todas** as sessões e derruba clientes Roblox em execução. Para leituras tolerantes a falha, chame a API com `get_cookie` direto e trate o erro; para ações em lote, reporte a sessão inválida em vez de "consertá-la".
- O refresh **desloga todas as outras sessões** da conta (é o endpoint `signoutfromallsessionsandreauthenticate`): clientes Roblox abertos com o cookie antigo podem cair.
- `LastAttemptedRefresh` é atualizado mesmo se o refresh falhar.
- `change_password` também pode devolver um novo cookie; ele é persistido.
- `unlock_pin` exige exatamente 4 caracteres; `quick_login_*` extrai só dígitos e exige 6.
- `check_pin` retorna `true` se o PIN estiver desativado ou desbloqueado (`unlockedUntil > 0`).
- `send_with_retry` (APIs de jogos/servidores) tenta até 3 vezes em erro de rede ou HTTP 429, com espera de 400 ms e 800 ms.
- Em `make_selected_friends`, o reuso de CSRF por conta tem regra própria: erro com `token validation` ou `status 403` → busca novo CSRF e tenta 1 vez; erro de sessão (401) → **não** renova, retorna o erro com "(sessão inválida — refaça o login da conta)". A leitura de listas de amigos (`fetch_friend_set`) também não passa por `run_with_session_retry`.
- No launch, falha ao obter ticket com mensagem de moderação move a conta para o grupo `moderadas` (ver [accounts.md](accounts.md)).

## Configurações relacionadas

| Seção.Chave | Default | Efeito |
|---|---|---|
| `General.AutoCookieRefresh` | `true` | Liga o loop de auto-refresh no frontend. |
| `Fields.NoCookieRefresh` (campo da conta) | — | `"true"` exclui a conta do auto-refresh. |
| `Developer.DevMode` | `false` | Mostra "Get Auth Ticket", "rbx-player Link" e "App Link" no menu de contexto. |

## Armadilhas / cuidados

- A detecção de erro de sessão é **por substring** da mensagem de erro. Se você criar uma função de API nova, mantenha mensagens como `"... (status 401)"` para que o retry funcione; mensagens genéricas ("Failed to X") não disparam refresh.
- `test_auth(cookie)` é um comando de diagnóstico que devolve texto com validação, CSRF e ticket truncados — não use para lógica.
- O `Referer` de CSRF/ticket é uma URL fixa de jogo (`REFERER_URL` em [auth.rs](../../src-tauri/src/api/auth.rs)); mudanças no Roblox podem exigir ajustá-la.
- Contas adicionadas só por username têm `SecurityToken` vazio: todas as operações autenticadas falham e o refresh não consegue recuperar.
- Como o refresh derruba outras sessões, evitar chamá-lo durante botting/multi-launch ativos. Note que o próprio launch usa `run_with_session_retry` para o auth ticket: uma conta com sessão inválida é renovada ali (e seus clientes antigos caem).
