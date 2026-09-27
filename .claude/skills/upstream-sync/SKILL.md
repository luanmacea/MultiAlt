---
name: upstream-sync
description: Compara este fork com o repositório original (niccsprojects/Roblox-Account-Manager), audita o que ele lançou depois do fork — segurança, bug fix e feature — valida se o código presta e se o problema existia de verdade, e só então traz o que vale a pena. Trigger: /upstream-sync
---

# Sincronizar com o repositório original

Este repositório é um fork **por cópia de arquivos**, não por `git fork`: não existe
ancestral comum no git. O upstream continuou lançando coisas — principalmente de
segurança e de arquitetura, que é onde o autor original é forte e o dono deste
fork não quer palpitar. A skill acha o que há de novo lá, **audita** e traz o que
presta.

## Regras invioláveis

1. **O repositório original é read-only.** Só `git log`, `git show`, `git diff`,
   `git ls-tree`, `git cat-file`, e leitura de arquivos. **Nunca** `fetch`,
   `pull`, `checkout`, `clean`, `gc`, `stash`, `add`, nem escrever arquivo dentro
   dele. Se precisar de algo mais novo que o clone local, pedir ao dono para
   atualizar o clone dele.
2. **Nunca executar o programa do upstream.** Nem o `.exe`, nem `bun run tauri
   dev`, nem `cargo run`, nem `bun install` dentro dele (script de `postinstall`
   executa código). Auditoria é leitura de código, não execução.
3. **Nada entra sem passar pela triagem de segurança** (etapa 3). Dúvida não
   resolvida = não entra, e o relatório diz por quê.
4. **Confiar no código, não na mensagem de commit.** "fix(api): retry on stale
   XSRF" só vale depois de confirmar no código que (a) o problema existia mesmo
   neste fork e (b) a correção resolve.
5. **Um commit por item portado**, com teste que falha primeiro, `bun run check`
   antes de commitar, e push — o padrão do projeto (ver CLAUDE.md).

## Onde ficam os dois repositórios

Estrutura esperada, na área de trabalho do dono:

```
Roblox-Account-Manager-master/
  RAM-repositorio-original/Roblox-Account-Manager/   <- upstream, READ-ONLY
  RAM-repositorio-pessoal/Roblox-Account-Manager-4/  <- este repositório
  executavel-novo/                                   <- onde ele guarda o .exe que usa
```

Se os caminhos não existirem, perguntar antes de sair procurando pelo disco.

## Etapa 1 — achar o ponto de fork

Não há ancestral comum, então o ponto de fork se descobre **comparando árvores**:
os hashes de blob do git são do conteúdo, então são iguais nos dois repositórios.
Rodar `scripts/find-fork-point.sh` (ao lado deste arquivo): ele pega o primeiro
commit deste fork e procura no upstream o commit cuja árvore mais bate.

Empate no topo (vários commits com a árvore idêntica) é normal — um merge commit
e o commit antes dele têm a mesma árvore. **Pegar o mais recente** dos empatados:
é o último estado que o fork já continha.

Resultado medido em 2026-09-26: `ddcb4e48` (Merge PR #63, 2026-06-08), árvore
`a4feb866…`, **idêntica** ao primeiro commit deste fork (`305fcb2`, "primeiro
commit seguro"). Guardar o ponto de fork no relatório; se ele mudar entre
execuções, algo está errado.

## Etapa 2 — listar e agrupar o que é novo

```bash
git -C "$UPSTREAM" log --reverse --no-merges --format='%h|%cs|%s' <fork>..v4
git -C "$UPSTREAM" diff --shortstat <fork> v4
git -C "$UPSTREAM" diff --numstat <fork> v4 | sort -k1 -rn | head -30
git -C "$UPSTREAM" diff --diff-filter=A --name-only <fork> v4
```

Agrupar por **tema**, não por commit: o upstream trabalha em PR com um commit de
feature e 3–6 de "fix review feedback" em cima. Avaliar o tema inteiro; portar
só o primeiro commit de uma série é trazer o bug e deixar a correção.

Arquivo novo é onde mora a surpresa — olhar a lista de `--diff-filter=A` antes
de qualquer outra coisa.

## Etapa 3 — triagem de segurança do delta (antes de avaliar utilidade)

Passar o delta inteiro por esta lista. Qualquer acerto vira item explícito no
relatório, mesmo quando termina em "benigno, e aqui está o porquê".

- **Saída de dados:** URL, IP, domínio ou endpoint novo. Comparar com o que o
  app já falava (`endpoints::host(...)`, API do Roblox). Qualquer host que não
  seja Roblox/GitHub/Crowdin é suspeito até provado o contrário.
- **Credenciais:** mudança em como `.ROBLOSECURITY`, senha, ticket de auth ou
  chave de criptografia é lida, gravada, logada ou passada adiante. Cookie em
  log, em URL, em query string ou em telemetria é reprovação imediata.
- **Criptografia:** mudança de formato de arquivo de contas, de derivação de
  chave, de "encriptado por default". Aqui o risco não é só maldade: é
  **lockout** — o dono perder acesso às contas dele. Procurar o caminho de
  recuperação e testar mentalmente o caso "a máquina mudou".
- **Execução:** `Command::new`, `powershell`, `cmd /c`, `ShellExecute`, download
  que vira arquivo executado, `include_str!` de script, `eval`, `new Function`.
- **Entrada sintética / automação de input:** `SendInput`, `keybd_event`,
  `SetForegroundWindow`, hooks. Legítimo num AFK mode; também é a primitiva de
  um keylogger. Ler linha por linha.
- **Privilégio:** UAC, elevação, escrita em `HKLM`, serviço, tarefa agendada.
- **Dependência nova:** `Cargo.toml`, `package.json`. Nome parecido com pacote
  conhecido (typosquat), autor desconhecido, crate sem uso aparente.
- **Build e CI:** `.github/workflows`, `build.rs`, `postinstall`, `scripts/`.
  É onde código roda sem ninguém olhar.
- **Ofuscação:** base64, hex longo, string montada por concatenação, comentário
  que não corresponde ao código, código morto que só liga por variável de
  ambiente.

Ferramentas: `git -C "$UPSTREAM" diff <fork> v4 -- <caminho>` e grep sobre o
diff salvo em arquivo no scratchpad. Nunca executar nada.

## Etapa 4 — classificar e cruzar com este fork

Para cada tema, responder quatro perguntas curtas:

1. **Categoria:** segurança / bug fix / feature / chore.
2. **A quem serve:** o usuário do programa, ou o autor do programa? Feature que
   atrapalha quem usa (telemetria, trava, "phone home", limite artificial) não
   entra, mesmo sendo bem escrita.
3. **Já existe aqui?** Este fork tem 100+ commits próprios; muita coisa foi
   resolvida por outro caminho. Checar no código deste repositório, não na
   memória. Se já existe, dizer **qual** implementação é melhor e por quê.
4. **Aplica-se a este fork?** Tema em cima de código que este fork reescreveu
   pode não ter mais sentido aqui.

## Etapa 5 — validar de verdade o que sobrou

Para cada candidato a portar, e **só** para esses (o resto não merece o tempo):

- **O problema existia?** Achar no código *deste* fork a linha que tem o
  defeito. Se não achar, o problema era do upstream e não nosso — descartar e
  dizer isso.
- **A correção resolve?** Ler o código, não o título. Procurar o caso que a
  correção deixa passar.
- **Qualidade:** trata erro ou engole? Tem teste? O teste testaria a ausência do
  código, ou passaria de qualquer jeito? Complexidade proporcional ao problema?
- **Custo:** o que ele arrasta junto (dependência, migração de formato de
  arquivo, mudança de comportamento que o dono vai notar).
- **Veredito:** trazer / trazer adaptado / não trazer — com uma linha de motivo.

## Etapa 6 — relatório

Curto e objetivo, agrupado em **Segurança**, **Bug fix**, **Feature**, e uma
seção **Não vale a pena** com uma linha por item descartado. Por item: uma frase
do que é, se aplica-se a este fork, e o veredito. Sem colar diff, sem repetir a
mensagem de commit. Fechar com o ponto de fork usado e quantos commits foram
auditados.

**Parar aqui e esperar o dono escolher.** Ele valida o relatório antes de
qualquer mudança no código.

## Etapa 7 — portar o que ele aprovar

Item por item, na ordem: segurança, depois bug fix, depois feature.

- **Reimplementar, não `git apply`.** Os dois repositórios divergiram demais; o
  patch do upstream não encaixa e, quando encaixa, encaixa errado.
- **Teste que falha primeiro** (`docs/development.md#testes`), teste novo numa
  suíte de `scripts/test-suites.ts`.
- **Sabotar o próprio teste** para confirmar que ele morde.
- `bun run check`, commit em português, push, e o build no fim (padrão do
  CLAUDE.md).
- **Crédito:** o código é de outro autor sob a licença do upstream. Citar a
  origem no corpo do commit (`origem: niccsprojects@<hash>`).

## Executar de novo mais tarde

Guardar no relatório o ponto de fork e o último commit upstream auditado. Na
próxima passada, a janela é `<último auditado>..v4` — não repetir o que já foi
decidido. Itens recusados ficam recusados; se o upstream mexer neles de novo,
reavaliar só aí.
