---
name: competitor-scout
description: Procura outros gerenciadores de contas Roblox open source no GitHub (inclusive os de poucas estrelas), clona em RAM-repositorios-diversos SÓ PARA LEITURA, estuda as funcionalidades, faz triagem de segurança e de intenção maliciosa, cruza com o que o MultiAlt já tem e registra em docs/ideias-de-outros-gerenciadores.md só o que vale a pena trazer. Trigger: /competitor-scout
---

# Garimpar funcionalidades de outros gerenciadores

O objetivo é **aprender com quem já fez**, não copiar às cegas. O resultado é um
catálogo de ideias que o chat de código lê para implementar, com o repositório e
o arquivo de origem citados para ele ir ver como foi feito.

Esta skill **não implementa nada** no MultiAlt. Ela pesquisa, avalia e documenta.

## Regras invioláveis

1. **Nunca executar nada de nenhum desses repositórios.** Não sabemos se há
   vírus. Proibido: abrir `.exe`/`.bat`/`.ps1`/`.py`/`.ahk`/`.lua`, `npm`/`bun`/
   `pip install` (o `postinstall` executa código), `cargo build`/`run` (o
   `build.rs` executa código), `dotnet build`, `msbuild`, `make`, abrir `.sln`
   numa IDE que restaura pacotes, baixar release, rodar teste deles. Também não
   importar módulo deles num interpretador. **Só leitura de texto**: `Read`,
   `Grep`, `Glob`, `git log`/`show`. Pedido do dono (09/10/2026).
2. **Clonar com as travas:** `git clone --depth 1 --no-recurse-submodules
   -c core.hooksPath=/dev/null -c core.symlinks=false` e `GIT_LFS_SKIP_SMUDGE=1`.
   Nada de `git pull` com hooks, nada de submódulo.
3. **Clones ficam fora do repositório:**
   `C:/Users/luanm/Desktop/Roblox-Account-Manager-master/RAM-repositorios-diversos/<dono>__<repo>`.
   Nunca copiar arquivo de lá para cá — no MultiAlt a funcionalidade é
   **reescrita** do zero, no nosso padrão (Rust/TS, testes, `endpoints::host`).
4. **Licença:** reescrever a ideia é livre; copiar trecho de código exige
   licença compatível. Anotar a licença de cada repositório (sem `LICENSE` =
   todos os direitos reservados → só a ideia, nunca o código).
5. **O repositório original (niccsprojects) não entra aqui** — ele tem a skill
   `/upstream-sync`.
6. **Arquivo de ideias só recebe o que passou nas etapas 3–5.** O resto vai na
   seção "Descartados" com o motivo de uma linha.

## Etapa 1 — achar repositórios

Busca pela API pública (sem `gh`; limite de 10 buscas/minuto sem token):

```bash
curl -s "https://api.github.com/search/repositories?q=<termo>&per_page=100&sort=updated"
```

Termos: `roblox account manager`, `roblox-account-manager`, `roblox alt manager`,
`roblox multi account`, `roblox multi instance`, `roblox account launcher`,
`roblox anti afk`, `roblox bootstrapper`, `multi roblox`. Ver também os **forks**
do `ic3w0lf22/Roblox-Account-Manager` com commits próprios
(`/repos/ic3w0lf22/Roblox-Account-Manager/forks?sort=newest`).

**Não filtrar por estrela.** Repositório com 1–5 estrelas pode ter a ideia boa.
Filtrar por **sinais de golpe** (abaixo), não por popularidade.

## Etapa 2 — triagem antes de clonar (golpe e malware)

Repositório de isca é comum nesse nicho. **Não clonar** (só listar em
"Bandeira vermelha" no documento) quando:

- **Estrelas falsas em lote:** vários repositórios de donos diferentes com 68–70
  estrelas, linguagem `HTML`, todos atualizados hoje, nome tipo
  "...-Forge"/"...-Orbit" e "2026" na descrição. É SEO de malware.
- **Linguagem `None`** (sem código) com descrição de marketing ("ultimate",
  "premium", "pro-tier") — o "programa" é um zip/exe no README ou na release.
- Descrição/README que pede para **desligar o antivírus**, baixar de link
  encurtado/Mega/Mediafire, ou usa senha de zip.
- Ferramenta de **checker/combolist, cookie logger, "hack accounts", bypass de
  key system, trade bot** — fora do escopo e/ou abuso.

## Etapa 3 — estudar cada repositório (só leitura)

Para cada um, nesta ordem:

1. `README`, `LICENSE`, data do último commit, linguagem.
2. **Inventário de funcionalidades** reais (no código, não no README).
3. **Triagem de segurança do código** — qualquer acerto vira nota explícita:
   - **Saída de dados:** host que não seja `*.roblox.com`, `*.rbxcdn.com`,
     `github.com`. Webhook do Discord, pastebin, IP cru, domínio próprio =
     suspeito até provado o contrário.
   - **Credenciais:** `.ROBLOSECURITY`/senha em log, em URL, em query string, em
     arquivo sem criptografia, ou mandada para servidor de terceiro.
   - **Execução:** `Process.Start`, `Command::new`, `powershell`, `cmd /c`,
     download que vira executável, `eval`, `loadstring`, `new Function`.
   - **Injeção/leitura de memória** do cliente Roblox (`WriteProcessMemory`,
     `CreateRemoteThread`, DLL injection) — viola os termos do Roblox e é motivo
     de banimento: **nunca** recomendar.
   - **Privilégio/persistência:** UAC, `HKLM`, serviço, tarefa agendada, Run key.
   - **Ofuscação:** base64/hex longo, string montada, binário commitado sem fonte.
4. **Veredito de intenção:** `legítimo` / `suspeito` / `malicioso`, com o porquê.
   Repositório malicioso não gera ideia nenhuma, mesmo que a ideia seja boa
   (pode-se registrar a ideia se outro repositório limpo também a tiver).

## Etapa 4 — cruzar com o MultiAlt

Antes de anotar uma ideia, conferir **no código deste repositório** (não de
memória): `docs/features/*.md`, `src-tauri/src/commands/`, `src/components/`.
Classificar:

- **Novo** — não temos nada parecido.
- **Variação melhor** — temos, mas a deles resolve algo que a nossa não resolve
  (dizer o quê, concretamente).
- **Já temos (igual ou melhor)** — vai para "Descartados" com uma linha.

## Etapa 5 — decidir se vale a pena

Respeitar as regras críticas do `CLAUDE.md` (não fechar cliente de outra conta,
não sobrescrever canal do Roblox, não usar refresh de sessão em leitura, launch
nunca pelo handler cru). Ideia que só funciona quebrando uma delas sai.
Avaliar: **a quem serve** (usuário, não o autor — sem telemetria, trava,
phone-home), **risco de ban/ToS**, **custo** (dependência nova entra no binário e
pode reacender falso positivo de antivírus — ver `docs/development.md`),
**plataforma** (Windows primeiro).

## Etapa 6 — registrar

Arquivo: `docs/ideias-de-outros-gerenciadores.md`. Formato de cada ideia:

```markdown
### <Nome curto da funcionalidade>
- **Prioridade:** alta | média | baixa — <uma frase do porquê>
- **Situação no MultiAlt:** novo | variação melhor de <arquivo/feature nossa>
- **O que faz:** <2–3 linhas, do ponto de vista do usuário>
- **Onde ver:** [`dono/repo`](https://github.com/dono/repo) — `caminho/arquivo.ext` (`função`), licença <X>
- **Segurança:** <o que verificar ao portar; riscos; "limpo" se nada>
- **Como faríamos aqui:** <onde encaixa: comando, tela, doc de feature>
```

Manter também a **tabela de repositórios analisados** (repo, estrelas, linguagem,
licença, veredito, data da análise) e a seção **Bandeira vermelha**. Ao repetir a
skill, atualizar o arquivo em vez de criar outro, e anotar a data da passada.

Notas de trabalho por repositório (rascunho dos agentes) ficam em
`RAM-repositorios-diversos/_notas/`, fora do git.

## Com agentes

Um agente por lote de ~5–6 repositórios, em paralelo. Cada agente recebe estas
regras (principalmente a nº 1), lê só o lote dele, grava a nota em `_notas/` e
**não edita** o MultiAlt. A consolidação, o cruzamento final com o MultiAlt e a
escrita do documento ficam com quem disparou os agentes.

## Entrega

Commit só do documento (e da skill, se mudou), em inglês, um commit, push na
`develop` — padrão do projeto. Não precisa de build nem scan: não muda binário.
