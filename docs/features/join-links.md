# Join links (convites, VIP e links de jogo)

## Objetivo

Permitir colar **qualquer link de entrada do Roblox** num único campo e entrar com todas as contas selecionadas — sem separar campos, sem cadastrar o servidor VIP antes e sem precisar extrair Place/Job ID na mão.

Tipos aceitos:

| Tipo | Exemplo | `kind` resolvido |
|---|---|---|
| Convite de experiência | `https://www.roblox.com/share?code=<code>&type=ExperienceInvite` | `invite` |
| Servidor VIP/privado | `https://www.roblox.com/games/<placeId>/Nome?privateServerLinkCode=<code>` | `private` |
| Share link de servidor | `https://www.roblox.com/share?code=<code>&type=Server` | `private` |
| Código VIP direto | `vip:<code>` | `private` |
| Servidor específico | `https://www.roblox.com/games/start?placeId=<id>&gameInstanceId=<jobId>` | `job` |
| Jogo (servidor público) | `https://www.roblox.com/games/<placeId>/Nome` | `place` |
| Deep link | `roblox://experiences/start?placeId=...`, `roblox://navigation/share_links?type=...&code=...` | conforme o conteúdo |
| Link curto | `https://ro.blox.com/Ebh5?af_dp=...` (AppsFlyer) ou um redirect | conforme o destino |

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Parser puro + resolução | [api/roblox/join_links.rs](../../src-tauri/src/api/roblox/join_links.rs) (`parse_join_link`, `resolve_join_link`, `JoinTarget`) |
| POST share-link (compartilhado) | [api/roblox/private_links.rs](../../src-tauri/src/api/roblox/private_links.rs) (`resolve_share_link_payload`, `resolve_share_server_link`) |
| Comando Tauri | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs) (`resolve_join_link`) |
| Registro do comando | [lib.rs](../../src-tauri/src/lib.rs) |
| UI (aba Follow) | [ChooseGameScreen.tsx](../../src/components/ChooseGameScreen.tsx) (`JoinLinkSection`, `FollowTab`, `useLauncher`) |
| Launch a partir do alvo | [store.tsx](../../src/store.tsx) (`LaunchTarget`, `joinServer`, `launchMultiple`) |
| Tipo no frontend | [types.ts](../../src/types.ts) (`JoinTarget`) |

## Fluxo

1. O usuário cola o link na seção **Join link** (topo da aba *Follow* da tela Choose Game) e aperta Enter ou "Join".
2. O frontend chama `invoke("resolve_join_link", { userId, link })` com a **primeira** conta selecionada.
3. O backend chama `parse_join_link` (puro, sem rede). Se o link já traz tudo (place/job/link code), responde sem usar a conta.
4. Se o link tem um **share code**, chama `POST https://apis.roblox.com/sharelinks/v1/resolve-link` com o cookie da conta e CSRF — via `send_with_csrf_retry`, porque o `apis.roblox.com` recusa o token do `auth.roblox.com` ("XSRF token invalid") e devolve o dele no 403 (ver [authentication.md](authentication.md#o-token-é-por-serviço--send_with_csrf_retry)):
   - tipo `ExperienceInvite` (ou desconhecido) → lê `experienceInviteData` → `placeId`, `instanceId` (vira `jobId`), `launchData`, `inviterId`, `status`;
   - tipo `Server` → caminho já existente (`resolve_share_server_link`) → `placeId` + `linkCode`.
5. Devolve um `JoinTarget` e a UI mostra o que foi resolvido ("Private server · place 606849621").
6. O launch usa o alvo resolvido:

| `kind` | Place | Job enviado | Extras |
|---|---|---|---|
| `private` | resolvido | `""` | `joinVip: true`, `linkCode` (ou `accessCode`) |
| `invite` | resolvido | `instanceId` | `launchData` do convite |
| `job` | resolvido | `gameInstanceId` | — |
| `place` | resolvido | `""` | — |

## Regras de negócio

- O `placeId` resolvido do link **sempre vence** o `placeId` que está no estado da tela; ele é passado explicitamente para o launch (o estado do React é assíncrono e já causou o bug de entrar no jogo anterior).
- `launch_multiple` **não tem** parâmetros `joinVip`/`linkCode`: para VIP, o job vai codificado como `vip:<code>`, que `resolve_launch_job` entende. O launch de conta única passa `joinVip`/`linkCode` explicitamente.
- `accessCode` é enviado pelo mesmo parâmetro `linkCode`: `resolve_private_join` reconhece um valor com 5 grupos separados por `-` como access code.
- Um convite com `status` diferente de `Valid` (`Expired`, `InviterNotInExperience`) **não é erro**: vira o campo `note`, a UI avisa e o launch continua no servidor público do mesmo place.
- Convite sem `placeId` → erro (não dá para entrar em lugar nenhum).
- Link de servidor privado **sem place** (ex.: só `?privateServerLinkCode=` sem `/games/<id>`) → erro pedindo o link completo.
- O cookie da conta só é usado quando há share code para resolver; os outros formatos são resolvidos offline.
- Um único resolve serve para todas as contas selecionadas (evita N chamadas autenticadas e N chances de rate limit).

## Configurações relacionadas

Nenhuma configuração nova. O launch resultante respeita as opções normais (`General.EnableMultiRbx`, `Developer.UseOldJoin`, isolamento, versões) — ver [launch.md](launch.md).

## Armadilhas / cuidados

- O parser vive em `join_links.rs`, mas **existe parsing parecido** em [launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs) (`resolve_launch_job`) e em `private_links.rs` (`normalize_private_server_link_code`), porque o campo "Job ID" da tela aceita link também. Ao mudar regra de link, verifique os três.
- Convites de uso único podem falhar da segunda conta em diante — o resolve funciona, a entrada é que não.
- Se o convite aponta para um servidor cheio ou que já fechou, o cliente cai num servidor público sem avisar (comportamento do Roblox).
- `launchData` tem limite de ~200 bytes no Roblox.
- O parser é coberto por testes unitários (`mod join_link_tests`, `cargo test --lib join_link`). Ao aceitar um formato novo de link, **acrescente um teste** com o link real antes de mexer na lógica.
