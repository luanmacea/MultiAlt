# Escolha de servidor (preferência e região)

## Objetivo

Decidir **em qual servidor público** o lote entra, em vez de deixar o Roblox escolher: aleatório, o mais vazio ou o mais cheio — opcionalmente exigindo um país (o caso concreto: garantir servidor brasileiro) — e permitir escolher o servidor na mão, vendo a lista.

## Onde fica o código

| Peça | Arquivo |
|---|---|
| Preferência e ranking | [api/roblox/server_pick.rs](../../src-tauri/src/api/roblox/server_pick.rs) (`ServerPreference`, `rank_servers`, `pick_server`) |
| Região do servidor | [api/roblox/server_regions.rs](../../src-tauri/src/api/roblox/server_regions.rs) (`resolve_server_region`, `lookup_ip_region`, `format_region`) |
| Comandos Tauri | [commands/account_api.rs](../../src-tauri/src/commands/account_api.rs) (`pick_server`, `list_servers_ranked`, `start_server_scan`, `stop_server_scan`, `get_server_regions`) |
| Aba Servers (Choose Game) | [components/servers/ServersTab.tsx](../../src/components/servers/ServersTab.tsx) |
| Navegador de servidores (diálogo) | [components/server-list/ServersTab.tsx](../../src/components/server-list/ServersTab.tsx) |
| Estado/persistência | [store.tsx](../../src/store.tsx) → `General.ServerPreference`, `General.ServerRegionFilter` |
| Evento | `server-region-progress` → `{ done, total }` |
| Suíte de teste | `bun run t servers` |

## Fluxo

### Preferência no launch

1. O usuário escolhe a preferência na aba **Servers** da Choose Game e, se quiser, um país.

   | Preferência | O que escolhe |
   |---|---|
   | **Best fit** (padrão) | O mais cheio que ainda caiba o lote **com uma vaga de folga** |
   | Fullest | O mais cheio com vaga para o lote |
   | Emptiest | O mais vazio |
   | Random | Qualquer um com vaga |
   | Let Roblox choose | Não escolhe: Job ID vazio (comportamento antigo) |
2. Ao lançar **sem** servidor escolhido, `launchAll` chama `pick_server` **uma vez** para o lote.
3. O backend pede a lista com o `sortOrder` da preferência (`Asc` = menos jogadores, `Desc` = mais — confirmado contra a API) e `excludeFullGames=true`, descarta quem não cabe o lote, e devolve o Job ID.
4. No **Best fit** ele desce as páginas de `Desc` (até 6) até achar a faixa que cabe o lote: num jogo popular a primeira página é toda de servidores com uma ou duas vagas, e cair no `Asc` traria os vazios, que é justamente o que não se quer.
4. Todas as contas entram naquele Job ID pelo `launch_multiple`.

### Região

1. A lista de servidores **não** traz região. Para cada servidor: `join-game-instance` → `joinScript.MachineAddress` (o IP da máquina) → geolocalização (`ipwho.is`, com `ip-api.com` de reserva).
2. Na aba Servers isso só acontece quando o usuário clica **Load regions**, em lotes de 10.
3. Com filtro de país no launch, `pick_server` percorre os candidatos **na ordem da preferência** e para no primeiro que bate.

## Varredura da lista (aba Servers)

Um jogo grande tem milhares de servidores e a API devolve 100 por página; com um lote de 6 contas, as primeiras páginas podem não ter **nenhum** servidor que caiba todo mundo. Por isso a aba não espera o fim: `start_server_scan` percorre as páginas em background (250 ms entre elas) e publica um evento `server-scan` a cada página, já com a lista reordenada. A varredura para quando acha 12 servidores que cabem o lote, quando as páginas acabam, quando bate o limite de páginas, ou quando o usuário troca de jogo/ordem (`stop_server_scan`).

**Quantas páginas** vem do campo *Pages to scan* (padrão 30 = 3000 servidores), guardado em `General.ServerScanPages`. O backend aplica um teto absoluto de 500 páginas: uma varredura sem fim martelaria a API do Roblox. Quando a varredura para **por causa do limite** (e não por falta de servidores), o evento traz `stoppedAtLimit` e a aba oferece dobrar o número em vez de dizer que acabou.

**Sem nenhum servidor que caiba o lote**, a ordem deixa de ser a preferência e passa a ser **quantas contas cabem** — mais vagas primeiro, e entre iguais o mais cheio. Antes a lista abria com os de uma vaga só e os que levavam quatro contas ficavam escondidos lá embaixo.

A aba **reordena a lista recebida** antes de desenhar (`rankRows`), com o número de contas que está selecionado naquele momento: cabe com folga → cabe justo → não cabe (mais vagas primeiro). O backend já manda ordenado, mas quem tem certeza do tamanho do lote é a tela, e é ela que escreve os rótulos ("3 free · needs 6") — as duas coisas precisam concordar. Pelo mesmo motivo, o recorte dos 150 servidores enviados por evento coloca os que cabem o lote **antes** do corte.

Os eventos são aceitos pelo **maior `scanId` já visto**, não pelo id que o `start_server_scan` devolveu: duas varreduras seguidas resolvem o `invoke` fora de ordem, e comparar com o id "atual" podia descartar os eventos da varredura nova e deixar na tela a primeira página da antiga.

### Servidor repetido entre páginas

A lista do Roblox se mexe entre uma requisição e outra (jogador entra, jogador sai), então **o mesmo Job ID volta em páginas diferentes** — em dados reais de um jogo grande, 50 repetidos em 400 servidores, 33 deles só entre a primeira e a segunda página.

A varredura junta as páginas **deduplicando por Job ID** (`extend_unique`), e a aba faz o mesmo antes de desenhar (`dedupeRows`). Sem isso:

- a contagem de "servidores examinados" mentia;
- e, pior, cada repetido virava uma **chave de lista duplicada** no React, que então parava de reordenar a tabela — a lista subia os servidores bons e travava no meio do caminho, que foi o sintoma relatado (o DOM chegava a ter 183 linhas para uma lista de 150).

## Regras de negócio

- **Um servidor por lote, não um por conta.** O `shuffleJob` antigo sorteia dentro do laço de cada conta, o que espalha o lote; a preferência resolve antes e manda todo mundo para o mesmo Job ID.
- **O servidor tem que caber o lote inteiro** (`playing + contas <= maxPlayers`). Se nenhum couber, usa os que têm alguma vaga — travar o launch seria pior.
- **O Best fit deixa uma vaga de folga** (`FREE_SEAT_BUFFER`): entrar num servidor que fica lotado no instante seguinte impede qualquer um de entrar depois, inclusive uma conta que caiu e voltou. O teto (`maxPlayers - contas - 1`) **nunca é negativo**, e a folga é preferência, não regra: sem nenhum servidor com folga, vale o que couber.
- **Job ID explícito, VIP e Follow vencem a preferência**: ela só entra quando o usuário não escolheu servidor.
- **Falhar ao escolher não cancela o launch**: sem servidor resolvido o lote segue com Job vazio (comportamento antigo), com um toast explicando.
- **Sem servidor no país pedido, a UI pergunta.** O backend devolve o melhor disponível com `regionFallback: true`; entrar calado no país errado é exatamente o que o usuário quer evitar.
- **Região é cara**: uma chamada de `join-game-instance` por servidor, com pausa de ~0,9 s e teto de 40 por pedido. Nunca é resolvida automaticamente ao abrir a lista.
- **Cada IP é geolocalizado uma vez.** O cache fica em memória e em `ServerRegionCache.json` na pasta de dados — é cache descartável, fora de `DATA_FILES`, então não entra em backup nem em migração.
- Na lista, servidor **sem região resolvida continua visível** mesmo com filtro ligado: escondê-lo faria parecer que não existe servidor naquele país.

## Configurações relacionadas

| Chave (`RAMSettings.ini`, seção `General`) | O quê |
|---|---|
| `ServerPreference` | `bestfit` (padrão) \| `fullest` \| `emptiest` \| `random` \| `none`. O valor antigo `default` é lido como `bestfit`. |
| `ServerRegionFilter` | Código do país exigido (`BR`); vazio = sem filtro |
| `ServerRegionFormat` | Template do rótulo: `<city>`, `<region>`, `<country>`, `<countryCode>`, `<ip>` |

## Armadilhas / cuidados

- O `join-game-instance` é o **mesmo** endpoint que o cliente usa para entrar no jogo. Resolver região em rajada toma 429 e marca a conta — a pausa e o teto existem por isso.
- A ordem tem que ser pedida à **API**, não refeita na página carregada: a resposta traz no máximo 100 servidores e um jogo grande tem milhares, então reordenar localmente mostra "o mais cheio entre os mais vazios" (foi o bug de a aba exibir 3/13 em tudo com "Fullest"). O backend ainda reordena o que recebeu, porque paginação e cache do Roblox já devolveram páginas fora de ordem.
- O serviço de geolocalização pode cair (o `ipapi.co` que o app usava antes passou a exigir desafio do Cloudflare e a coluna Region parou de funcionar). Por isso há reserva e, na pior hipótese, o rótulo vira o IP cru em vez de "erro".
- Os Job IDs mudam a cada refresh da lista; as regiões resolvidas são descartadas junto.
