# Automação do Roblox Player em Python — design

> **Branch local e secreta** (`feature/python-scripts`): não dar push nem citar a
> funcionalidade em commit/PR/notas públicas até o dono liberar (21/10/2026).

## 1. Objetivo

Pedido do dono (21/10/2026), meta prometida a um membro técnico do Discord: uma
forma de rodar **scripts Python** que automatizam os **clientes do Roblox** de
cada conta — o mesmo espírito das [gravações](../../features/recordings.md)
(trazer a janela de cada conta, agir nela, passar para a próxima), mas com código
de verdade: ler a tela, decidir, repetir.

Público: usuários técnicos, que escrevem Python ou pedem para uma IA (Claude)
escrever. Por isso a pasta de trabalho é feita para ser aberta por uma IA, com
instruções e referência da API.

Não é: injeção no cliente, executor de Lua, nem substituto dos scripts JS (que
automatizam o **MultiAlt** e continuam iguais).

## 2. Decisões (21/10/2026)

| Tema | Decisão |
|---|---|
| Onde fica na tela | Dentro da página **Scripts**, que ganha duas opções no topo: **"Roblox Player automation (Python)"** e **"MultiAlt automation (JavaScript)"**. Nada novo na barra lateral. |
| Idioma | **Só inglês**, sem i18n: os textos da parte Python não passam por `t()`. |
| Python | **Portátil, oficial, baixado sob demanda** — só quando o usuário aperta "Install Python" (nada baixa sozinho). Mais `numpy` e `opencv-python-headless`. |
| Formato do script | Por conta (`main(account)`) **ou** controlador (`main_all(accounts)`); o runner detecta qual existe. |
| Execução | **script + contas + modo + gatilho**. Modo: uma por vez / todas juntas. Gatilho: manual, intervalo (a cada N min), loop do próprio script. Várias execuções ao mesmo tempo, em contas diferentes ou não. |
| Visão (v1) | Pixel/cor e **achar imagem** (OpenCV, multi-escala). OCR e afins ficam para depois. |
| Canal | **Híbrido**: stdin/stdout do processo filho (sempre) + *named pipe* opcional no **Modo desenvolvedor** para rodar pelo terminal/Claude. |
| Código de troca de tela | **Em Python, editável**: o Rust só dá primitivas; a orquestração (foco, troca, chamar `main`) é um **bloco padrão** que o usuário pode reescrever. |
| Modo desenvolvedor | Desligado por padrão. Ligado: mostra e deixa editar os **blocos padrão**, com **"Restore default" por bloco** (nunca um botão único para tudo), e abre o canal do terminal. |
| Segurança | Travas no **Rust**, que valem mesmo com os blocos reescritos (seção 6), com erros claros. |
| Edição | Feature do Cargo `python-scripts` (+ `python-dev-pipe`). Vai para a padrão se o scan der 0; se marcar, só na completa. |

## 3. Arquitetura

```
 Página Scripts ── aba Python (React) ──invoke/eventos──► Rust (src-tauri/src/python/)
                                                          │  runtime.rs   instala/verifica o Python portátil
                                                          │  workspace.rs pasta de trabalho, blocos padrão, reset
                                                          │  bridge.rs    processo filho + protocolo JSON
                                                          │  primitives.rs janelas, foco, entrada, captura (+ travas)
                                                          │  runs.rs      execuções, gatilhos, parar
                                                          │  devpipe.rs   named pipe (feature python-dev-pipe)
                                                          ▼
                                     python.exe -u -m multialt._host  (um processo por execução)
                                       └─ multialt/defaults/runner.py ─► main(account) do usuário
```

Reuso do que já existe (chamado, não copiado): tracker de processos/janelas, a
trava de ciclo do AFK (`AFK_CYCLE_LOCK`) como **fila de foco única**,
`bring_forward_for_cycle` / `give_focus_back_after_cycle`, `input.rs` (o único que
chama `SendInput`), o vigia de chacoalhar o mouse e o download com hash do
Chromium como modelo para o do Python.

A lógica de *como* trocar de janela e chamar o `main` **não** é a do AFK
reimplementada em Rust: é Python nos blocos padrão, para o usuário ajustar.

## 4. Python portátil (`runtime.rs`)

- Estado na aba: **Not installed** → botão **"Install Python (~65 MB)"**, com a
  lista do que baixa e de onde (python.org e PyPI). Sem clique, nada baixa.
- Baixa o *embeddable zip* oficial do CPython (versão fixa, SHA-256 fixo) para
  `<pasta de dados>/python/runtime/`, ajusta o `pythonXY._pth` (liga `site` e
  põe a pasta de trabalho no caminho), instala o pip pelo `get-pip.py` (hash
  fixo) e depois `numpy` + `opencv-python-headless` com `--require-hashes`.
- Progresso por etapa na tela; erro diz a etapa e o motivo (sem rede, hash não
  bate, disco cheio). **Reinstall** apaga só `runtime/`, nunca os scripts.
- `pip install` de pacote extra fica com o usuário (comando copiável na aba).

## 5. Pasta de trabalho (`workspace.rs`)

`<pasta de dados>/python/` (botão **Open folder**):

```
scripts/            scripts do usuário (.py) — a lista da aba é esta pasta
lib/                funções globais do usuário (from lib.x import y)
assets/             imagens para find_image (o inspetor salva recortes aqui)
multialt/
  _core/            cliente do protocolo — fixo, reescrito a cada abertura (só leitura)
  defaults/         BLOCOS PADRÃO — editáveis no Modo desenvolvedor
    runner.py       uma por vez / todas juntas; chama main / main_all
    focus.py        account.focused(): pega a fila, traz a janela, confere, devolve
    vision.py       wait_pixel, find_image, wait_image, click_image
    actions.py      press, click, type_text, esperas com variação
    dashboard.py    farm_report() e helpers do dashboard (seção 8b)
  __init__.py       API pública (Account, accounts(), ...)
  __init__.pyi      tipos para editor e IA
examples/           exemplos prontos
AGENTS.md           instruções para IA (e CLAUDE.md apontando para ele)
docs/API.md         referência completa
```

- Os originais dos blocos vão **embutidos no app** (`include_str!`). Restaurar
  não baixa nada.
- **Restore default por bloco**, com confirmação: *"Restore runner.py to the
  default? Your changes to this block will be lost. Your scripts and other
  blocks are not touched."* Badge **Modified** quando o arquivo difere do
  original (hash).
- Atualização do app: bloco **não** modificado é atualizado sozinho; bloco
  modificado é mantido e ganha o badge **New default available** (o usuário
  decide se restaura).
- `_core/`, `__init__`, `AGENTS.md`, `docs/` e `examples/` são do app: reescritos
  a cada versão.

## 6. Protocolo e primitivas (`bridge.rs`, `primitives.rs`)

**Canal:** o `_host` duplica o stdout original para o protocolo **antes** de rodar
código do usuário e troca `sys.stdout`/`sys.stderr` por escritores que viram
**log** — um `print()` nunca corrompe o protocolo. Mensagens em JSON por linha:
`{"id", "op", "args"}` → `{"id", "ok": true, "result"}` ou
`{"id", "ok": false, "error": {"code", "message", "hint"}}`. O app manda eventos
(`{"event": "stop"}`).

**Imagens:** a captura vai por **memória compartilhada nomeada** do Windows
(`CreateFileMapping`); o Python abre com `mmap.mmap(-1, n, tagname=...)` e o
`numpy.frombuffer` lê sem copiar por JSON. Captura com `PrintWindow`
(`PW_RENDERFULLCONTENT`), que funciona com a janela atrás de outras (não
minimizada); com a janela na frente, cai para cópia da tela se o primeiro método
vier preto. **A validar no spike** (seção 12, P0).

**Primitivas** (a única porta do Python para o app):

| Op | Faz |
|---|---|
| `accounts` | id, nome, apelido, grupo, no jogo?, place, servidor |
| `windows` | janela de cada conta: área interna (x, y, w, h), DPI, minimizada, na frente |
| `focus.acquire(account, timeout)` / `focus.release` | **fila de foco única** (a mesma do AFK e das gravações); devolve um *lease* que expira em 30 s sem renovar |
| `window.bring_forward(account)` / `window.restore_focus` | traz para frente e confere / devolve o foco a quem tinha |
| `input.key`, `key_down`, `key_up`, `type_text`, `click(x, y, button)` | entrada; ponto em **fração da área interna** (0–1) |
| `screen.pixel(account, x, y)`, `screen.capture(account, region)` | cor e imagem |
| `run.sleep(ms)`, `run.stopped`, `run.log`, `run.status` | espera que respeita o Parar, log, status por conta na aba |
| `dashboard.define`, `set`, `push`, `clear` | o dashboard do script (seção 8b) |

**Travas no Rust** (valem mesmo com os blocos reescritos), cada uma com erro claro
no log da conta:

| Código | Quando | Mensagem (exemplo) |
|---|---|---|
| `not_foreground` | entrada com a janela da conta fora da frente | *"Blocked: Main_01's window is not in front. Input only goes to the focused window — use `with account.focused():`."* |
| `no_focus_lease` | entrada sem ter pego a fila de foco | *"Blocked: take the focus queue first (`account.focused()`), so other runs, AFK and recordings don't type into the same window."* |
| `untracked_window` | conta sem cliente aberto pelo MultiAlt | *"Blocked: Main_01 has no Roblox client opened by MultiAlt."* |
| `minimized` | captura/entrada em janela minimizada | *"Main_01's window is minimized — `account.focused()` restores it."* |
| `forbidden` | pedido de cookie, senha ou comando fora da lista | *"Not available to scripts: cookies and passwords never leave MultiAlt."* |
| `focus_timeout` | a fila não liberou no prazo | *"Waited 20 s for the focus queue (AFK cycle running?)."* |
| `stopped` | o usuário parou | *"Run stopped."* (sem traceback) |

Mais: tecla apertada por uma execução e não solta é **solta** quando o lease
acaba ou o processo sai. Nada fecha, mata ou minimiza cliente. Só cliente aberto
pelo app (tracker) recebe.

## 7. Execuções (`runs.rs`)

- Uma execução = `{script, accounts[], mode: one_at_a_time | all_together,
  trigger: manual | interval(min) | loop}`; **um processo Python por execução**
  (o modo "todas juntas" é com threads dentro do `runner.py`, e a entrada passa
  pela fila de foco).
- Configuração salva por script em `RAMPython.json` (contas, modo, gatilho).
- **Run / Stop**; Stop manda `stop`, espera 3 s e mata **o processo Python**
  (nunca o Roblox). **Stop all**.
- Chacoalhar o mouse (`Afk.ShakeToStop`) para **todas** as execuções Python.
- Intervalo: o Rust reabre a execução a cada N min se ela não estiver rodando.
- Log ao vivo **por conta** (filtro por conta) e o último status de cada uma.
- Script com erro: traceback no log, e o erro do Python em destaque na linha da
  conta; as outras contas seguem (no modo todas juntas).

## 8. A tela

Topo da página Scripts: seletor **Roblox Player automation (Python)** |
**MultiAlt automation (JavaScript)** (a escolha fica lembrada).

Na parte Python:

1. **Faixa explicativa** curta: o que faz (controla as janelas do Roblox de cada
   conta: lê pixels, acha imagens, manda teclas e cliques, uma janela por vez) e
   o que não faz (não injeta, não lê memória do jogo). Aviso em destaque:
   **"Python scripts have full access to your PC. Only run scripts you trust."**
2. **Not installed**: card com Install Python (seção 4).
3. **Lista de scripts** (pasta `scripts/`, recarrega quando um arquivo muda —
   a IA edita por fora): New (a partir de um modelo), Open folder, Open in
   editor.
4. **Editor** simples do script selecionado (mesmo componente da aba JS) — salva
   no arquivo; mudança externa recarrega.
5. **Painel de execução**: contas (seleção e grupos), modo, gatilho, Run/Stop;
   lista **Running** (todas as execuções, de onde vieram: tela ou terminal);
   **log** por conta com os erros da seção 6 em destaque.
6. **Inspector** (ferramentas de ajuda): escolhe uma conta, mostra a captura da
   janela; passar o mouse mostra `x, y` (fração) e a cor; clicar copia o trecho
   (`account.click(0.52, 0.81)`, `account.wait_pixel(0.52, 0.81, (255, 200, 0))`);
   arrastar um retângulo salva o recorte em `assets/` para `find_image`.
7. **Modo desenvolvedor** (seção de configurações da aba): liga/desliga. Ligado,
   mostra a lista dos **blocos padrão** (abrir/editar, badge Modified / New
   default available, **Restore default** por bloco) e o **canal do terminal**
   com o comando copiável:
   `"<python>" -m multialt run scripts/farm.py --account Main_01 --mode one_at_a_time`.
   Texto na tela dizendo o que cada coisa liga.

## 8b. Dashboard (pedido de 21/10/2026)

Um lugar onde o **próprio script decide o que mostrar** — relatório do farm,
contadores, progresso, erros por conta. Sub-aba **Dashboard** na parte Python.

- **Quem desenha é o código**, por primitivas `dashboard.*`:
  - `dashboard.define(layout)` — lista de widgets com `id`, tipo e título;
  - `dashboard.set(id, value, account=None)` — valor geral ou por conta;
  - `dashboard.push(id, value)` — ponto novo num gráfico (guarda os últimos 500);
  - `dashboard.clear()`.
- **Tipos de widget (v1):** `stat` (número grande + variação), `progress`,
  `text`, `badge` (ok/aviso/erro), `table` (linhas livres), `accounts` (uma linha
  por conta com colunas que o script escolhe), `line` (gráfico no tempo), `log`
  (últimas N linhas que o script mandar).
- Cada script tem **a sua seção** no dashboard (título = nome do script); várias
  execuções aparecem lado a lado. Os últimos valores ficam salvos em
  `<pasta de dados>/python/dashboard.json`: o painel continua lá depois que a
  execução termina ou o app reinicia, até o usuário limpar (**Clear** por seção).
- Limites: 40 widgets por script, texto ≤ 2 KB, tabela ≤ 200 linhas, atualização
  na tela no máximo 4 vezes por segundo (o resto é agrupado).
- **Vem pronto para trabalhar em cima:** bloco padrão `defaults/dashboard.py`
  (editável, com Restore default como os outros) com o helper `farm_report()` —
  tabela por conta (status, voltas, último erro, tempo rodando), contador total e
  gráfico de voltas por hora — e o exemplo `examples/farm_dashboard.py`, que o
  usa. O runner padrão já alimenta status e erro por conta nesse relatório.
- Os textos dos widgets vêm do script; a tela só desenha (nada de HTML do script:
  sem risco de injetar conteúdo na interface).

## 9. Modo desenvolvedor e canal do terminal (`devpipe.rs`)

- Setting `Python.DeveloperMode` (padrão `false`). Desligado: blocos escondidos e
  pipe fechado.
- Pipe `\\.\pipe\multialt-python-<hash do SID>`, com DACL **só do usuário
  atual**, aberto só com o modo ligado e o app aberto. Nada de porta de rede.
- `python -m multialt run <script> --account <nome>... [--mode] [--trigger]`
  conecta no pipe e vira uma execução como outra qualquer (aparece em Running
  como "terminal", com o log nos dois lados). É o que deixa uma IA escrever,
  rodar, ler o erro e corrigir.
- Feature `python-dev-pipe` separada: se ela fizer o scan marcar, sai da padrão
  sem levar o resto.

## 10. Kit para IA

- `AGENTS.md` (e `CLAUDE.md` com uma linha apontando para ele): o que é a pasta,
  onde escrever (`scripts/`, `lib/`), regras (frações 0–1, sempre
  `account.focused()` antes de entrada, nada de loop sem `sleep`, checar
  `stopped()`), como testar pelo terminal com o Modo desenvolvedor, os erros da
  seção 6 e o que cada um pede para mudar.
- `docs/API.md` e `multialt/__init__.pyi` gerados da **mesma fonte** (um teste
  confere que batem).
- `examples/`: anti-AFK com pixel, clicar num botão por imagem, rotina por todas
  as contas, relatório do farm no dashboard.

## 11. Edições e antivírus

Features `python-scripts` e `python-dev-pipe` no `standard` se o scan da padrão
der 0/75 e Defender limpo; senão, só no `full` (regra do CLAUDE.md). Escanear o
`.exe` e o MSI das duas edições a cada fase que mexe no binário. O binário não
embute Python: só o download, o processo filho, a captura e o pipe.

## 12. Fases

| Fase | Entrega | Pronto quando |
|---|---|---|
| **P0 spike** | captura de janela do Roblox atrás de outra (`PrintWindow`) e memória compartilhada lida pelo `mmap` do Python | as duas provadas num cliente real; resposta escrita, código descartado |
| P1 | runtime (instalar/reinstalar) + pasta de trabalho + blocos padrão + restore por bloco | instala do zero, reset por bloco com confirmação |
| P2 | bridge + primitivas + travas e erros | um script de teste clica e lê pixel; cada trava tem teste |
| P3 | runner (uma por vez / todas juntas) + execuções + gatilhos + parar | duas execuções juntas sem atropelar AFK nem uma à outra |
| P4 | a tela (seletor, lista, editor, execução, log) | harness com cenários de instalado/não instalado/rodando/erro |
| P5 | inspector + dashboard (widgets, `farm_report()`, exemplo) | copia ponto/cor e salva recorte; o exemplo mostra o relatório do farm |
| P6 | Modo desenvolvedor + pipe + kit para IA | Claude roda um script pelo terminal e lê o erro |
| P7 | scan das duas edições, doc em `docs/features/python-automation.md` | portões da seção 11 |

## 13. Testes

- Rust: `#[cfg(test)] mod python_<parte>_tests` em cada arquivo — protocolo
  (linha quebrada, id desconhecido, erro), lease (expira, solta teclas), cada
  trava (`not_foreground`, `no_focus_lease`, `untracked_window`, `forbidden`),
  instalação com download mockado (hash errado falha), reset por bloco e badge.
- Frontend: vitest da aba (seletor, estados, confirmação do reset, erros no log).
- Python: testes dos blocos padrão contra um dublê das primitivas, rodados pela
  suíte só quando há Python disponível.
- Nova suíte `python` em `scripts/test-suites.ts`. Harness com cenário
  `?scenario=python-*`.

## 14. Regras que não mudam

Nada fecha cliente; só cliente do tracker recebe; `SendInput` só no `input.rs`;
cookie e senha nunca saem do app; nenhum `run_with_session_retry` em leitura; o
canal do Roblox não é tocado.

## 15. Fora de escopo (v1)

OCR; converter gravação em Python; gatilhos depois do launch/reconexão; macOS;
editor completo com autocomplete (a IA e o editor externo cobrem); porta de rede.
