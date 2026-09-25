# Escolha de servidor (preferência e região)

## Objetivo

Decidir **em qual servidor público** o lote entra, em vez de deixar o Roblox escolher: aleatório, o mais vazio ou o mais cheio — opcionalmente exigindo um país (o caso concreto: garantir servidor brasileiro) — e permitir escolher o servidor na mão, vendo a lista.

## Onde fica o código

| Peça | Arquivo |
|---|---|
| Preferência e ranking | [api/roblox/server_pick.rs](../../src-tauri/src/api/roblox/server_pick.rs) (`ServerPreference`, `rank_servers`, `pick_server`) |
| Região do servidor | [api/roblox/server_regions.rs](../../src-tauri/src/api/roblox/server_regions.rs) (`resolve_server_region`, `lookup_ip_region`, `format_region`) |
| Comandos Tauri | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs) (`pick_server`, `get_server_regions`) |
| Aba Servers (Choose Game) | [components/servers/ServersTab.tsx](../../src/components/servers/ServersTab.tsx) |
| Navegador de servidores (diálogo) | [components/server-list/ServersTab.tsx](../../src/components/server-list/ServersTab.tsx) |
| Estado/persistência | [store.tsx](../../src/store.tsx) → `General.ServerPreference`, `General.ServerRegionFilter` |
| Evento | `server-region-progress` → `{ done, total }` |
| Suíte de teste | `bun run t servers` |

## Fluxo

### Preferência no launch

1. O usuário escolhe a preferência na aba **Servers** da Choose Game (`Default` / `Random` / `Emptiest` / `Fullest`) e, se quiser, um país.
2. Ao lançar **sem** servidor escolhido, `launchAll` chama `pick_server` **uma vez** para o lote.
3. O backend pede a lista com o `sortOrder` da preferência (`Asc` = menos jogadores, `Desc` = mais — confirmado contra a API) e `excludeFullGames=true`, descarta quem não cabe o lote, e devolve o Job ID.
4. Todas as contas entram naquele Job ID pelo `launch_multiple`.

### Região

1. A lista de servidores **não** traz região. Para cada servidor: `join-game-instance` → `joinScript.MachineAddress` (o IP da máquina) → geolocalização (`ipwho.is`, com `ip-api.com` de reserva).
2. Na aba Servers isso só acontece quando o usuário clica **Load regions**, em lotes de 10.
3. Com filtro de país no launch, `pick_server` percorre os candidatos **na ordem da preferência** e para no primeiro que bate.

## Regras de negócio

- **Um servidor por lote, não um por conta.** O `shuffleJob` antigo sorteia dentro do laço de cada conta, o que espalha o lote; a preferência resolve antes e manda todo mundo para o mesmo Job ID.
- **O servidor tem que caber o lote inteiro** (`playing + contas <= maxPlayers`). Se nenhum couber, usa os que têm alguma vaga — travar o launch seria pior.
- **Job ID explícito, VIP e Follow vencem a preferência**: ela só entra quando o usuário não escolheu servidor.
- **Falhar ao escolher não cancela o launch**: sem servidor resolvido o lote segue com Job vazio (comportamento antigo), com um toast explicando.
- **Sem servidor no país pedido, a UI pergunta.** O backend devolve o melhor disponível com `regionFallback: true`; entrar calado no país errado é exatamente o que o usuário quer evitar.
- **Região é cara**: uma chamada de `join-game-instance` por servidor, com pausa de ~0,9 s e teto de 40 por pedido. Nunca é resolvida automaticamente ao abrir a lista.
- **Cada IP é geolocalizado uma vez.** O cache fica em memória e em `ServerRegionCache.json` na pasta de dados — é cache descartável, fora de `DATA_FILES`, então não entra em backup nem em migração.
- Na lista, servidor **sem região resolvida continua visível** mesmo com filtro ligado: escondê-lo faria parecer que não existe servidor naquele país.

## Configurações relacionadas

| Chave (`RAMSettings.ini`, seção `General`) | O quê |
|---|---|
| `ServerPreference` | `default` \| `random` \| `emptiest` \| `fullest` |
| `ServerRegionFilter` | Código do país exigido (`BR`); vazio = sem filtro |
| `ServerRegionFormat` | Template do rótulo: `<city>`, `<region>`, `<country>`, `<countryCode>`, `<ip>` |

## Armadilhas / cuidados

- O `join-game-instance` é o **mesmo** endpoint que o cliente usa para entrar no jogo. Resolver região em rajada toma 429 e marca a conta — a pausa e o teto existem por isso.
- A ordem tem que ser pedida à **API**, não refeita na página carregada: a resposta traz no máximo 100 servidores e um jogo grande tem milhares, então reordenar localmente mostra "o mais cheio entre os mais vazios" (foi o bug de a aba exibir 3/13 em tudo com "Fullest"). O backend ainda reordena o que recebeu, porque paginação e cache do Roblox já devolveram páginas fora de ordem.
- O serviço de geolocalização pode cair (o `ipapi.co` que o app usava antes passou a exigir desafio do Cloudflare e a coluna Region parou de funcionar). Por isso há reserva e, na pior hipótese, o rótulo vira o IP cru em vez de "erro".
- Os Job IDs mudam a cada refresh da lista; as regiões resolvidas são descartadas junto.
