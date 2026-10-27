# Avisos no Discord (webhook)

## Objetivo

Avisar num canal do Discord quando algo acontece com as contas sem a pessoa estar olhando o
app: a conta caiu (com o motivo), a reconexão desistiu, a janela travou, o launch falhou, a
conta reconectou. A pessoa cola a URL de um webhook **dela** e escolhe quais avisos quer. É a
ideia 5 de [ideias-de-outros-gerenciadores.md](../ideias-de-outros-gerenciadores.md).

**Desligado por padrão.** Ligado, as informações **saem do PC** — a tela diz isso no próprio
interruptor.

**Página Discord alerts** (27/10/2026, na barra lateral logo abaixo do AFK Mode; antes era a seção
Settings › Notifications, só com os avisos). Além dos avisos, a página tem: **menção por evento**,
**horário silencioso**, **status ao vivo** (uma mensagem editada a cada N minutos) e **resumo do
dia** (tempo de jogo por conta e por jogo). Tudo no mesmo webhook e atrás do mesmo interruptor.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Parte pura e envio: conferência da URL, filtro de eventos, mensagem, fila com lote e limite, POST | [api/discord_webhook.rs](../../src-tauri/src/api/discord_webhook.rs) |
| Cola: settings, nome da conta, pontos onde os eventos nascem, task que manda, "Send test message" | [commands/webhook.rs](../../src-tauri/src/commands/webhook.rs) |
| Pontos de origem | [client_health.rs](../../src-tauri/src/commands/client_health.rs) (`webhook_after_health_tick` no laço do monitor), [reconnect.rs](../../src-tauri/src/commands/reconnect.rs) (`publish_reconnect`), [launch.rs](../../src-tauri/src/commands/launch.rs) (falha no auth ticket e ao abrir o cliente, launch de uma conta e fila) |
| Segunda trava da URL ao gravar | `update_setting` em [data/settings/commands.rs](../../src-tauri/src/data/settings/commands.rs) → `validate_setting` |
| Status ao vivo e resumo: parte pura e envio (POST com `?wait=true`, PATCH na mensagem) | [api/discord_status.rs](../../src-tauri/src/api/discord_status.rs) |
| Status ao vivo e resumo: o laço de 30 s, a soma do tempo de jogo, `RAMDiscordStats.json`, "Update now"/"Send a preview" | [commands/discord_status.rs](../../src-tauri/src/commands/discord_status.rs) |
| Tela | Página **Discord alerts** ([DiscordPage.tsx](../../src/components/pages/DiscordPage.tsx) → [DiscordSettings.tsx](../../src/components/discord/DiscordSettings.tsx)); conferência da URL espelhada em [utils/discordWebhook.ts](../../src/utils/discordWebhook.ts) |
| Chaves | `[Webhook]` em [settings.md](settings.md#webhook--detalhes-em-webhookmd) |

## Fluxo

1. Página Discord alerts: liga **Send notifications to Discord**, cola a URL (campo de senha;
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
| An account was banned | `NotifyModerated` | ligado | `mark_account_moderated` ([launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs)): o launch recusado por moderação ou a consulta de moderação viu ban ativo. Só na **primeira vez** (quando a conta vai para o grupo das moderadas), não a cada launch recusado |
| A launch batch finished | `NotifyBatchFinished` | desligado | A fila de launch fechou ([launch.rs](../../src-tauri/src/commands/launch.rs), `webhook_after_launch_batch`): quantas abriram, falharam e foram canceladas. Só lote de **2 contas ou mais**; sem conta no cartão |
| A client was closed for memory | `NotifyMemoryClosed` | ligado | O teto de memória fechou o cliente ([memory_ceiling.rs](../../src-tauri/src/commands/memory_ceiling.rs), `MemoryNotice::Closed`) ou o Watcher fechou por memória abaixo do mínimo ([watcher.rs](../../src-tauri/src/commands/watcher.rs)) |
| Windows is almost out of memory | `NotifySystemLowMemory` | desligado | O laço do status ([discord_status.rs](../../src-tauri/src/commands/discord_status.rs)) lê a memória livre (`GlobalMemoryStatusEx`, só leitura): avisa abaixo de **10%** e só avisa de novo depois de voltar para mais de **15%** (`system_memory_step`) |
| A launch failed | `NotifyLaunchFailed` | desligado | Falha no auth ticket / ao abrir o cliente (launch de uma conta e fila). Vai **só a etapa**, nunca o texto do erro (pode ter URL de launch, ticket ou código de VIP) |

"Conta aberta como outra conta do Roblox" ficou de fora: o evento ainda não existe no backend.

## Menção por evento e horário silencioso

- **Menção por evento** (pedido do dono, 27/10/2026): cada evento tem uma caixa ao lado
  (`Mention<evento>`, ex.: `MentionModerated`). Vazia, vale o texto geral (`Mention`). Num lote com
  vários eventos, a mensagem leva cada menção **uma vez**, na ordem (`WebhookConfig::mention_for`).
  A menção continua sendo a única parte que pode chamar alguém.
- **Horário silencioso** (`QuietHours`, `QuietStart`/`QuietEnd` em `HH:MM`, hora do PC): nesse
  horário a mensagem sai **sem menção**. Pode atravessar a meia-noite (23:00 → 08:00). Início = fim
  ou hora inválida = desligado. Com **Don't send notifications during quiet hours** (`QuietSkip`),
  o aviso nem sai (o lote é descartado). O status ao vivo continua editando de qualquer jeito.

## Status ao vivo

- `StatusEnabled` (desligado) + `StatusInterval` (minutos, 1–60, padrão 5). Uma mensagem só no
  canal: a primeira vez é um POST com `?wait=true` (o Discord devolve o id, guardado em
  `StatusMessageId`); depois, **PATCH** nessa mensagem. Apagaram no Discord (404)? Cria outra.
- Mostra cada cliente aberto (o tracker do Windows, inclusive os abertos pelo site): 🟢 em jogo
  com o jogo e há quanto tempo ("1h 05m"), 🟡 fora de jogo; contagem "Em jogo"/"Abertas" e
  "Atualizado há…". O "desde quando" é lembrado pelo laço (o monitor de quedas só diz "em jogo").
- **Nunca menciona** ninguém (`allowed_mentions` vazio): é editado a cada poucos minutos.
- **Update now** manda/edita na hora (`send_discord_status_now`).
- Só Windows por enquanto (tracker e monitor de quedas); no macOS a lista sai vazia.

## Resumo do dia

- `SummaryEnabled` (desligado) + `SummaryTime` (`HH:MM`, padrão 23:00) + `MentionSummary`
  (opcional; vazio = sem menção, **não** usa o texto geral; nada no horário silencioso).
- A cada passada (30 s) o laço soma o tempo de cada conta **em jogo** e do jogo dela; quedas
  entram pela queda do monitor (`discord_stats_note_drop`). Uma passada que demorou mais de 2 min
  (PC suspenso) conta só 2 min. Só conta com o app aberto e o resumo ligado.
- A soma mora em `RAMDiscordStats.json` (pasta de dados), para não zerar se o app fechar no meio
  do dia. Não entra nos backups (é só contagem do dia).
- Sai **uma vez por dia**, na primeira passada depois da hora; com o PC desligado nessa hora, sai
  na próxima vez que passar dela (o cartão diz "Desde <data>"). Sem nada para contar, não manda:
  só marca o dia. Depois de mandar, zera.
- **Send a preview** manda o que somou até agora, sem zerar (`send_discord_summary_now`).

## Mensagem

Redesenhada em 15/10/2026 a pedido do dono ("o nome não apareceu, está feio — põe título, o que
aconteceu"). Antes era um embed com uma linha em inglês por aviso e o nome mascarado.

**Um cartão (embed) por aviso**, até 10 por mensagem (o 10º resume o resto: "+N avisos a mais"):

| Parte | O que leva |
|---|---|
| Título | ícone + evento: 🔴 Conta caiu · 🟢 Voltou para o jogo · 🟠 Não conseguiu reconectar · 🟡 Janela travada · ⚠️ Falha ao abrir · ⚠️ Abriu em outra conta · ⛔ Conta banida · 📋 Lote de launch terminou · 🧠 Cliente fechado por memória · 🧠 O Windows está quase sem memória. Os dois do app inteiro (lote, memória do Windows) não têm campo Conta nem foto: a frase é o motivo ("8 abertas, 1 falharam, 0 canceladas.") |
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

`bun run t webhook` — Rust: `discord_webhook_url_tests`, `discord_webhook_message_tests`
(também menção por evento, horário silencioso e os cartões novos), `discord_webhook_queue_tests`,
`discord_webhook_http_tests` (POST no mock compartilhado, 429 com `retry_after`, erro sem
URL/token), `discord_status_tests` (status, soma do tempo, hora do resumo, memória do Windows),
`discord_status_http_tests` (POST `?wait=true` devolvendo o id, PATCH, 404 = apagada),
`webhook_glue_tests`, e o `a_finished_batch_reports_how_many_opened_failed_and_were_cancelled`
(`launch_queue_tests`); front:
[DiscordSettings.test.tsx](../../src/components/discord/DiscordSettings.test.tsx),
[discordWebhook.test.ts](../../src/utils/discordWebhook.test.ts) (os mesmos casos de URL dos dois
lados). Defaults em `settings_store_tests`.

## Armadilhas / cuidados

- **Não verificado com um Discord de verdade** pelo agente (sem rede nos testes): o dono confere
  com o **Send test message** num canal dele.
- A fila vive em memória: avisos ainda não enviados se perdem ao fechar o app.
- Sem nome do jogo na mensagem: o backend não guarda o nome, só o Place ID que o log do cliente deu.
