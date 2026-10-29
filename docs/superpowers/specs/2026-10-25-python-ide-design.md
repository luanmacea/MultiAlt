# Python screen v2 — editor estilo VS Code, Global functions e exemplo — design

> **Atualização (28/10/2026):** o dono liberou a funcionalidade; ela entrou na `develop` e não é mais secreta. O aviso abaixo é histórico.

> Branch local e secreta `feature/python-scripts` (sem push). Trabalho na cópia
> `C:\mwt\py`. Pedido do dono em 25/10/2026, aprovado com as opções recomendadas.

## Decisões

1. **Aviso** "Python scripts have full access to your PC…" em **amarelo** (alerta,
   não erro), com **X** para fechar; fechado não volta (`localStorage`
   `ram_python_warning_dismissed`, try/catch).
2. **Abas:** `Code · Global functions · Dashboard · Inspector · Developer`.
3. **Code** (estilo VS Code):
   - **Explorer** à esquerda: árvore da pasta de trabalho — `scripts/`, `lib/`,
     `assets/`, `examples/` (só leitura, com "Copy to scripts"),
     `multialt/defaults/` (fixos, cadeado). Expandir/recolher, novo arquivo, nova
     pasta, renomear, apagar (confirmação; vai para `.trash/`), menu de botão
     direito. Ocultos: `multialt/_core`, `multialt/__init__*`, `.trash`,
     `runs.json`, `dashboard.json`, `.originals.json`, `.seeded-examples`.
   - **Editor** CodeMirror 6 (Python, números de linha, indentação, busca,
     desfazer) com **abas** de arquivos abertos, ponto de "não salvo", Ctrl+S,
     fechar aba (pergunta se sujo). Arquivo só leitura mostra faixa explicando por
     quê (exemplo do app / bloco padrão sem Modo desenvolvedor) e a ação possível.
     Imagens de `assets/` abrem como prévia.
   - **Run** na barra do editor (contas, modo, gatilho em popover; ▶ Run / ■
     Stop) — habilitado só para `scripts/*.py` do nível de cima (as execuções
     continuam por nome simples); em outro lugar mostra "Only files directly in
     scripts/ can be run".
   - **Painel inferior** (como o terminal do VS Code): o terminal Python e a lista
     Running, recolhível e redimensionável.
4. **Global functions:** lista de módulos com as funções de cada um (nome,
   assinatura, primeira linha da docstring) e o **import pronto para copiar**:
   - os 5 blocos padrão fixos, com cadeado, badges Modified / New default
     available, **Restore default** (por bloco, com confirmação) — editar e
     restaurar exigem o Modo desenvolvedor;
   - os módulos do usuário em `lib/` (**New function** cria `lib/<nome>.py` com
     um modelo e abre no editor).
   - Clicar num item abre o arquivo na aba Code.
5. **Developer:** só o liga/desliga e o canal do terminal (a lista de blocos sai
   daqui e vai para Global functions).
6. **Exemplo na primeira abertura** (uma vez só; apagado, não volta — marcador
   `.seeded-examples`):
   - `scripts/example_walk_jump.py`: por conta, em loop até parar — anda para
     frente (W), pula (Space), abre o menu (Escape), espera, fecha (Escape);
     alimenta o dashboard com `farm_report()` (status, voltas, voltas por hora);
     comentários mostrando como pegar ponto/cor no Inspector e usar
     `wait_pixel`/`click`.
   - `lib/helpers.py`: função global de exemplo `walk_forward(account, seconds)`
     importada pelo script (`from lib.helpers import walk_forward`), e o script
     também importa um bloco padrão (`from multialt.defaults.actions import …`).
7. **Tecla Escape liberada só para Python** (as gravações e o AFK continuam
   recusando). AGENTS.md avisa que Esc + L sai do jogo.

## Contrato backend ↔ frontend (novo)

```rust
#[serde(rename_all = "camelCase")]
pub struct FsNode { pub path: String /* relativo, com "/" ; "" = raiz */, pub name: String,
  pub kind: String /* "file" | "dir" */, pub size: u64, pub modified_ms: u64,
  pub writable: bool, pub pinned: bool /* blocos padrão */, pub runnable: bool /* scripts/*.py do topo */,
  pub children: Option<Vec<FsNode>> }
#[serde(rename_all = "camelCase")]
pub struct GlobalFunction { pub name: String, pub signature: String /* "walk_forward(account, seconds=1.0)" */, pub doc: String }
#[serde(rename_all = "camelCase")]
pub struct GlobalModule { pub path: String, pub module: String /* "lib.helpers" | "multialt.defaults.vision" */,
  pub pinned: bool, pub modified: bool, pub new_default_available: bool, pub functions: Vec<GlobalFunction> }
```

Comandos (stub sem a feature, como os outros):
- `python_fs_tree() -> FsNode`
- `python_fs_read(path) -> String` (texto, até 1 MB; binário → erro claro)
- `python_fs_read_image(path) -> String` (data URL `data:image/png;base64,…`, só em `assets/`)
- `python_fs_write(path, content)` — só em `scripts/`, `lib/`, `assets/`; bloco
  padrão → o mesmo caminho do `python_write_default_block` (Modo desenvolvedor)
- `python_fs_create(path, kind)` — `kind` = `"file"` | `"dir"`, só nas pastas graváveis
- `python_fs_rename(from, to)` — dentro da mesma pasta gravável de topo
- `python_fs_delete(path)` — move para `.trash/<ms>-<nome>`
- `python_list_global_functions() -> Vec<GlobalModule>` (lê `def` de nível de topo
  por texto, sem rodar Python)

Regras de caminho: relativo, `/` como separador, sem `..`, sem `:`/`\`, cada
segmento `^[A-Za-z0-9 _\-.]{1,64}$` sem começar com `.`; tudo resolvido e
conferido dentro da pasta de trabalho (canonicalize). Erros em inglês claros
("Only scripts/, lib/ and assets/ can be changed.", "That name is not allowed…").

## Testes

Rust (`python_fs_tests`, `python_globals_tests`, `python_seed_tests`): travessia
de caminho recusada, escrita fora das pastas graváveis recusada, apagar vai para
`.trash`, árvore esconde os internos, `runnable` só no topo de `scripts/`, parser
de `def` (assinatura multi-linha, docstring), exemplo semeado uma vez só, Escape
aceito no Python e recusado nas gravações. Frontend (vitest): aviso amarelo some
no X e lembra; árvore abre/fecha e cria/renomeia/apaga com confirmação; abas e
"não salvo"; só leitura; Run só em runnable; Global functions mostra import e
restore pede confirmação; harness com o exemplo semeado.
