# Task 11 e Task 12 — relatório de implementação

Worktree: `.claude/worktrees/agent-addf09243b1dfc160`, branch `worktree-agent-addf09243b1dfc160`,
nascida da `main` (atrasada ~20 mudanças em relação à `develop`). Nenhum `merge`/`fetch`/`pull`
foi rodado; a `develop` só foi **lida** com `git show develop:<arquivo>`.

Arquivos que a `develop` também mexeu e que este trabalho encosta (conflito provável na
integração): `src/store.tsx`, `src/store.test.ts`, `src-tauri/src/data/settings/store.rs`,
`src/components/settings/GeneralTab.tsx`, `src/locales/{en,pt,de}/common.json`,
`src/components/server-list/ServerListDialog.tsx` (região diferente da que eu mexi).
Os quatro arquivos que o pedido mandou conferir (`ImportDialog.tsx`, `RecentGamesList.tsx`,
`server-list/types.ts`, `RecentTab.tsx`) estão **idênticos** entre `main` e `develop`, então a
base usada é a mesma.

---

## Task 11 — importar `username:password:cookie`

Origem: `niccsprojects@416a4de` (+ `32975e0`, `c7304a4`, `843f498`, `ff5e0ca`). Commit: `bafad8d`.

### O que mudou, por arquivo

| Arquivo | Mudança |
|---|---|
| `src/utils/cookies.ts` (novo) | `COOKIE_PATTERN` compartilhado e `parseImportLine(raw)` puro, devolvendo `{ kind, username, password, cookie }` com `kind` em `cookie` / `userpass` / `unknown` / `empty`. |
| `src/utils/cookies.test.ts` (novo) | 14 testes do parser, incluindo o do `:` dentro do cookie. |
| `src/components/dialogs/ImportDialog.tsx` | Aba Cookie passa a aceitar `user:pass:cookie`; aba User:Pass importa direto a linha que já traz cookie (sem navegador); helper `importByCookie` compartilhado pelas duas; a senha só vai ao `add_account` quando existe; textos novos. |
| `src/components/dialogs/ImportDialog.test.tsx` | 4 testes novos (aviso da senha, dica do formato, import de `user:pass:cookie`, linha incompleta pulada). |
| `src/components/accounts/AccountList.tsx` | Drag & drop de texto passa a usar o `COOKIE_PATTERN` do módulo em vez da regex copiada. |
| `src/locales/{en,pt,de}/common.json` | 4 chaves novas (+ pt); a chave morta "Paste one .ROBLOSECURITY cookie per line" saiu dos três catálogos. |
| `docs/features/accounts.md` | Linha nova na tabela de origens de conta e duas entradas em "Regras de negócio" (regras do parser e do aviso). |

### Decisões

- **O corte nunca é `split(":")`.** Acha-se onde o cookie começa (`COOKIE_PATTERN`, ou o marcador
  `_|WARNING` quando o texto do aviso não casa exatamente), o que está antes é o prefixo, e dele
  tira-se **só** o delimitador com `/\s*:\s*$/`. O `[\s:;,]+$/` do primeiro commit do upstream
  (corrigido por ele em `843f498`) comeria a pontuação final de uma senha legítima — há teste para
  isso (`hunter2;`).
- A senha pode conter `:`; só o **primeiro** `:` do prefixo separa usuário de senha.
- `user:cookie` (sem senha) vale como cookie sozinho. O `Username` gravado é sempre o que o
  `validate_cookie` devolve, não o da linha.
- **Linha incompleta é pulada** (`ff5e0ca`): na aba Cookie, `user:pass` sem cookie vira
  "Skipped `<nome>`: no cookie in this line" e a importação **segue** para a próxima linha.
- O `kind: "unknown"` (token solto, linha pela metade) existe para as duas abas divergirem sem
  ambiguidade: na aba Cookie a linha inteira vai para o backend validar (comportamento antigo), e
  na aba User:Pass ela cai na mensagem "Skipped invalid line (use username:password)" que já havia.
  Este quarto `kind` apareceu **depois** de o teste ter sido escrito (o teste original esperava
  `cookie` no caso do token solto); os dois casos do teste foram atualizados junto com o contrato.

### Texto de aviso do diálogo (como ficou)

Parágrafo 1 (inalterado): _"This cookie is the account's whole session: anyone holding it is signed
in as that account, with no password and no 2-step verification. Treat it like the account itself."_

Parágrafo 2 (**novo**, âmbar, ao lado do primeiro): _"A username:password:cookie line also saves the
password, which is more than the session: it can change the account's email and password, and
signing out of every session does not revoke it. It is stored in AccountData.json, encrypted only if
you set an app password."_

Em pt-BR: _"Uma linha username:password:cookie também guarda a senha, que é mais que a sessão: ela
troca o e-mail e a senha da conta, e sair de todas as sessões não a revoga. Fica gravada no
AccountData.json, encriptada só se você puser uma senha no app."_

A dica de cima virou _"Paste one .ROBLOSECURITY cookie per line, or one username:password:cookie per
line."_, e a aba User:Pass ganhou _"A line that already carries the cookie
(username:password:cookie) is imported straight away, with no browser."_

### Teste que falhou primeiro

1. `src/utils/cookies.test.ts` escrito antes do módulo → `Failed to resolve import "./cookies"`,
   0 testes rodados.
2. Depois do módulo: 14/14 passando.

### Sabotagem (o split ingênuo de volta)

Troquei o corte por `line.split(":")` com `parts[2]` como cookie e `[\s:;,]+$` no prefixo:

```
× keeps the whole cookie and leaves the credentials empty
× splits on the delimiter without cutting the cookie's own colon
× keeps a password that has colons of its own
× strips only the delimiter, not punctuation the password ends with
… (8 falhas no parser)
× validates and adds one account per pasted line
× imports username:password:cookie without cutting the cookie in half
× skips a line that has only username:password, instead of importing half a credential
Tests  11 failed | 10 passed (21)
```

Desfeita; 21/21 passando de novo.

---

## Task 12 — servidores recentes (job ids)

Origem: `niccsprojects@b3e3eb4` (`RecentJobsList.tsx`, `RecentJobsPopover.tsx`). Commit: ver
`git log` (segundo commit desta branch).

### O que mudou, por arquivo

| Arquivo | Mudança |
|---|---|
| `src/components/server-list/types.ts` | `RecentJobKind`, `RecentJobEntry`, `classifyJobInput`, `loadRecentJobs`, `saveRecentJobs`, `addRecentJob`, `visibleRecentJobs`, `removeRecentJob`; `localStorage["ram_recent_jobs"]`. |
| `src/components/server-list/types.test.ts` | 19 testes novos (classificação, round-trip, dedupe/limite, donos, visibilidade, remoção). |
| `src/components/server-list/RecentJobsList.tsx` (novo) | Lista com rótulo `Job`/`VIP`/`Link`, alvo em mono, place, tempo, remover e "Clear all". |
| `src/components/server-list/RecentJobsList.test.tsx` (novo) | 5 testes, inclusive o de a tela **não** mostrar alvo privado de outra conta. |
| `src/components/server-list/RecentTab.tsx` | Duas colunas (Games | Servers) quando `onSelectJob` é passado; sem o callback, renderiza exatamente o que renderizava antes. |
| `src/components/server-list/ServerListDialog.tsx` | `handleSelectJob` preenche Place + Job ID (reaproveitando o `jobIdPrefill`/`nonce` que já existia) e volta para a aba Servers; lê `MaxRecentJobs`. |
| `src/store.tsx` | `joinServer` e `launchMultiple` chamam `addRecentJob` depois do launch bem-sucedido, junto do `recordRecentGame`. |
| `src/store.test.ts` | Mock de `addRecentJob` + 5 testes (cap configurado, default 12, alvo VIP virando `vip:<código>`, sem job não grava, lote registra todas as contas). |
| `src-tauri/src/data/settings/store.rs` | Default `General.MaxRecentJobs = 12` em `apply_defaults` e no espelho `documented_defaults()`. |
| `src/components/settings/GeneralTab.tsx` | Campo "Max Recent Servers" (1–50) com descrição. |
| `src/dev/harness/scenarios.ts` | `MaxRecentJobs: "12"` nos settings do harness. |
| `docs/features/server-list.md`, `docs/features/settings.md` | Seção nova "Servidores recentes (Job IDs)", regras de negócio, config e armadilha do link em texto puro. |
| `src/locales/{en,pt}/common.json` | 5 chaves novas (+ pt). |

### Job id de servidor privado — o que decidi e por quê

**Guardo o alvo privado, mas com dono, e ele não aparece para outra conta.**

1. **O `raw` é guardado como o launch o usou** — `vip:<código>` ou o link — e nunca reescrito:
   é o `resolve_launch_job` do backend que entende esses formatos
   (`docs/features/join-links.md`). Normalizar aqui quebraria o significado. Como num alvo VIP o
   Job ID vai **vazio** e o código viaja em `linkCode`, a store reconstrói o `vip:<código>` antes
   de gravar — guardar o Job ID cru perderia o servidor, que era justamente o ponto da tarefa.
2. **Cada entrada carrega `userIds`**, as contas que já entraram por ela (um launch em lote
   registra todas), e `visibleRecentJobs(entries, userId)` mostra:
   - `kind: "job"` (Job ID **público**) para **qualquer** conta — é o mesmo servidor que a aba
     Servers lista para todo mundo, não há nada a vazar;
   - `kind: "vip"` / `"link"` **só** para as contas que já usaram aquele alvo. Sem conta
     selecionada, nenhum privado aparece.

   O motivo de não fazer como o upstream (lista única, global, para qualquer conta): um link VIP
   vale para quem o tem. Deixá-lo aparecer no painel de outra conta entregaria o servidor privado
   de uma conta a outra **sem o dono pedir**, que é exatamente o que o pedido proibiu. Não fui pela
   saída conservadora ("guardar só job id público") porque ela jogaria fora o caso mais útil (voltar
   ao servidor privado) sem necessidade: o dono do alvo é conhecido no momento do launch.
3. Limite: **um** cap global (`General.MaxRecentJobs`, default 12), mesmo desenho do de jogos, e não
   um cap por jogo — cada entrada guarda o `placeId` que a tela mostra.
4. "Clear all" apaga só o que **aquela conta vê**, senão limparia entradas privadas de outra conta
   sem que ninguém as tivesse visto.
5. A coluna só aparece onde há campo de Job ID para preencher (hoje o Server List). A aba Recent da
   Choose Game não passa `onSelectJob` e continua só com os jogos — a tela não tem campo de Job ID,
   e ação sem destino é pior que ação ausente ("Ação sem callback não aparece", regra já registrada
   em `docs/features/server-list.md`).
6. Fica registrado na documentação que o código do link mora em texto puro no `localStorage` do
   WebView — como os favoritos VIP já moravam. A regra de visibilidade evita mostrar o link a quem
   não o usou; ela **não** é proteção contra quem abre o perfil do WebView.

### Testes que falharam primeiro

1. 19 testes em `types.test.ts` antes do código → `TypeError: classifyJobInput is not a function`
   (e os demais símbolos), `19 failed | 34 passed`.
2. 4 testes em `store.test.ts` antes de a store gravar:
   ```
   × records the job id as a recent server, with the configured cap
   × defaults the recent-servers cap to 12
   × records a VIP target as vip:<code>, not as an empty job
   × records the recent server for every account that launched
   Tests  4 failed | 136 passed (140)
   ```

### Sabotagens

1. **Duplicar e estourar o limite** (`addRecentJob` sem o filtro de `raw` e sem o `slice`):
   ```
   × moves a job it already has to the top without duplicating it
   × drops the oldest entry when the list passes the limit
   Tests  2 failed | 51 passed (53)
   ```
   Desfeita; 53/53.
2. **Default novo fora do `apply_defaults`** (só no espelho documentado):
   ```
   test ...::every_documented_default_is_applied_on_a_fresh_install ... FAILED
   panicked: missing defaults: ["General.MaxRecentJobs"]
   ```
   Desfeita; 21/21 na `settings_store_tests`.

---

## `bun run check`

Rodado inteiro antes de cada commit. Saída resumida da última rodada:

```
$ tsc --noEmit                      (sem saída)
$ bun scripts/test-suite.ts --audit
Auditoria ok: toda suíte de teste está mapeada em scripts/test-suites.ts
$ vitest run
 Test Files  70 passed (70)
      Tests  1419 passed (1419)
$ cargo test --all-features
test result: ok. 1354 passed; 0 failed
test result: ok. 24 passed; 0 failed   (security_regression_scripts_store)
```

Nenhuma falha — nem as 4 `platform::windows::singleton_event_tests` conhecidas como instáveis
nesta máquina apareceram nestas rodadas.

## Conferida no harness (`bun run dev:ui`)

A aba Recent do Server List foi aberta no harness com `ram_recent_games` e `ram_recent_jobs`
semeados (um job público, um `vip:`, um link privado):

- as duas colunas (**GAMES** | **SERVERS**) cabem no diálogo de 680×560, sem estouro, com rótulo
  `Job`/`VIP`/`Link`, o place e o tempo;
- clicar no `vip:abcdef1234567890` preencheu **PLACE ID 606849621** e **JOB ID
  `vip:abcdef1234567890`** na aba Servers, sem lançar nada — o `vip:` chegou intacto;
- a aba Recent da Choose Game continua mostrando só os jogos, como projetado.

⚠️ Detalhe que atrapalhou e vale registrar: o `preview_start` com a config `ui-harness` do
`.claude/launch.json` subiu o Vite servindo o **checkout principal**, não este worktree (o
`RecentJobsList.tsx` voltava como fallback do SPA). Para validar o worktree foi preciso rodar
`UI_HARNESS=1 bun x vite --port 1421` de dentro dele.

Suítes: nenhum arquivo novo precisou entrar em `scripts/test-suites.ts` — `src/utils` já está
mapeado na suíte `ui` e `src/components/server-list` nas suítes `servers` e `ui`. A auditoria
confirma (nenhum teste órfão).

---

# Rodada de correção 1 — achados da revisão

Em cima de `a05bef9` (já integrado na `develop` pelo merge `ebb98c1`). Edições cirúrgicas, para
o merge seguinte ser trivial.

## Important 1 — `addRecentJob` derrubava launch que deu certo

`src/store.tsx` (`joinServer` e `launchMultiple`). A chamada estava **nua** entre o
`invoke("launch_roblox"/"launch_multiple")` bem-sucedido e o `catch` do launch: uma escrita
recusada pelo `localStorage` (cota cheia, perfil sem storage) virava `setError` +
`"Launch failed: …"` com o cliente do Roblox **já aberto**, e no lote o `catch` ainda **relança**,
interrompendo o que a Choose Game faria depois. O `recordRecentGame` da linha de cima já estava
blindado; eu tinha blindado um e deixado o outro.

Como `addRecentJob` é **síncrono**, `.catch()` não serve: os dois call sites agora são
`try { addRecentJob(...) } catch {}`, com o comentário explicando por quê.

**Testes que falharam primeiro** (`src/store.test.ts`, com `addRecentJobMock` lançando):

```
× keeps the launch successful when recording the recent server throws
× does not fail or rethrow when recording the recent server throws
Tests  2 failed | 140 passed (142)
```

O primeiro cobra `error === null`, tom ≠ `error` e a conta ainda em `joiningAccounts`; o segundo
cobra que `launchMultiple` **resolve** em vez de relançar. O `beforeEach` passou a usar
`addRecentJobMock.mockReset()` (e não `mockClear`), senão a implementação que explode vazaria para
o teste seguinte.

**Sabotagem:** removi os dois `try/catch` → exatamente esses 2 testes reprovaram. Desfeita.

## Minor 4 — share link duplo-codificado escapava da filtragem por dono

`classifyJobInput` (`src/components/server-list/types.ts`) olhava só o texto cru, então
`https://ro.blox.com/Ebh5?af_dp=…%3Fcode%3DDEADBEEF%26type%3DServer` — formato real, o mesmo do
teste `extract_query_param_value_recursive_handles_double_encoded_urls` em `launch_shared.rs` —
caía em `job` e **aparecia para todas as contas**, com o código privado do dono na linha e no
`title`. Agora o padrão é procurado também no texto decodificado (`decodeLayers`, até 3 passadas,
parando quando não muda), e escape quebrado não lança: o `decodeURIComponent` vai num `try` e vale
o que já se decodificou.

**Testes que falharam primeiro** (`types.test.ts`):

```
× sees through a double-encoded share link
```
(mais `survives a broken percent-escape instead of throwing`, que já passava e trava a guarda).

**Sabotagem:** voltei a testar só o texto cru → esse teste reprovou. Desfeita.

## Minor 2 — o comentário dizia o contrário do código

O comportamento fica como está (a revisão julgou a normalização **melhor**); corrigi o **texto**.
`RecentJobEntry.raw` agora diz que o valor é o alvo "no vocabulário que o campo de Job ID aceita de
volta", que a store **normaliza** link privado para `vip:<código>` antes de gravar, por quê
(no VIP o Job ID vai vazio e o código viaja em `linkCode`), e que o `resolve_launch_job` chega ao
mesmo `link_code` pelos dois caminhos — sendo a forma normalizada a que ainda classifica como
privada, o lado seguro do erro. O que não se faz é reescrever o valor **dentro do módulo**.
O comentário de `RecentJobKind` passou a dizer que, por causa disso, `kind: "link"` quase não é
alcançado pela store (cobre o já gravado e quem grave o link cru) — e que é justamente por isso que
`classifyJobInput` continua tendo de reconhecer link, inclusive o duplo-codificado.
`docs/features/server-list.md` foi corrigida nos mesmos termos (a frase "Nada é reescrito nem
normalizado" era falsa).

## Minors opcionais que fiz

- **`COOKIE_MARKER` exportado** (`utils/cookies.ts`): `AddAccountDialog.tsx` e `Toolbar.tsx` deixaram
  de repetir o literal `"_|WARNING:-DO-NOT-SHARE"`. Efeito colateral consciente: os dois passam a
  aceitar como cookie um valor que comece por `_|WARNING` com o aviso torto — é a mesma regra que o
  parser de import já documenta ("cookie com o aviso torto ainda é cookie"), e nome de usuário do
  Roblox não contém `_|`. Coberto pelos testes de cookie que já existiam nas duas telas.
- **Mensagem de conta repetida com senha na linha**: conta que já existe não passa pelo
  `add_account`, então a senha **não** é gravada. O resultado agora diz
  `"{{name}} - already exists; the password in this line was not stored"` (antes só
  "already exists", que deixava pensar que a senha tinha sido guardada). Teste
  `says the password was not stored when the account already exists`; sabotado (voltando à
  mensagem curta) e confirmado que reprova.
  ⚠️ A primeira redação usava "not saved" e **reprovou** `locales.test.ts`: `"saved"` é marcador de
  **sucesso** em `toneFromMessage`, então o inglês virava toast verde e o pt-BR, neutro — o teste de
  tom pegou. Trocado por "not stored", que não aciona marcador nenhum. Ficou registrado na doc.

## Minors que **não** mexi (conforme combinado)

Dono presumido no lote sem retorno por conta do backend; cap global compartilhado entre contas;
senha contendo literalmente `_|WARNING` caindo no fallback.

## `bun run check` desta rodada

```
$ tsc --noEmit                      (sem saída)
$ bun scripts/test-suite.ts --audit
Auditoria ok: toda suíte de teste está mapeada em scripts/test-suites.ts
$ vitest run
 Test Files  70 passed (70)
      Tests  1424 passed (1424)
$ cargo test --all-features
test result: ok. 1354 passed; 0 failed
test result: ok. 24 passed; 0 failed   (security_regression_scripts_store)
```

Verde inteiro — as 4 `singleton_event_tests` instáveis também passaram.
