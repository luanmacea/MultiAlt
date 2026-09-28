# Plano — trazer do upstream o que foi aprovado (passada de 2026-09-26)

Auditoria em `.claude/skills/upstream-sync/estado-da-auditoria.md`. Ponto de
fork: `ddcb4e48`. Upstream é **read-only** e o programa dele **nunca roda**.

## Global Constraints

Valem para **toda** tarefa. Vêm do `CLAUDE.md` do projeto e da auditoria.

1. **Regras de launch que não se quebram:**
   - `launch_url` (protocolo) abre **sempre a build de produção** — o `channel:`
     vazio da URL vence o registro. Versão por conta **não** se aplica a esse
     caminho.
   - `default_player_dir` / old join (sem URL): o cliente lê o canal do
     registro, então a pasta da versão instalada **é** o que manda.
   - **Nunca sobrescrever o canal do Roblox** (só o reparo em
     `set_player_channel`).
   - **Nada no launch pode fechar cliente de outra conta.** Isolamento pula em
     vez de matar; botting só fecha as próprias contas bot.
2. **Nunca** usar `run_with_session_retry` / `refresh_account_session` em
   leitura não crítica: o refresh chama
   `signoutfromallsessionsandreauthenticate` e derruba as sessões abertas.
3. **Teste que falha primeiro.** Teste Rust em `#[cfg(test)] mod <nome>_tests`
   **dentro** do arquivo, com nome único. Todo arquivo de teste novo entra numa
   suíte de `scripts/test-suites.ts` (a auditoria falha se ficar órfão).
   **Sabotar o próprio teste** e confirmar que ele reprova antes de dar a tarefa
   por pronta.
4. URL da API do Roblox sempre via `endpoints::host(...)` — literal
   `https://*.roblox.com` em `api/` quebra os testes mockados.
5. **`bun run check` verde** (typecheck + auditoria de suítes + vitest +
   `cargo test --all-features`) antes de commitar. Nada de commit com check
   vermelho.
6. **Um commit por tarefa**, mensagem em português, direto na `develop`.
   Sem `--force`, sem reescrever histórico, sem branch nova, sem PR.
   No corpo do commit, citar a origem: `origem: niccsprojects@<hash>`.
7. **Reimplementar, não `git apply`.** Os dois repositórios divergiram; o patch
   do upstream não encaixa.
8. Texto de UI passa por `t()` e por `bun run i18n:extract`. O `pt-BR` tem que
   cobrir o inglês inteiro, na mesma ordem (`src/i18n/locales.test.ts`); o `de`
   pode ficar incompleto. Chave que fica igual ao inglês em pt-BR entra em
   `IDENTICAL_BY_DESIGN`.
9. Piso de fonte da UI: nada abaixo de `text-[11px]`
   (`src/components/ui/textSize.test.ts`).
10. Comentário explica **por que**, não o que. Em português, como o resto do
    projeto.

---

## Task 1 — `ClientAppSettings.json` na pasta da versão que vai abrir

**Origem:** `niccsprojects@cf799dd`.

**O defeito, confirmado neste fork:**
`src-tauri/src/platform/windows/core.rs:280` — `get_client_settings_file()`
monta o caminho a partir de `get_roblox_path()`, que é a build padrão/produção.
`patch_client_settings_for_launch` (em `commands/launch_shared.rs`) chama
`windows::apply_runtime_client_settings(...)`, que grava por esse caminho. Quando
a conta abre numa versão do catálogo (`RobloxVersion` por conta, ou
`Versions.DefaultVersion`), o FPS, o volume, a qualidade, a tela cheia e os fast
flags são gravados **na pasta errada** e o cliente que abriu não os lê.

Isso ficou mais grave agora que existem exceções de launch por conta
(`docs/features/launch.md#exceções-de-launch-por-conta`).

**O que fazer:**
- Em `platform/windows/client_settings.rs`, deixar as escritas aceitarem a pasta
  base: `apply_runtime_client_settings` e `copy_custom_client_settings` ganham
  um `base_path: Option<&str>`; `None` continua significando "a build padrão"
  (é o que o servidor HTTP local usa, que não tem conta no contexto).
- Em `platform/windows/core.rs`, extrair `get_client_settings_file_in(base)` e
  fazer `get_client_settings_file()` chamá-la com `get_roblox_path()?`.
- `patch_client_settings_for_launch` ganha o `base_path` e o repassa.
- Call sites: `commands/launch.rs` passa `resolved_base_path` no launch de uma
  conta e `acct_base_path` dentro do laço da fila; `commands/botting.rs` passa a
  pasta que ele resolver (ver Task 3 — se a Task 3 ainda não rodou, passar
  `None` aqui e a Task 3 troca); `api/server/launch_patch.rs` continua `None`.
- macOS: a assinatura muda junto, e o `base_path` fica sem uso lá (`let _ =`),
  como o upstream fez.

**Testes:** `get_client_settings_file_in` é a parte pura — teste que, dada uma
pasta base, o caminho sai em `<base>\ClientSettings\ClientAppSettings.json`, e
que uma base diferente dá um caminho diferente. Não chamar
`get_client_settings_file()` em teste: ela resolve a pasta real do Roblox e
gravaria no disco do usuário (o módulo de teste de `client_settings.rs` já avisa
isso).

---

## Task 2 — achar o Roblox instalado por Bloxstrap, Fishstrap e Voidstrap

**Origem:** `niccsprojects@bfa8e1a`.

**O defeito, confirmado neste fork:**
`src-tauri/src/platform/windows/core.rs:256` só olha
`%LOCALAPPDATA%\Roblox\Versions`. Quem usa Bloxstrap, Fishstrap ou Voidstrap tem
o `RobloxPlayerBeta.exe` em `%LOCALAPPDATA%\<Launcher>\Versions`, e para o app
esse usuário simplesmente "não tem Roblox instalado".

**O que fazer:** varrer também `%LOCALAPPDATA%\{Bloxstrap,Fishstrap,Voidstrap}\Versions`,
mantendo `Roblox` como **primeira** opção (instalação oficial ganha).

**Testes:** extrair a lista de pastas candidatas para uma função pura que recebe
o `LOCALAPPDATA` e devolve os caminhos na ordem, e testar a ordem e o conteúdo.
A função que toca disco continua sem teste.

---

## Task 3 — Botting abre a conta na versão configurada dela (só no old join)

**Origem:** `niccsprojects@8bcbdfc`, **adaptado**.

**O defeito, confirmado neste fork:** `commands/botting.rs` não tem nenhuma
referência a `RobloxVersion` nem a `resolve_roblox_install_path`. A conta que
tem versão própria abre no Botting com a build padrão.

**A adaptação (Global Constraint 1):** no caminho `launch_url` a build de
produção é regra do projeto — ali **não** se aplica versão por conta. A correção
vale para o caminho `launch_old_join`, que é o que usa a pasta da versão
instalada. Se a conta tem versão própria e o Botting vai pelo protocolo, não
mudar o comportamento.

**O que fazer:**
- No `launch_account_for_cycle`, resolver a versão da conta do mesmo jeito que
  `commands/launch.rs` faz (campo `RobloxVersion` + `resolve_roblox_install_path`).
- Usar a pasta resolvida no `base_path` do `patch_client_settings_for_launch`
  (Task 1) e no old join.
- Não inventar caminho novo de launch: reaproveitar os helpers que
  `commands/launch.rs` já usa.

**Testes:** testar a decisão, não o launch — uma função pura que, dado
(tem versão da conta, vai por old join), diz qual pasta usar. Incluir o caso
"tem versão mas vai por URL → produção", que é a regra do projeto.

---

## Task 4 — duas sequências de launch não podem rodar juntas

**Origem:** `niccsprojects@2f82c9b`, `946997a`, `1c9e450`, `f2ee810`.

**O defeito, confirmado neste fork:** `commands/launch.rs:319`
`launch_queue_start` chama `queue.start(...)`, que **substitui** a fila. Se o
usuário disparar um segundo launch enquanto o primeiro anda (dois cliques, ou
um launch de uma conta durante uma fila), os dois lotes rodam ao mesmo tempo
disputando o mutex do Multi Roblox, o registro e o patch de client settings — e
a UI mostra só o segundo, então o primeiro fica invisível.

**O que fazer:**
- Reservar a sequência: `launch_queue_start` passa a **recusar** quando já há
  uma sequência ativa, e quem chama devolve erro em vez de seguir.
  "Ativa" = existe conta na fila que não está num estado final.
- A reserva cobre as contas do lote inteiro **antes** de qualquer `invoke` —
  reservar por conta, uma a uma, deixaria a segunda sequência entrar no meio.
- Mensagem de erro clara e traduzida ("Já existe um launch em andamento").
- Cancelar a fila (o caminho que já existe) tem que liberar a reserva, senão o
  app trava até reiniciar. Falha no meio do lote também libera.
- Não mexer no Botting: o ciclo dele não é uma "sequência de launch" da UI.

**Testes:** a fila já é testável sem `AppHandle` (`LaunchQueue`,
`launch_queue_helper` / os testes de `launch_queue_tests`). Testar: começar com
a fila livre funciona; começar com uma sequência ativa é recusado; depois de
todas as contas chegarem a estado final, uma nova sequência é aceita; cancelar
libera. Incluir o caso da corrida: `start` chamado duas vezes sem nada entre as
duas.

---

## Task 5 — presença com cookie de quem está olhando

**Origem:** `niccsprojects@c82a438`, `ab724d2`.

**O defeito, confirmado neste fork:** o comando `get_presence`
(`commands/account_api.rs:1259`) chama `api::roblox::get_presence(&user_ids)`,
que é `get_presence_as(None, ...)` — **sem cookie**. É o que alimenta as
bolinhas In Game / Online / Offline da lista (`src/store.tsx:2316`). Sem cookie
a API do Roblox devolve a versão degradada.

**O que fazer:**
- O comando aceita um `viewer_user_id: Option<i64>` e, quando não vier,
  escolhe o cookie de uma conta válida do store (preferindo `valid == true` e
  `security_token` não vazio).
- **Fallback obrigatório:** se a chamada com cookie falhar, repetir sem cookie.
  Sem isso, uma conta com cookie morto apaga a presença de todas.
- **Não** usar `run_with_session_retry` aqui (Global Constraint 2): é leitura.
- Ressalva a registrar em comentário: usar o cookie de uma conta como "viewer"
  faz a visibilidade das outras depender da privacidade **daquela** conta.

**Testes:** com o mock de `endpoints::host` que os testes de
`social_presence.rs` já usam: manda o cookie quando há conta válida; cai para
sem-cookie quando a chamada autenticada falha; não quebra quando o store está
vazio. A escolha da conta viewer é uma função pura — testar a preferência por
conta válida.

---

## Task 6 — recuperação de tela branca/preta do WebView2

**Origem:** `niccsprojects@a5c0577`, `f058f6e`, `1d6b813`.

**O problema:** o app é WebView2. Quando o WebView2 falha (driver de GPU,
runtime atualizado no meio, composição), a janela abre **em branco** e não há
nada que o usuário possa fazer — o app fica inutilizável e ele não tem como nem
chegar nas configurações.

**O que fazer** (novo `src-tauri/src/webview_recovery.rs`, chamado do `lib.rs`
antes de criar a janela):
- `prepare_environment()` monta `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
  **mesclando** o que já estiver na variável em vez de sobrescrever.
- Safe mode (`--disable-gpu --disable-gpu-compositing`) ligado por: argumento
  `--safe-mode`, variável `RAM_WEBVIEW_SAFE_MODE`, Shift segurado no boot, ou
  marcador em disco.
- O marcador guarda a **versão do runtime do WebView2**: se o runtime mudou, o
  marcador é apagado (senão o safe mode fica ligado para sempre).
- Sinal de "frontend pintou" com prazo (~25 s): sem o sinal, grava o marcador e
  avisa o usuário que a próxima abertura vai ser em safe mode.
- Error boundary no React (`src/components/layout/AppErrorBoundary.tsx`) para
  erro de render não virar tela branca silenciosa.
- Windows-only atrás de `#[cfg(target_os = "windows")]`; o resto compila sem.

**Não trazer junto:** o diálogo nativo do upstream depende de
`tauri-plugin-dialog`. Se der para avisar com o que o projeto já tem, melhor —
dependência nova só se não houver caminho sem ela, e nesse caso dizer por quê.

**Testes:** o testável é puro — decidir safe mode a partir de
(argumento, variável, shift, marcador, versão do runtime); e a mesclagem dos
argumentos do browser (não perder o que já estava lá, não duplicar flag).
`GetAsyncKeyState` e disco ficam fora do teste.

---

## Task 7 — renomear Botting para Auto Rejoin

**Origem:** `niccsprojects@e5fcd9c`, `4f360ef`. Pedido do dono: o nome novo é
mais intuitivo.

**Escopo: só o que o usuário lê.** Rótulo, título, texto de ajuda, toast, linha
de console. Papéis: "player" vira **main**, "bot" vira **alt**.

**O que NÃO muda** (mudar quebra dados ou testes de quem já usa):
- Chaves do `RAMSettings.ini` (`Botting*`, `BottingPlayer*`, `BottingBot*`) —
  renomear apaga a configuração de quem já tem.
- Nome de comando Tauri, nome de evento, nome de campo de JSON.
- Nome de arquivo, de módulo, de função e de teste no Rust/TS.
- A entrada `"Botting"` em `IDENTICAL_BY_DESIGN`: se a chave sumir do inglês, o
  teste de locales reprova por chave morta — tirar dali junto.

**O que fazer:** trocar as strings de UI, rodar `bun run i18n:extract`, traduzir
as chaves novas em pt-BR (o de pode ficar), remover do catálogo as chaves que
ficaram sem uso, e atualizar `docs/features/botting.md` e o que mais citar o
nome antigo na doc.

**Testes:** os testes que procuram o texto antigo na tela vão reprovar — é o
sinal de que o rename pegou. Atualizá-los. Não deixar nenhum `Botting` visível
na UI.

---

## Task 8 — `AccountData.json` encriptado por padrão, com chave presa ao aparelho

**Origem:** `niccsprojects@b2893e8`, `1d76b9c`, `b262df9`, `9a88224`, `1b3f588`.
**A tarefa mais perigosa do plano: ela pode custar o arquivo de contas do dono.**

**O problema:** sem senha, o `AccountData.json` é JSON puro com o
`.ROBLOSECURITY` de todas as contas. Qualquer programa que leia o arquivo entra
em todas elas, sem senha e sem 2FA.

**O ganho é real mas limitado, e isso tem que estar escrito no código:** a chave
fica num arquivo ao lado e o DPAPI é do usuário do Windows. Protege contra
arquivo copiado, backup vazado e outro usuário no mesmo PC. **Não** protege
contra malware rodando como o próprio usuário.

> **Correção (27/09/2026) — esta frase do brief estava errada, não a siga.**
> Backup vazado **não** é protegido: o zip tem que levar o `AccountData.key`
> (senão não restaura), então quem tem o zip tem a chave e só sobra o embrulho
> do aparelho, que é fraco. E "arquivo copiado" só vale **sem** o `.key` do
> lado. O que vale é o que está em
> [accounts.md](../../features/accounts.md#o-que-essa-proteção-vale-e-o-que-não-vale)
> e no comentário de `data/vault_key.rs`.

**O que fazer:**
- `data/vault_key.rs`: chave mestra aleatória de 32 bytes, guardada num arquivo
  `.key` ao lado do vault, embrulhada **de duas formas** — DPAPI
  (`CryptProtectData`, com entropia própria) e um hash derivado do aparelho, com
  os fallbacks de identificador que o upstream usa. Qualquer um dos dois abre.
- O vault passa a ser encriptado com a chave mestra quando **não** há senha do
  usuário. Com senha, o comportamento atual continua (a senha manda, e o
  arquivo `.key` é removido).
- **Migração nos dois sentidos, e é aqui que se ganha ou se perde:**
  - `AccountData.json` em JSON puro é lido e reescrito encriptado, **com backup
    `.json.bak` antes de qualquer escrita**;
  - se a chave não puder ser recuperada, **não apagar nada e não travar o app**:
    reportar erro dizendo onde está o backup. Perder o arquivo é pior que ficar
    sem a criptografia.
  - importar backup encriptado por padrão (sem senha) tem que funcionar.
- Windows-only para o DPAPI; o caminho do hash do aparelho é o comum.

**Testes:** o formato é testável em arquivo temporário, sem tocar o
`AccountData.json` real (é o que `data/accounts/store.rs` já faz nos testes
dele). Cobrir: vault puro migra para encriptado e o conteúdo bate; `.json.bak`
existe depois da migração; chave irrecuperável **não** apaga o vault e devolve
erro; definir senha remove o `.key` e passa a exigir senha; tirar a senha volta
para a chave do aparelho; round-trip da chave mestra pelos dois embrulhos.

**Antes de commitar esta tarefa:** copiar o `AccountData.json` real do dono para
fora do projeto, como rede de segurança manual, e dizer no relatório onde ficou.

---

# Segunda leva — aprovada pelo dono em 2026-09-27

Ele confirmou que a grade de janelas já existe aqui e pediu os outros cinco itens
"não urgentes", mais o AFK mode (que ele quer **como envio periódico de teclas**,
não como detecção de interação — ver Task 14).

As Global Constraints do começo deste arquivo continuam valendo inteiras.

---

## Task 9 — intervalo de rejoin até 480 minutos

**Origem:** `niccsprojects@6e6c9dc`.

Hoje `clamp_botting_interval_minutes` (`src-tauri/src/commands/botting.rs:49`)
trava em `10..=120`, e o diálogo mostra esse limite. Subir o teto para **480**
(8 horas) no backend e no que a tela mostra e valida. O piso continua 10.

**Testes:** o `clamp` já tem teste (`clamp_botting_interval_minutes_keeps_10_to_120`)
— ele vai reprovar, é o sinal. Atualizar nome e casos: 480 passa, 481 vira 480,
9 vira 10. Conferir se há teste de UI afirmando o limite antigo.

---

## Task 10 — alias até 240 caracteres, com opção de quebrar nomes longos

**Origem:** `niccsprojects@c276b30`.

Hoje o alias é cortado em 30 (`src/components/accounts/SingleSelectSidebar.tsx:53`
e `:144`, `src/components/menus/ContextMenu.tsx:211`). Subir para **240** nos
três lugares e em qualquer outro que corte alias.

Um alias de 240 caracteres estoura a linha da conta, então vem junto uma opção
de settings **"quebrar nomes longos"** (chave nova em `General`, default
`false`): ligada, a linha da conta deixa o nome quebrar em mais de uma linha em
vez de truncar.

**Cuidado:** `chipMaskName` (mesmo arquivo) mascara o nome quando "esconder
usernames" está ligado — conferir que ele continua correto com nome longo.
O default da chave nova entra no espelho de defaults de
`src-tauri/src/data/settings/store.rs` (há teste que cobra isso:
`every_documented_default_is_applied_on_a_fresh_install`); atenção que default
vazio **não** é gravado no INI, então siga a regra dos outros booleanos.

**Testes:** alias de 240 é aceito e um de 241 é cortado; com a opção ligada a
linha não truncou e desligada truncou; o mascaramento de nome longo continua
certo.

---

## Task 11 — importar `username:password:cookie`

**Origem:** `niccsprojects@416a4de`, `32975e0`, `c7304a4`, `843f498`, `ff5e0ca`.

Hoje o `ImportDialog` aceita **um `.ROBLOSECURITY` por linha**
(`src/components/dialogs/ImportDialog.tsx:274`). Passar a aceitar também
`username:password:cookie` na mesma caixa, detectando o formato por linha.

O upstream extraiu um **padrão de cookie compartilhado** (criou
`src/utils/cookies.ts`) para não ter três regex diferentes de `.ROBLOSECURITY`
espalhadas — faça o mesmo aqui e use o mesmo padrão no arrastar-e-soltar de
arquivo, se este fork tiver isso.

Regras que o upstream corrigiu depois e que valem:
- linha com credencial **incompleta** é pulada, não importada pela metade
  (`ff5e0ca`);
- ao separar, tirar **só** o delimitador do cookie — o cookie tem `:` dentro
  dele, então um `split(':')` ingênuo corta o cookie no meio (`843f498`). Esse é
  o detalhe que quebra tudo se for feito errado.

**Cuidado com o aviso existente:** o diálogo hoje avisa que o cookie é a sessão
inteira da conta. Aceitar senha junto **aumenta** o que está em jogo — o texto
precisa refletir isso.

**Testes:** parser puro, com casos: só cookie; `user:pass:cookie` com `:` dentro
do cookie; linha só com `user:pass` (incompleta, pulada); linha vazia; espaço nas
pontas; cookie com o prefixo `_|WARNING:-DO-NOT-SHARE...`. O teste do `:` dentro
do cookie é obrigatório.

---

## Task 12 — servidores recentes (job ids), não só jogos recentes

**Origem:** `niccsprojects@b3e3eb4`, arquivos `RecentJobsList.tsx` e
`RecentJobsPopover.tsx` no upstream.

Este fork já guarda **jogos** recentes (`RecentGamesList.tsx`,
`RecentGamesPopover.tsx`, `RecentTab.tsx`, persistido em
`src-tauri/src/data/settings/`). Falta guardar o **servidor** (job id) em que as
contas entraram, para poder voltar exatamente para aquele servidor.

**O que fazer:** guardar, por jogo, os últimos job ids usados, respeitando um
limite configurável (o de jogos já tem um — seguir o mesmo desenho e reaproveitar
o que der). Mostrar numa lista ao lado da de jogos recentes; clicar preenche o
Job ID do launch.

**Cuidado:** job id de servidor **VIP/privado** neste fork usa o prefixo `vip:`
ou URL a decodificar (ver `CLAUDE.md` e `docs/features/join-links.md`). Guardar e
reexibir sem quebrar o significado — e **não** deixar link privado aparecer para
outra conta sem o dono querer. Se ficar ambíguo, guardar só job id público e
dizer no relatório por quê.

**Testes:** a parte pura é a lista — adicionar um job já presente sobe ele para o
topo sem duplicar; passar do limite derruba o mais antigo; job vazio não entra;
round-trip pela persistência.

---

## Task 13 — Chromium: instalação manual e uso do navegador do sistema

**Origem:** `niccsprojects@395fd4d`, `70c30c5`, `f2a0aca`, `687fcac`, `785cedc`,
`2a111df`.

Este fork já tem progresso de download (`chromium-download-progress` em
`src-tauri/src/chromium/download.rs`). Falta:
- **fallback para o navegador do sistema** quando o download falha ou o usuário
  não quer baixar nada: usar um Chrome/Edge já instalado. O upstream exige que o
  candidato seja **arquivo regular** (`785cedc`) — não seguir link nem pasta;
- **apontar um binário à mão**, para quem tem o navegador em lugar não padrão;
- robustez do download que ele corrigiu depois: repetir a consulta de versão em
  resposta não-2xx (`f2a0aca`, `687fcac`), limitar a requisição de versão, e
  **finalizar a extração de forma atômica** (`2a111df`) — extrair para pasta
  temporária e só então renomear, senão uma extração interrompida deixa uma
  instalação meio pronta que o app acha que está boa;
- reinstalar **substitui** a instalação em vez de empilhar (`70c30c5`).

**Cuidado:** o Chromium é usado para login e criação de contas, ou seja passa
perto de credencial. Não logar caminho junto de cookie ou senha, e não aceitar
caminho de binário que não venha do usuário escolhendo.

**Testes:** escolha do candidato do sistema (arquivo regular sim, pasta não, link
não, inexistente não, ordem de preferência); a decisão de repetir por status
HTTP; e a extração atômica (a pasta temporária vira final só no sucesso, e uma
extração interrompida não deixa instalação "válida" pela metade).

---

## Task 14 — AFK mode: mandar teclas de tempo em tempo para as janelas do Roblox

**Origem:** `niccsprojects@b3e3eb4` e os follow-ups `40c3c18`, `f683b7a`,
`1add22f`, `07ce381`. O `7bba2e8` (expor ao script API) fica **fora**.

**O que o dono quer, nas palavras dele:** "ao invés de detectar interação eu
tenha a opção de mandar teclas de forma automática de tempo em tempo nas telas,
assim consigo usar meu pc normalmente que às vezes o próprio app interage com a
janela do Roblox e nem precisa dar rejoin". O objetivo é **não perder o estado**:
não sair do lugar do mapa, não precisar de rejoin.

**O que é, tecnicamente:** envio de entrada sintética (`SendInput`) para a janela
de um cliente Roblox, num intervalo configurável. **Não** é detecção de
interação — isso não é viável (`GetLastInputInfo` é da sessão inteira do Windows,
não sabe de qual janela veio a entrada) e não faz parte desta tarefa.

**Limites que precisam estar no código e na tela:**
- `SendInput` vai para a janela **em primeiro plano**, então o app precisa trazer
  a janela do Roblox para frente por um instante e **devolver o foco** para onde
  estava. Isso rouba o foco por um piscar a cada ciclo — a tela tem que dizer
  isso com essas palavras, sem enfeitar.
- **Só envia, nunca lê teclado:** proibido `SetWindowsHookEx`, `GetAsyncKeyState`
  para ler tecla do usuário, ou qualquer leitura de entrada. Lista fechada de
  teclas permitidas, no espírito do `AFK_KEYS` do upstream (espaço, WASD, E, F,
  R, Q, 1–5); o usuário escolhe de dentro dessa lista, não digita tecla
  arbitrária.
- Nada aqui pode fechar, minimizar ou mexer em cliente de conta que não está no
  AFK mode (Global Constraint 1).
- Parar o AFK mode interrompe o ciclo **na hora**, inclusive um ciclo já em
  andamento (`1add22f`), e não restaura foco quando está parando (`07ce381`).
  Sessão parada à força limpa o estado (`f683b7a`).
- Sem tecla escolhida, o AFK mode não liga — não inventar tecla padrão que mexa
  no personagem sem o usuário ter pedido.

**Escopo:** módulo novo no backend (envio e agendamento por conta), a UI para
ligar e desligar por conta com intervalo e tecla, e as chaves de settings. Fica
**fora**: expor ao script API, e qualquer detecção de interação.

**Testes:** o testável é puro — o mapa tecla para (virtual key, scan code) recusa
tecla fora da lista; o agendador decide "está na hora desta conta?" a partir de
(último envio, intervalo, agora); parar marca a sessão como parando e o ciclo
seguinte não envia nem restaura foco; conta fora do AFK mode nunca entra na
lista de alvos. `SendInput`, foco e janela ficam fora do teste.
