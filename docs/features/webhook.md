# Avisos no Discord (webhook)

## Objetivo

Avisar num canal do Discord quando algo acontece com as contas sem a pessoa estar olhando o
app: a conta caiu (com o motivo), a reconexão desistiu, a janela travou, o launch falhou, a
conta reconectou. A pessoa cola a URL de um webhook **dela** e escolhe quais avisos quer. É a
ideia 5 de [ideias-de-outros-gerenciadores.md](../ideias-de-outros-gerenciadores.md).

**Desligado por padrão.** Ligado, as informações **saem do PC** — a tela diz isso no próprio
interruptor.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Parte pura e envio: conferência da URL, filtro de eventos, mensagem, fila com lote e limite, POST | [api/discord_webhook.rs](../../src-tauri/src/api/discord_webhook.rs) |
| Cola: settings, nome da conta, pontos onde os eventos nascem, task que manda, "Send test message" | [commands/webhook.rs](../../src-tauri/src/commands/webhook.rs) |
| Pontos de origem | [client_health.rs](../../src-tauri/src/commands/client_health.rs) (`webhook_after_health_tick` no laço do monitor), [reconnect.rs](../../src-tauri/src/commands/reconnect.rs) (`publish_reconnect`), [launch.rs](../../src-tauri/src/commands/launch.rs) (falha no auth ticket e ao abrir o cliente, launch de uma conta e fila) |
| Segunda trava da URL ao gravar | `update_setting` em [data/settings/commands.rs](../../src-tauri/src/data/settings/commands.rs) → `validate_setting` |
| Tela | Settings › **Notifications** ([NotificationsTab.tsx](../../src/components/settings/NotificationsTab.tsx)); conferência da URL espelhada em [utils/discordWebhook.ts](../../src/utils/discordWebhook.ts) |
| Chaves | `[Webhook]` em [settings.md](settings.md#webhook--detalhes-em-webhookmd) |

## Fluxo

1. Settings › Notifications: liga **Send notifications to Discord**, cola a URL (campo de senha;
   grava ao sair do campo, só se for válida) e, se quiser, um texto antes de cada mensagem
   (`@everyone`, `<@&id do cargo>`). **Send test message** manda na hora, para a URL do campo
   (salva ou não), um cartão "✅ Webhook conectado" com os avisos marcados e um **exemplo** de queda
   com a primeira conta do app — exatamente como os avisos vão chegar.
2. Um evento acontece no backend → `notify_webhook(app, tipo, conta, motivo, place)`: lê a seção
   `[Webhook]`; se a opção, a URL e a caixa daquele evento estão ligadas, monta o nome da conta
   (à mostra por padrão; mascarado com **Hide account names**) e põe na fila. **Sem rede aqui**: só lock de settings e da fila — nunca
   atrasa launch nem reconexão.
3. `start_webhook_dispatcher` (task do tokio ligada no `setup`) olha a fila a cada 1 s:
   - **Lote:** espera 4 s depois do primeiro aviso para juntar os vizinhos (`BATCH_WINDOW_MS`);
     até 10 linhas por mensagem (`MAX_BATCH`). Cinco quedas em dois segundos = uma mensagem.
   - **Limite do Discord:** no máximo 5 envios a cada 2 s por webhook; um 429 devolve o lote
     para a frente da fila e nada sai antes do `retry_after`.
   - **Teto:** 50 avisos na fila (o mais antigo sai) e aviso com mais de 10 min é descartado.
   - Desligou a opção (ou a URL ficou inválida) com aviso na fila: o lote é descartado.
   - Erro de rede ou 4xx/5xx: vai para o log **sem URL e sem token**, e o lote não é repetido.

## Eventos

| Caixa | Chave | Padrão | De onde vem |
|---|---|---|---|
| An account dropped or its client closed unexpectedly | `NotifyDropped` | ligado | `HealthNotice::Dropped` (motivo do log: perdeu a conexão, entrou em outro lugar, parada demais, expulso com a mensagem do jogo, servidor fechou, cliente fechou sem sair) |
| Auto-reconnect gave up | `NotifyReconnectFailed` | ligado | `ReconnectNotice::GaveUp` e `Stopped` por ban, sessão expirada ou sem destino. Fechado pela pessoa, conta em outro lugar ou aberta pelo site **não** é falha |
| A client stopped responding | `NotifyNotResponding` | ligado | `HealthNotice::NotResponding { true }` (30 s travada) |
| An account reconnected | `NotifyReconnected` | desligado | `ReconnectNotice::Reconnected` |
| An account opened as a different Roblox account | `NotifyWrongAccount` | ligado | O cliente lançado para uma conta entrou logado em outra (`HealthNotice::WrongAccount`, ver client health). A fila foi parada e o cliente fica aberto. Vai sem o id da outra conta |
| A launch failed | `NotifyLaunchFailed` | desligado | Falha no auth ticket / ao abrir o cliente (launch de uma conta e fila). Vai **só a etapa**, nunca o texto do erro (pode ter URL de launch, ticket ou código de VIP) |

"Conta aberta como outra conta do Roblox" ficou de fora: o evento ainda não existe no backend.

## Mensagem

Redesenhada em 15/10/2026 a pedido do dono ("o nome não apareceu, está feio — põe título, o que
aconteceu"). Antes era um embed com uma linha em inglês por aviso e o nome mascarado.

**Um cartão (embed) por aviso**, até 10 por mensagem (o 10º resume o resto: "+N avisos a mais"):

| Parte | O que leva |
|---|---|
| Título | ícone + evento: 🔴 Conta caiu · 🟢 Voltou para o jogo · 🟠 Não conseguiu reconectar · 🟡 Janela travada · ⚠️ Falha ao abrir · ⚠️ Abriu em outra conta |
| Descrição | a frase do que aconteceu, com o nome em negrito ("**conta** saiu do jogo.") |
| Conta | o nome; à mostra, vira link do perfil (`roblox.com/users/<id>/profile`) |
| Jogo | o **nome** do jogo com link (`roblox.com/games/<place>`); sem nome conhecido, "Place 123" com o mesmo link |
| Motivo | traduzido ("O Roblox fechou sem sair do jogo", "Perdeu a conexão (código 277)"…) |
| Quando | `<t:…:R> · <t:…:T>` — o Discord mostra "há 2 minutos" e a hora local de quem lê |
| Foto | a foto da conta (thumbnail), só com o nome à mostra |
| Cor / rodapé | cor pela gravidade do evento; rodapé "MultiAlt" e a hora (`timestamp`) |

- **Idioma:** o do app (`General.Language`): português, espanhol ou inglês (o resto cai em
  inglês). O motivo é um **código** (`WebhookReason`) traduzido na hora de montar — o único texto
  livre é o do kick.
- **Nome do jogo e foto** (`webhook_enrich`): buscados na hora de enviar, nunca no aviso. Nome do
  jogo pelo `multiget-place-details` com o cookie de uma conta do aviso, **só leitura, sem renovar a
  sessão**, e guardado em cache; foto pelo `avatar-headshot` (sem cookie). Cada busca tem teto de
  4 s; falhou, a mensagem sai sem aquilo.
- **Nome da conta:** **à mostra por padrão** (`Webhook.MaskNames` = `false`: o canal é do próprio
  usuário). Com **Hide account names in messages**, sai a mesma máscara do "Names hidden" da tela
  (`General.HiddenNameLetters`) e **sem** foto e link do perfil (identificariam a conta). A opção
  era `HideNames` (ligada) até 15/10/2026, nunca publicada — trocou de nome para o padrão novo valer.
  Conta que não está mais no app vira "Unknown account".
- **Mensagem de kick:** com os nomes à mostra, vai o texto do Roblox ("Expulsa: …"); mascarado, só a
  categoria ("Expulsa do jogo (código 267)") — o texto pode citar o nome real de outro jogador.
- **Nunca vai:** cookie, senha, Job ID, link ou código de servidor privado.
- **Texto livre** (nome, mensagem de kick, nome do jogo) é escapado: não formata, não vira link e
  não menciona ninguém (`@` ganha um espaço invisível). Só o texto antes da mensagem (`Mention`)
  pode mencionar; sem ele, `allowed_mentions` vai vazio.

## Regras de negócio

- **A rede é do backend.** A tela só grava settings e chama `send_test_webhook`; nada no frontend
  fala com o Discord (travado em `the_frontend_never_posts_to_the_webhook_itself`).
- **Só Discord.** A URL colada é desmontada em (id, token) e o POST vai sempre para
  `https://discord.com/api/webhooks/<id>/<token>`, montado pelo app
  (`endpoints::external_host("discord")`). Host colado nunca vira destino. Aceitos na colagem:
  `discord.com`, `discordapp.com`, `ptb.`/`canary.` dos dois, `/api/webhooks/` ou `/api/vN/webhooks/`;
  recusados: `http`, porta, usuário/senha, query, fragmento, id que não é número, token estranho.
- **A URL é segredo.** Fica no `RAMSettings.ini` em texto puro (como as outras settings; a tela
  avisa), mas nunca em log: o erro do reqwest sai com `without_url()`, o token é trocado por `***`
  (`WebhookTarget::redact`) e o `Debug` do alvo esconde o token.
- Cliente HTTP de `http_client::builder()` (teto de 10 s de conexão e 30 s no total).

## Testes

`bun run t webhook` — Rust: `discord_webhook_url_tests`, `discord_webhook_message_tests`,
`discord_webhook_queue_tests`, `discord_webhook_http_tests` (POST no mock compartilhado, 429 com
`retry_after`, erro sem URL/token), `webhook_glue_tests`; front:
[NotificationsTab.test.tsx](../../src/components/settings/NotificationsTab.test.tsx),
[discordWebhook.test.ts](../../src/utils/discordWebhook.test.ts) (os mesmos casos de URL dos dois
lados). Defaults em `settings_store_tests`.

## Armadilhas / cuidados

- **Não verificado com um Discord de verdade** pelo agente (sem rede nos testes): o dono confere
  com o **Send test message** num canal dele.
- A fila vive em memória: avisos ainda não enviados se perdem ao fechar o app.
- Sem nome do jogo na mensagem: o backend não guarda o nome, só o Place ID que o log do cliente deu.
