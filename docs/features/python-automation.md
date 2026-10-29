# Automação do Roblox Player em Python

> **Branch local e secreta** (`feature/python-scripts`): não citar em
> commit/PR/notas públicas até o dono liberar (21/10/2026). Desenho em
> [specs/2026-10-21-python-automation-design.md](../superpowers/specs/2026-10-21-python-automation-design.md),
> plano em [plans/2026-10-21-python-automation.md](../superpowers/plans/2026-10-21-python-automation.md).

## Objetivo

Rodar **scripts Python do usuário** contra a janela do Roblox de cada conta:
trazer a janela, mandar tecla e clique, ler pixel, achar imagem, repetir. O
mesmo espírito das [Gravações](recordings.md), com código de verdade. Fica na
página **Scripts**, no modo "Roblox Player automation (Python)" (o modo
"MultiAlt automation (JavaScript)" é o de sempre). Textos da parte Python são
só em inglês, sem `t()`.

Não é injeção no cliente, executor de Lua nem leitura de memória do jogo.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Comandos da tela (`python_*`, sempre registrados; stubs sem a feature) | [python/commands.rs](../../src-tauri/src/python/commands.rs) |
| Tipos do contrato (espelhados em `src/components/python/pythonApi.ts`) | [python/types.rs](../../src-tauri/src/python/types.rs) |
| Códigos, mensagens e dicas dos erros | [python/errors.rs](../../src-tauri/src/python/errors.rs) |
| Python portátil (download, hash, instalação) | [python/runtime.rs](../../src-tauri/src/python/runtime.rs) |
| Pasta de trabalho, blocos padrão, scripts, `runs.json`, exemplo semeado | [python/workspace.rs](../../src-tauri/src/python/workspace.rs) |
| Explorer da aba Code (árvore, ler/gravar/criar/renomear/apagar, `.trash/`) | [python/explorer.rs](../../src-tauri/src/python/explorer.rs) |
| Aba Global functions (`def` de topo dos blocos e de `lib/`, lidos como texto) | [python/globals.rs](../../src-tauri/src/python/globals.rs) |
| Fila de foco (lease sobre o `AFK_CYCLE_LOCK`) | [python/focus.rs](../../src-tauri/src/python/focus.rs) |
| Primitivas e travas | [python/primitives.rs](../../src-tauri/src/python/primitives.rs) |
| Protocolo JSON por linha | [python/bridge.rs](../../src-tauri/src/python/bridge.rs) |
| Execuções, gatilhos, processo Python, log | [python/runs.rs](../../src-tauri/src/python/runs.rs) |
| Dashboard | [python/dashboard.rs](../../src-tauri/src/python/dashboard.rs) |
| Inspetor (captura em PNG, recortes em `assets/`) | [python/inspector.rs](../../src-tauri/src/python/inspector.rs) |
| Estado vivo no app e eventos | [python/service.rs](../../src-tauri/src/python/service.rs) |
| Modo desenvolvedor: named pipe do terminal | [python/devpipe.rs](../../src-tauri/src/python/devpipe.rs) |
| Captura de janela (`PrintWindow`) e memória compartilhada | [platform/windows/capture.rs](../../src-tauri/src/platform/windows/capture.rs) |
| SDK Python (embutido com `include_str!`) | [src-tauri/python-sdk/](../../src-tauri/python-sdk/) |
| Tela | `src/components/python/` |

Features do Cargo: `python-scripts` (tudo) e `python-dev-pipe` (o canal do
terminal), as duas dentro do `standard` (logo também no `full`). Frontend:
`ENABLE_PYTHON_SCRIPTS` (`VITE_ENABLE_PYTHON_SCRIPTS`, `"true"` nos dois passos
do [release-v4.yml](../../.github/workflows/release-v4.yml)). Se o scan da
edição padrão marcar, a feature culpada sai do `standard` (regra do CLAUDE.md).

## Python portátil

Nada baixa sem o clique em **Install Python**. Versões e SHA-256 fixos em
`runtime.rs` (conferidos em 21/10/2026):

| Peça | Versão | Origem do hash |
|---|---|---|
| CPython *embeddable* amd64 | 3.12.10 (a última 3.12 com binários) | bundle sigstore do python.org (`.zip.sigstore`, `messageDigest`): `4acbed6d…a3c3` |
| pip (wheel) | 25.1.1 | PyPI: `2913a38a…51af` |
| numpy (cp312 win_amd64) | 2.2.6 | PyPI: `c1f9540b…40b2` |
| opencv-python-headless (abi3 win_amd64) | 4.12.0.88 | PyPI: `86b413bd…5528` |

O numpy fica em 2.2.6 porque o opencv 4.12.0.88 pede `numpy>=2,<2.3`. Total
≈ 64 MB.

Passos: baixa o zip → confere o hash → extrai em `python-runtime/.tmp`
(recusando entrada que sai da pasta) → ajusta o `python312._pth` (liga
`import site`, põe `Lib\site-packages` e a **pasta de trabalho**) → abre o wheel
do pip no `Lib\site-packages` → `python -m pip install --no-index --find-links
<downloads> --require-hashes --only-binary=:all: --ignore-installed -r
requirements.txt` → grava `version.json` e põe o novo no lugar de
`python-runtime/current`. Erro em qualquer passo apaga o `.tmp` e deixa o runtime de antes.
**Reinstall** é a mesma instalação: o `current` só é trocado no fim; os scripts
moram na pasta de trabalho e nunca são tocados. Uma segunda instalação com outra
em andamento é recusada. O runtime mora em `<app_local_data_dir>/python-runtime/`
— fora do backup.

Instalar/reinstalar com execução viva é recusado ("Stop the running Python
scripts first...", `install_allowed`), e o Run é recusado durante a
instalação. A troca é atômica o quanto o disco deixa: `current` vira
`current.old`, o novo entra, o `.old` é apagado; se o novo não entra, o antigo
volta (`the_new_runtime_replaces_the_old_one_or_the_old_one_stays`).

**Todo `python.exe` que o app abre** leva `-s -E -B -X utf8`
(`ISOLATION_FLAGS`), `PYTHONNOUSERSITE=1` e `CREATE_NO_WINDOW`. O `-s` não é
enfeite: na primeira instalação real (21/10/2026) o pip do runtime enxergou o
*site-packages do usuário* (`%APPDATA%\Python\Python312`, do Python que o dono
tem instalado) e **desinstalou o numpy de lá** para pôr o fixado. O numpy do
dono foi reinstalado na hora (`pip install --user numpy==2.4.6`) e a instalação
passou a ser isolada e com `--ignore-installed`.

## Pasta de trabalho

`<pasta de dados>/python/` (`get_python_workspace_dir`, em `DATA_DIRS`: entra no
backup e na migração de pasta):

```
scripts/        scripts do usuário (^[A-Za-z0-9_-]{1,64}\.py$, sem caminho)
lib/            código compartilhado do usuário
assets/         imagens do find_image (o inspetor salva aqui)
multialt/       SDK; _core/ e __init__ são do app (reescritos a cada sincronização)
  defaults/     BLOCOS PADRÃO: runner, focus, vision, actions, dashboard
  defaults/.originals.json   hash do original de onde cada bloco partiu
  _core/pipe.txt             nome do pipe, só com o Modo desenvolvedor ligado
examples/ templates/ AGENTS.md CLAUDE.md docs/API.md   do app
runs.json       configuração salva por script (contas, modo, gatilho)
dashboard.json  últimos valores do dashboard
.seeded-examples  marcador: o exemplo já foi criado (não recria)
.trash/         o que o Explorer "apagou" (<ms>-<nome>)
```

**Exemplo da primeira abertura** (`SEED_FILES`, embutidos de
`src-tauri/python-sdk/seed/`): sem o marcador `.seeded-examples`, a
sincronização cria `scripts/example_walk_jump.py`, `lib/__init__.py` e
`lib/helpers.py` — só os que ainda não existem, nunca sobrescreve — e grava o
marcador. Com o marcador, nada: apagado, o exemplo não volta (apagar o
marcador traz de volta só o que falta). O exemplo anda (W, por
`lib.helpers.walk_forward`), pula (Space), abre e fecha o menu (Escape),
importa um bloco (`multialt.defaults.actions.jitter_sleep`) e alimenta o
`farm_report()` (`REPORT = True` + `report_loop`). O `lib/__init__.py` deixa
`lib` como pacote normal (sem ele o `from lib.helpers import ...` também
funciona, como namespace package — os dois casos testados em
`tests/test_seed.py`, num Python limpo com a pasta de trabalho no `sys.path`
como o host faz).

`sync_workspace` roda ao abrir a aba (`python_workspace_info`) e antes de cada
execução. Bloco **não editado** (hash do disco = original registrado) acompanha o
app; bloco **editado** fica, com **Modified**, e ganha **New default available**
quando o embutido mudou. **Restore default** é por bloco e só aceita nome de
bloco. Os hashes são do texto com `\r\n` normalizado (o checkout CRLF não conta
como edição). Editar bloco exige `Python.DeveloperMode = true`.

"Open in editor" usa o verbo "open" do Windows **só** quando ele não roda o
script: se o `.py` estiver associado a um Python (`python.exe`, `py.exe`,
`pythonw.exe`), abre no Bloco de Notas — "abrir" ali seria executar.

O editor da tela avisa a vista (`onDirtyChange`) quando tem edição não salva:
trocar de script ou sair da aba Scripts pergunta antes ("<script> has unsaved
changes. Discard them and open ...?"); cancelar deixa tudo como estava. Script
apagado não pergunta (não há o que guardar).

## Protocolo

JSON por linha, UTF-8, pelo stdin/stdout do processo filho
(`"<python>" -s -E -B -X utf8 -u -m multialt._core.host scripts/<nome>`, cwd =
pasta de trabalho) ou pelo pipe do Modo desenvolvedor — o mesmo `bridge::serve`.
O app manda o `hello` primeiro e, ao Parar, `{"event":"stop"}` (antes de
qualquer outra resposta). Pedido `{"id","op","args"}` → `{"id","ok":true,
"result"}` ou `{"id","ok":false,"error":{"code","message","hint"}}`; `"id": null`
(log, status) não tem resposta. Cada pedido roda numa task: no modo "todas
juntas" várias threads esperam ao mesmo tempo. Linha que não é JSON vira aviso
no log, sem derrubar a execução. O stderr do Python vai para o log como `error`;
a última linha dele vira o erro da execução que falhou. Antes de qualquer
código do usuário, o host (`protocol.stdio_transport`) duplica o stdin e o
stdout do protocolo (cópias não herdáveis) e aponta o descritor 0 para o
`NUL` e o 1 para o stderr: um programa que o script abre (`subprocess.run`)
herda stdin vazio, não o pipe do app — antes ele travava lendo o protocolo ou
comia as respostas (`test_a_child_process_of_the_script_does_not_get_the_protocol_stdin`).

Captura: `screen.capture` publica os bytes BGRA numa memória compartilhada
nomeada `Local\multialt-cap-<pid>-<run>-<n>` (o Python abre com
`mmap.mmap(-1, n, tagname=...)`). Janela "Não respondendo" (`IsHungAppWindow`)
não é capturada — o `PrintWindow` espera a janela se desenhar e travaria junto:
`capture_failed` "window is not responding" (`a_hung_window_is_not_captured`;
vale para o inspetor também). As 4 últimas capturas de cada execução ficam vivas
(contrato dizia "até a próxima": no modo "todas juntas" duas threads capturam
ao mesmo tempo). `screen.pixel` é um ponto dessa captura.

Pânico do app ao atender um pedido (`dispatch` dentro de `catch_unwind`, no
`bridge::dispatch_guarded`) vira resposta `internal` para aquele id ("MultiAlt
hit an internal error while handling <op>.") e uma linha `error` no log; antes
a task morria calada e o Python esperava a resposta para sempre
(`a_panic_answers_internal_instead_of_hanging`). O SDK entrega como
`MultiAltError` com `code == "internal"` (sem classe própria).

## Mensagens no Discord (`multialt.discord`, bloco `notify`)

Pedido do dono (28/10/2026): mandar mensagem pelo webhook a partir dos scripts.
O script pede (`discord.send` com `text`, `title` e `user_id` opcionais) e o
**app** manda pelo webhook da página Discord alerts: a URL nunca chega ao
Python. Entra na mesma fila dos avisos (lote, limite do Discord, horário
silencioso, a menção do evento "A message from a Python script"); `@` no texto
nunca menciona. Conta de fora da execução é recusada como nas outras ops; com o
envio desligado, `MultiAltError` com `code == "webhook_off"`.

O bloco padrão `multialt/defaults/notify.py` (editável no Modo desenvolvedor):
`send`, `send_report({nome: valor})`, `send_at_most_every(minutos, texto,
key=...)` (não inunda o canal num laço) e `with notify_errors(...)` (manda o
erro e o relança; o Parar não conta). Testes em `python-sdk/tests/test_notify.py`
e `a_discord_message_goes_to_the_app_checked`.

## Travas (valem com qualquer bloco reescrito)

- Só as contas da execução; conta de fora ou sem cliente aberto pelo MultiAlt
  (tracker) → `untracked_window`.
- Entrada (`input.*`, `window.bring_forward`), nesta ordem: lease da fila de foco
  desta execução **para esta conta** (`no_focus_lease`) → janela rastreada →
  não minimizada (`minimized`) → **na frente** (`not_foreground`).
- `forbidden` ("Not available through MultiAlt's API: cookies and passwords
  are never sent to scripts." — a frase diz *pela API*: o Python em si tem
  acesso total ao PC, e as travas valem para o que passa pelo app, não são
  sandbox) para qualquer op fora da lista ou com `cookie`, `password`,
  `token` ou `secret` no nome. As contas que o Python vê são
  `{user_id, name, alias, group, in_game, place_id, job_id}` — nunca segredo.
  `in_game`, `place_id` e `job_id` são lidos **na hora** a cada `accounts`
  (`Desk::session`): cliente no tracker e o último "Joining game" que o monitor
  de saúde já leu do log do Roblox (`client_health_of`), sem ler nada novo.
  Sem cliente, `false/0/""`; sem o log ainda, o cliente aberto conta como no
  jogo e place/job ficam `0/""` (`accounts_are_read_live`). O `hello` leva o
  mesmo retrato do começo.
- Teclas só da lista das Gravações (`RECORDING_KEYS`) **mais o `Escape`**
  (`PYTHON_ONLY_KEYS`, `Esc` também vale; `python_key`). O Escape é só do
  Python: as gravações e o Modo AFK continuam recusando (Esc + L sai do jogo, e
  eles rodam sem ninguém escrevendo o passo seguinte); o AGENTS.md e o API.md
  avisam. `type_text` só letra
  (maiúscula com Shift), dígito e espaço, e confere tudo antes de mandar a
  primeira tecla; cada tecla dele fica registrada no lease enquanto está
  apertada, como as do `key_down`, para o fim da execução soltá-la se o pedido
  morrer no meio (`type_text_keys_are_released_if_the_request_dies`). Envio pelas portas do `input.rs` (`press_python_key`
  — a das gravações mais o Escape —, `click_recording_point`); o primeiro clique depois de cada `bring_forward`
  leva o clique de foco.
- Depois do Parar só passam `run.stopped/log/status`, `focus.release`,
  `input.key_up`, `window.restore_focus` e `accounts`; o resto devolve `stopped`.
- Nada fecha, mata ou minimiza cliente do Roblox.

### Fila de foco

O lease é o guarda do mesmo `AFK_CYCLE_LOCK` do Modo AFK e das Gravações: com um
script segurando a fila, o ciclo do AFK espera, e vice-versa
(`focus_queue_is_shared_with_afk`). Lease sem renovar por 30 s é solto pelo vigia
do `serve` (com linha `system`); soltar o lease — pelo script, pelo vigia ou no
fim da execução — solta as teclas que ela deixou apertadas, na ordem inversa.
`window.restore_focus` devolve o foco à janela que estava na frente antes do
primeiro `bring_forward` (`give_focus_back_after_cycle`) **só com a fila livre
de verdade**: sem lease nenhum (nem de outra thread da mesma execução), sem
ciclo do Modo AFK ou gravação (que seguram o lock sem lease) e sem ninguém
esperando a vez (`FocusQueue::hold_if_free`, que usa `try_lock` no mesmo lock e
o contador de quem espera). A volta roda segurando o lock, então ninguém entra
no meio. Fila ocupada → `denied`, e a janela a devolver continua guardada: quem
soltar por último devolve (`restore_focus_only_runs_when_the_queue_is_free`).

Quanto o `account.focused()` (bloco `focus.py`) espera a vez: com
`timeout_ms`, aquilo (depois `FocusTimeout`); sem, 20 s — menos no modo
`all_together`, em que cada thread espera a vez o quanto for preciso, em
pedaços de 60 s, até o Parar (antes as outras contas caíam com `FocusTimeout`
quando uma segurava a fila por mais de 20 s;
`test_all_together_waits_for_its_turn_as_long_as_it_takes`). A regra para quem
escreve script é bloco curto, uma ação por vez, e esperar pixel/imagem fora
dele (o `click_image` já espera fora e só pega a fila para o clique).

## Execuções

Uma execução = script + contas + modo (`one_at_a_time` | `all_together`) +
gatilho (`manual` | `interval` N min | `loop`), um processo Python por rodada.
Várias convivem, mas **um script só tem uma execução viva por vez**
(`starting`/`running`/`waiting`/`stopping`), venha da tela ou do terminal: a
segunda é recusada com "This script is already running. Stop it first."
(`RunManager::start`, `the_same_script_cannot_run_twice`) e o Run da tela fica
desligado enquanto o script roda. **Stop** marca a parada (o `serve` manda o `stop`), espera 3 s e
mata o processo Python e só a árvore dele (job object com
`KILL_ON_JOB_CLOSE`). **Stop all** para todas. O gatilho de intervalo reabre a
execução N minutos depois de a rodada terminar (também depois de uma falha),
até o usuário parar; a execução fica `waiting` com `next_run_ms`. As 30 últimas
terminadas continuam na lista. O log guarda as últimas 5 000 linhas
(`python_get_logs`) e as linhas novas saem em `python-log` **em lote**: o payload
é um **array** de linhas, um evento a cada 100 ms no máximo (≤ 10/s; antes era
um evento por linha, e um script imprimindo sem parar inundava a tela). A tela
aceita também uma linha solta (`log_lines_are_sent_in_batches`, teste "takes a
batch of lines in one event"). Mensagem (`run.log`, `run.status`, stderr,
linha `system`) maior que 8 KB é cortada com "… (truncated, N KB in total)"
(`MAX_LOG_MESSAGE`, `huge_log_lines_are_cut_at_8_kb`); trava sai com nível
`blocked`, `code` e `hint`; `run.status` vira linha `system` "Status: ...".
Linhas `system` também para a fila de foco (`code: "focus"`, com a conta):
"Focus taken." e "Focus released after N ms." (com as teclas soltas, se houve),
**no máximo uma vez por segundo por execução** — as vezes puladas viram
"(N turns not shown)" na próxima (`focus_turns_are_logged_without_flooding`); e
para a instalação do Python: começo e fim de cada download, a saída do pip, o
pronto e o erro, sem o andamento a cada MB (`install_log_line`).

Nomes no terminal e o modo **Names hidden**: as linhas levam o `userId` e a
coluna da conta já sai pelo `useAccountLabel`. O texto da mensagem também tem
nome (o backend escreve "Run started: farm.py on Main_01", "Blocked: Main_01's
window..." — o Python precisa do nome de verdade nas exceções —, e o script
imprime `account.name`), então a tela troca, na hora de mostrar, todo username
e alias **inteiro** das contas pelo rótulo do app (`maskNamesInText` em
`components/python/logNames.ts`, com o `accountLabel` de
`utils/accountName.ts`). A busca e o "Copy visible" usam o texto mascarado.
O log do backend (`python_get_logs`) continua com o nome real, como o resto do
app (só o que aparece é mascarado).

**Fechar o MultiAlt** com execução viva (`RunEvent::ExitRequested`/`Exit` →
`python::shutdown_on_exit`, uma vez só): para todas, espera até 1 s elas saírem
e solta o lease e as teclas que sobraram (`runs::shutdown`,
`closing_the_app_stops_runs_and_releases_held_keys`). Antes, o Python morria
com o job object e a tecla apertada ficava presa no Windows. Nenhum cliente do
Roblox é fechado.

Chacoalhar o mouse (`Afk.ShakeToStop`) para **todas** as execuções Python: o
vigia liga com execução viva (`python::runs_active()`) — a cada Run da tela e a
cada execução do terminal aceita pelo pipe (`handle_client` chama o
`on_started`, que no app é o `ensure_shake_watch`) —, e o
`afk-emergency-stop` ganhou `python: <quantas pararam>`.

## Dashboard

Uma seção por script, em `dashboard.json`: `layout` (widgets `stat`, `progress`,
`text`, `badge`, `table`, `accounts`, `line`, `log`), `values` (`"id"` ou
`"id@userId"`) e `series` (`id → [[ts, v]...]`). O formato de cada valor é do
SDK (docs/API.md); o Rust só guarda JSON e aplica os limites: 40 widgets (o
`define` com mais é recusado e o layout de antes fica), texto até 2 KB (cortado),
listas até 200 itens (ficam as últimas), 500 pontos por série, 4 000 valores por
script. A tela recebe no máximo 4 atualizações por segundo por script
(`python-dashboard-update`); o arquivo é gravado no máximo uma vez por segundo.
**Clear** apaga a seção.

## Inspetor

`python_inspector_capture(userId)`: só cliente do tracker, não minimizado; PNG
reduzido a no máximo 1280 px de largura (as frações não mudam), com o
`frameId` do quadro. O quadro em resolução cheia fica guardado (o último de
cada conta, `inspector::FrameCache`).
`python_save_asset(userId, rect, name, frameId)`: recorte em resolução cheia
**daquele quadro** — o que a pessoa viu e onde arrastou — em `assets/<name>`
(`name` já vem com `.png`; sem extensão ganha `.png`; só `[A-Za-z0-9_-]`).
Antes ele capturava de novo, e a janela podia ter mudado entre o Capture e o
arrastar. Quadro que não é mais o último da conta → "That capture is no longer
available. Capture again, then select the area."
(`the_asset_is_cut_from_the_frame_on_screen`).

## Explorer e Global functions (tela v2)

Comandos (stubs sem a feature: leitura devolve vazio, ação `NOT_AVAILABLE`):

| Comando | O que faz |
|---|---|
| `python_fs_tree() -> FsNode` | árvore: `scripts/`, `lib/`, `assets/` (graváveis), `examples/` (só leitura), `multialt/defaults/` (só os 5 blocos, `pinned`; `writable` = Modo desenvolvedor) |
| `python_fs_read(path) -> String` | texto até 1 MB; binário (NUL ou UTF-8 inválido) → erro claro; bloco lê pelo `read_block` |
| `python_fs_read_image(path) -> String` | data URL (`data:image/<png\|jpeg\|bmp\|gif\|webp>;base64,…`), só em `assets/`, até 10 MB |
| `python_fs_write(path, content)` | só `scripts/`, `lib/`, `assets/` (pasta tem de existir, até 1 MB); bloco → `write_block_gated`, o mesmo caminho do `python_write_default_block` |
| `python_fs_create(path, kind)` | `"file"` (vazio) ou `"dir"`, nunca sobrescreve, só dentro das graváveis |
| `python_fs_rename(from, to)` | dentro da mesma pasta gravável de topo (move entre subpastas vale; a raiz não se renomeia) |
| `python_fs_delete(path)` | move para `.trash/<ms>-<nome>` (colisão ganha `-<n>`); a raiz não se apaga |
| `python_list_global_functions() -> Vec<GlobalModule>` | os 5 blocos (com Modified / New default available) e os módulos de `lib/` (`lib/a/b.py` → `lib.a.b`; `__init__.py` sem função não aparece; nome que não é identificador Python não é importável e fica de fora) |

Tipos em `types.rs` (`FsNode`, `GlobalModule`, `GlobalFunction`, camelCase).
`FsNode.runnable` só para `scripts/<nome>.py` do topo com nome válido de
script (o Run continua por nome simples). `pinned` vai nos blocos **e** nas
pastas `multialt` e `multialt/defaults`.

Regras de caminho (`explorer::split_path`): relativo, `/`, sem `\` nem `:`,
cada pedaço `^[A-Za-z0-9 _\-.]{1,64}$` sem começar com `.`; também recusa
terminar em espaço ou ponto e nome de dispositivo (`CON`, `NUL`, `COM1`...).
Depois o ancestral mais fundo que existe é resolvido (`canonicalize`) e tem de
cair dentro da pasta: junção em `lib/` apontando para fora é recusada, e a
árvore pula link/junção, oculto (`.`) e `__pycache__`. Ocultos e recusados:
`multialt/_core`, `multialt/__init__*`, o resto do SDK, `.trash`, `runs.json`,
`dashboard.json`, `.originals.json`, `.seeded-examples`.

As funções saem do texto (`globals::parse_functions`): `def`/`async def` na
coluna 0, fora de string e de parênteses; assinatura de várias linhas vira uma
(espaços colapsados, comentários e vírgula final fora, sem a anotação de
retorno); doc = primeira linha não vazia da docstring (até 200 caracteres);
nome com `_` na frente não aparece. Nunca roda Python.

## Modo desenvolvedor

`Python.DeveloperMode` (padrão `false`, [settings.md](settings.md)). Ligado:
libera a edição dos blocos e abre o pipe `\\.\pipe\multialt-python-<fnv1a64 do
SID, hex>`, com DACL `D:P(A;;GA;;;<SID do usuário>)` e clientes remotos
recusados; o nome vai para `multialt/_core/pipe.txt` (apagado ao desligar). No
boot, com o modo ligado, o canal reabre. O cliente
(`python -m multialt run <script> --account <nome>...`) manda `cli.start`; conta
desconhecida responde `{"error":{"message","hint"}}` e fecha; aceito, recebe o
`hello` e vira uma execução `source: "terminal"` (o script roda no processo do
terminal, uma rodada só — o laço é do script). Sem a feature `python-dev-pipe`
o modo ainda libera os blocos e o log avisa que o canal não existe na build. É a
única exceção nova à regra "só comandos Tauri" ([architecture.md](../architecture.md)).
Ligar e o canal não abrir (ou desligar e ele não fechar) desfaz a gravação do
`Python.DeveloperMode` (`persist_then_apply`,
`developer_mode_is_rolled_back_when_it_cannot_be_applied`): antes ficava
`true` gravado e o boot seguinte tentava abrir o canal de novo.

## Testes

Suíte `python` (`bun run t python`): o filtro Rust `python_` pega todo
`mod python_*_tests` — erros, tipos, runtime (download mockado, hash errado,
`._pth`, reinstall), workspace (sync, blocos, restore, nomes, modelos), exemplo semeado uma vez só
(`python_seed_tests`), Explorer (`python_fs_tests`: travessia e junção
recusadas, escrita fora das graváveis recusada, apagar vai para `.trash`,
árvore esconde os internos, `runnable` só no topo de `scripts/`), Global
functions (`python_globals_tests`: assinatura de várias linhas, docstring,
`def` dentro de string), Escape aceito no Python e recusado nas gravações
(`escape_is_a_python_key_but_not_a_recording_key`,
`python_keys_add_escape_but_recordings_still_refuse_it`),
primitivas (cada trava, lease que expira solta teclas, sleep que para, pixel,
`type_text`, fila compartilhada com o AFK), bridge (ids, linha quebrada,
fire-and-forget, `stop` primeiro, canal que cai solta o lease), execuções (duas
juntas, intervalo, stop all, falha), dashboard (limites, 500 pontos,
persistência, 4/s), inspetor, pipe (nome, `pipe.txt`, conta desconhecida,
execução "terminal", e o **CLI de verdade pelo pipe de verdade**), o
`python_e2e_tests::real_python_round_trip` (Python do PATH + SDK embutido) e o
`python_sdk_unittest_tests` (os `unittest` do SDK). Os que precisam de Python
passam com aviso quando não há `python` no PATH. Manuais (`--ignored`):
`real_install_from_the_official_sources` (baixa ~64 MB e confere que `numpy` e
`cv2` importam) e o spike de captura abaixo.

## Spike P0 (21/10/2026)

**Provado:**

- `PrintWindow(hwnd, hdc, PW_CLIENTONLY | PW_RENDERFULLCONTENT)` numa janela
  Chromium (classe `Chrome_WidgetWin_1`, renderização por GPU) **atrás** de
  outra janela devolve pixels reais: três janelas de 2560×1392, todas fora do
  primeiro plano, saíram com 100% dos pixels não pretos. Teste manual
  `python_capture_tests::spike_printwindow_on_a_background_gpu_window`
  (`cargo test --all-features --lib python_capture_tests -- --ignored --nocapture`);
  ele imprime só a classe, o tamanho e a contagem, nada do conteúdo da tela.
- A memória compartilhada nomeada (`CreateFileMappingW`) é lida pelo
  `mmap.mmap(-1, n, tagname=nome)` do Python 3.12.10 byte a byte
  (`python_capture_tests::python_mmap_reads_the_shared_frame`, roda na suíte).

**Não provado:** a captura num cliente **Roblox** de verdade — não havia
cliente aberto no PC durante o spike (e nenhuma conta foi aberta para isso). O
Roblox desenha por Direct3D como o Chromium; se o `PrintWindow` vier preto nele
com a janela na frente, o código já cai para a cópia da tela
(`capture_client_bgra`). Com a janela atrás de outra e preta, o resultado é uma
imagem preta (não erro) — conferir no primeiro teste com cliente real.

## Desvios do plano

- **pip sem `get-pip.py`:** o wheel do pip (hash do PyPI) é aberto no
  `Lib\site-packages` e o resto é `python -m pip`. Rodar o pip de dentro do
  wheel para ele se instalar é recusado pelo próprio pip no Windows.
- Todo `python.exe` leva `-s -E -B` além do `-u -X utf8` do plano (ver "Python
  portátil").
- `RunCtx` ganhou `desk`, `focus` e `state` (o contrato só listava
  `run_id/accounts/stop/sink`); `RunManager` recebe um `Executor` (processo ou
  pipe) para os testes não dependerem de Python.
- Comandos que precisam do estado vivo (`python_stop_run`, `python_list_runs`,
  `python_get_dashboard`, `python_clear_dashboard`, `python_get_logs`,
  `python_write_default_block`) recebem também o `AppHandle` — invisível para a
  tela (o Tauri injeta).
- `screen.capture` mantém os 4 últimos quadros por execução, não só o último.
- `window.bring_forward` também exige o lease (trocar a janela da frente é
  disputar o foco); `window.restore_focus` não, mas responde `denied` com a
  fila ocupada (lease de qualquer thread, ciclo do AFK ou alguém esperando).
- Execução do terminal não repete pelo gatilho de intervalo (a rodada é a
  conexão do CLI), e por isso o `cli.start` com `interval` é **recusado** com
  dica ("Terminal runs do not repeat every N minutes."; o CLI já recusa
  `--trigger interval:<min>` antes de conectar) — antes era aceito e ignorado.
- Contrato mudado na revisão de 21/10/2026 (o plano ainda mostra o antigo):
  `InspectorFrame` ganhou `frameId` (`frame_id: u64`);
  `python_save_asset(userId, rect, name, frameId)`; o evento `python-log` leva
  um **array** de `PythonLogLine` (a tela aceita também uma linha);
  código de erro novo `internal`; `account.focused(timeout_ms=None)` (antes
  `20000`; sem valor, 20 s ou a vez no `all_together`); `python_start_run` e o
  `cli.start` recusam script já rodando ("This script is already running. Stop
  it first."); `python_install_runtime`/`python_reinstall_runtime` recusam com
  execução viva; a mensagem do `forbidden` é "Not available through MultiAlt's
  API: cookies and passwords are never sent to scripts.".
