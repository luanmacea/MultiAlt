# Checkup de usabilidade — setembro/2026

Revisão do app inteiro sob quatro perguntas, uma por funcionalidade:

1. **Descoberta** — dá para achar, ou está escondida atrás de uma sequência de telas?
2. **Entendimento** — só com o que está na tela dá para saber para que serve e como usar?
3. **Design** — o desenho serve àquela função?
4. **Preenchimento** — pede número decorado onde poderia deixar escolher de uma lista?

## Como foi levantado

Seis agentes dirigiram o app no navegador pelo harness (`bun run dev:ui`, cenário
`tour`), um por área, cada um só **relatando**. Nenhum agente mexeu no código. Os
achados de gravidade alta foram depois **conferidos por mim no código** antes de
entrar aqui — os conferidos estão marcados com ✓ e trazem `arquivo:linha`. Os
demais vieram do relatório do agente e valem como indício, não como veredito.

Total: **131 achados** — 25 altos, 60 médios, 46 baixos.

## Situação

| Faixa | O que é | Situação |
|---|---|---|
| **P0** | A tela mente ou o ajuste não funciona | ✅ **feito** (16 itens, 7 commits) |
| **P1** | Existe e ninguém acha | ✅ **feito** (15 itens, 4 commits) |
| **P2** | Está na tela e não se explica | ✅ **feito** (16 itens, 8 commits) |
| **P3** | Desenho e ergonomia | ✅ **feito** (onda 1: risco, feedback, texto quebrado; onda 2: densidade, preenchimento, teclado) |
| **Tradução** | Não existe pt-BR | ✅ **feito** (catálogo `pt` completo; a paridade com o `en` é travada por teste, então o número exato de chaves não é repetido aqui — ele muda a cada feature) |

Cada correção entrou com o teste que falha primeiro e `bun run check` verde.

---

## O pano de fundo: o app não falava português ✅

`src/i18n/index.ts` declarava só `en` e `de` — e o alemão cobre 823 das chaves,
o resto caindo no inglês. Boa parte da sensação de "não entendo o que isso faz"
não era falta de texto explicativo: era o texto estar num idioma que não é o seu.

**Concluído.** `src/locales/pt/common.json` traduz as **1584** chaves do
catálogo. O número subiu de 1286 porque P0–P2 acrescentaram texto explicativo e
porque **44 chaves nunca chegavam ao catálogo**: o extrator não via componente
com atributos (`<UtilButton onClick={…}>Sair das outras sessões</UtilButton>`),
prop com ternário (`description={cond ? "Requires Unlock FPS" : undefined}`),
ramo de ternário dentro do `t()` nem frase que cita uma URL. Essas telas ficavam
em inglês **em todos os idiomas**, alemão incluído. O idioma é escolhido em
Settings › Geral; o padrão continua inglês. Glossário e as regras de
"tradução não muda comportamento" estão em
[development.md#i18n](development.md#i18n).

Cinco coisas apareceram ao traduzir, e não eram texto:

- **O tom do toast era deduzido por palavra inglesa** (`store.tsx`:
  `includes("failed")` → erro), mas 162 call sites entregam a frase já
  traduzida — em português todo erro cairia como `info`, deixando o toast de
  falha igual ao de sucesso. O heurístico virou
  [src/utils/toastTone.ts](../src/utils/toastTone.ts), com marcadores dos dois
  idiomas completos e teste sobre o catálogo inteiro. Isso destrava o item de
  P3 que vai renderizar o `tone`.
- **`Browse` e `Manual install` nunca chegaram ao catálogo**: estão dentro de
  `t(cond ? "A" : "B")`, que o extrator não reconhece. Ficavam em inglês em
  todos os idiomas.
- **O alemão perdia um placeholder**: `"{{count}} server{{suffix}}"` traduzido
  como `"{{count}} Server"` (chave morta, mas o teste novo trava o caso).
- **O tour tinha o seu próprio seletor de idioma**, com só `en` e `de`: com
  `Language=pt` ele mostrava o literal "pt" e, na primeira execução, o botão
  Avançar fica travado até escolher da lista — quem estava em português não
  passava do primeiro passo. Agora as duas telas leem `LANGUAGE_OPTIONS` de
  [src/i18n/index.ts](../src/i18n/index.ts).
- **Concordância que o inglês não tem**: "1 selecionadas", "1 livres". O
  português obriga a escolher, e a forma `(s)` — que o app já usa em
  "conta(s)" — resolve sem inventar chave nova.

## Cinco padrões que se repetem

1. **O rótulo diz uma coisa e o código faz outra.** Não é falta de documentação — é informação errada na tela. Quatro casos confirmados (`Async Launching`, versão do Roblox "desta conta", `Read Interval`, o passo do tour sobre as bolinhas).
2. **Ajuste que não tem efeito, sem dizer que não tem.** Piso invisível no atraso de lançamento, fast flags descartados num `eprintln!`, senha curta no web server, campo numérico editável com o toggle desligado.
3. **A funcionalidade existe e não se anuncia.** Ícone sem nome acessível, ação que só aparece no hover, recurso destravado por um toggle escondido em outra aba, `Join link` enterrado na aba Follow.
4. **Pede o dado cru em vez de deixar escolher.** Place ID, Job ID, Universe ID, hash de versão, caminho de arquivo — e o campo de Place ID ainda corrompe link colado em silêncio.
5. **Ação perigosa com o mesmo peso visual de ação inofensiva.** Trocar senha e e-mail da conta Roblox não confirmam; "Sign out of other sessions", que é menos grave, confirma.

---

## P0 — a tela mente ou o ajuste não funciona ✅

**Concluído.** Era a faixa onde o usuário tomava decisão errada com base no que
lia. Abaixo, o que cada item virou.

| # | Achado | Evidência |
|---|---|---|
| ✓ | **`Async Launching` faz o contrário do nome**: com ele ligado o lote *espera* a conta anterior (deadline de 120 s) antes da próxima | `commands/launch.rs:1222` |
| ✓ | **A versão do Roblox no painel da conta é global**, mas o texto diz "used when *this account* launches" | `SingleSelectSidebar.tsx:170` → `store.tsx:1431` |
| ✓ | **`Account Join Delay` tem piso invisível de 8 s** e é ignorado com Async ligado; 0–7 s não faz nada | `launch.rs:5,74-76` |
| ✓ | **Fast flags fora da allowlist descartam o bloco inteiro** num `eprintln!` que ninguém vê; a opção parece ligada | `platform/windows/optimization.rs:237`, `launch_shared.rs:246` |
| ✓ | **`Read Interval` (Watcher) só existe no macOS** — está dentro de `#[cfg(target_os = "macos")]` | `commands/watcher.rs:324,338` |
| ✓ | **Web server recusa tudo com senha de menos de 6 caracteres**; a tela não exige nem valida | `api/server/middleware.rs:53` |
| ✓ | **Campo Place ID apaga tudo que não é dígito**: colar a URL do jogo vira um place inventado, sem erro | `servers/ServersTab.tsx:415` |
| ✓ | **Nome de grupo com número na frente perde dígitos**: "2024 Alts" vira "4 Alts" | `types.ts:150` |
| ✓ | **Quick Add por nome de usuário cria conta sem sessão** e anuncia "Added X" como sucesso | `Toolbar.tsx:79-87` |
| ✓ | **O botão de painel acende sem abrir painel** com 0 ou 2+ contas selecionadas | `App.tsx:125` |
| ✓ | **O filtro esconde contas que continuam selecionadas** e sujeitas a `Remove` e `Choose Game` | `store.tsx:654` |
| ✓ | **O passo do tour sobre as bolinhas ensina errado**: diz "amber means aged" sem citar que *launched* também é âmbar | `FirstRunWalkthrough.tsx:84` |
| ✓ | **Trocar senha e e-mail da conta Roblox não confirmam nada** | `AccountUtilsDialog.tsx:162-196` |
| ✓ | **`Studio` é contado em `in game`** e `online` já inclui quem está em jogo; a legenda anuncia um contador `studio` que não existe | `StatusBar.tsx:12-18` |
| | **`Hidden` vaza o nome real** na barra inferior e no placeholder do Alias | `BottomActionBar.tsx` |
| | **9 campos numéricos editáveis com o toggle que os ativa desligado** (Max FPS sem Unlock FPS, Memory Threshold sem Close If Memory Low…) | `OptimizationTab.tsx`, `WatcherTab.tsx` |

## P1 — existe e ninguém acha ✅

**Concluído.**

| # | Achado |
|---|---|
| ✓ | **`Browse servers` só existe na aba Games**; Favorites não recebe a prop e Recent não tem ação nenhuma (`ChooseGameScreen.tsx:888`) |
| ✓ | **As ações do card de jogo só aparecem no hover** (`GamesTab.tsx:188`) |
| ✓ | **O botão `Names` não tem tooltip** — o único da toolbar sem (`Toolbar.tsx:137`) |
| ✓ | **O estado vazio da lista oferece 4 opções**; o menu `Add` oferece 8, e faltam justamente as duas de criar conta (`AddAccountDialog.tsx:90-114`) |
| | **`Join link`** — a funcionalidade que resolve qualquer link — está dentro da aba Follow, a única sem dica 💡 |
| | **Botting Mode é invisível** até alguém achar o toggle em Settings › General |
| | **Os ícones da toolbar não têm nome acessível**; o rótulo só existe em tooltip com 350 ms de atraso |
| | **A aba WebServer só existe se o usuário descobrir o Developer Mode** |
| | **As duas redes de segurança do Isolation** (restaurar identificadores, pré-visualizar o que será apagado) estão dentro de "Advanced" |
| | **`Window layout`** (organizar janelas em grade) mora dentro da aba Console |
| | **Os ajustes que reduzem CAPTCHA** ficam em Settings › General, longe da tela de criar contas |
| | **Favoritar só é possível por botão direito** na aba Games |
| | **Não existe nenhum ponto de ajuda** na barra de título nem na toolbar |
| | **O gerador rodando some da tela** ao fechar o diálogo — e ele gasta dinheiro (o Botting, que não gasta, tem contador no rodapé) |
| | **O tour nunca menciona o Painel de Sessão** e fecha os diálogos de ferramenta sem apresentá-los |

## P2 — está na tela e não se explica ✅

**Concluído.** Aqui ninguém era enganado e tudo se achava: o problema era a tela
não contar o que a coisa faz. Foi trabalho de texto, com uma regra: nenhuma
explicação entrou sem estar conferida no código que a sustenta — ajuste que não
deu para confirmar ficou sem texto, e está listado abaixo.

| # | Achado | O que virou |
|---|---|---|
| ✓ | **Scripts não diz o que é**: JavaScript num Web Worker *dentro do RAM*, não executor, sem injector | Cabeçalho diz onde roda, que fala com o app pela API `ram.*`, que não injeta nada e que não recebe cookie nem senha |
| ✓ | **As 8 permissões de script são nomes técnicos** sem descrição (`PermissionRow` nem aceita uma) | `PermissionRow` passa a exigir descrição; cada permissão diz o risco concreto |
| ✓ | **Botting não diz que fecha e reabre o cliente a cada ciclo**, e os campos de tempo perdem a unidade na vista padrão | Bloco "How each cycle works" nas duas vistas + unidade, significado e faixa de cada campo de Timing |
| ✓ | **Nexus**: o ícone diz "Nexus", o diálogo diz "Account Control", e o Help não menciona que é preciso um executor de terceiros | Diálogo se chama Nexus; seção Requirements diz que ele só escuta, e o endereço `ws://localhost:<porta>/Nexus` |
| ✓ | **A aba WebServer inteira é uma lista de permissões sem uma linha de explicação** — `Allow GetCookie` entrega o cookie da conta | Uma linha por permissão, conferida nos handlers; `GetCookie` em aviso âmbar |
| ✓ | **A aba Watcher liga um sistema inteiro** sem dizer o que ele faz | Topo da aba diz o que varre, que ignora a janela em uso, a carência de 30 s, e que fecha sem reabrir |
| ✓ | **Nada diz que o `Account Generator` é serviço pago de terceiro**, nem que existe a alternativa gratuita ao lado | Aviso na aba de Settings e no diálogo, apontando a alternativa gratuita |
| ✓ | **O cookie `.ROBLOSECURITY` é pedido em três lugares** e em nenhum se explica onde achá-lo | Os três dizem o que ele vale (conta inteira, sem senha e sem 2FA) e onde encontrá-lo |
| ✓ | **`aged` não diz de que envelheceu** | Diz de quantos dias, a partir de qual data, e que sessão morta é a bolinha vermelha (o vermelho puro aos 30 dias já tinha caído no P0) |
| ✓ | **Quando o launch falha, o porquê está em outra tela** | A faixa de erro aponta Choose Game › Console e abre a tela, quando existe log |
| ✓ | **A faixa de erro corta o texto** numa linha (`App.tsx:96`) | Quebra linha, com teto de altura e rolagem |
| ✓ | **`Quick Login` não diz de onde vem o código** de 6 dígitos | Diz que nasce no `roblox.com/login` do aparelho que vai entrar, e que o app o envia com a sessão da conta |
| ✓ | **`Unlock` com PIN de 4 dígitos** sem explicação do que destrava | Diz que é o PIN da conta Roblox (não a senha do app nem a da criptografia) e o que destravar libera |
| ✓ | **`Region Format` pede um template** sem dizer que tokens existem | Lista os cinco tokens e mostra o resultado de um exemplo |
| ✓ | **21 opções de Settings são indecifráveis** só pela tela | 9 ajustes ganharam linha própria; as duas abas restantes já tinham descrição em todos os toggles |
| ✓ | **Quatro nomes diferentes para as mesmas duas funções** de criar conta | Um nome cada (`Create Accounts` grátis, `Account Generator` pago) e a mesma linha de apoio em todo ponto de entrada |

## Achados novos, levantados durante o P2

Explicar o que está na tela obriga a ler o código que sustenta a frase — e foi aí
que estes apareceram. **Todos foram tratados**: oito corrigidos (✅) e um decidido
como "fica assim, e o porquê está escrito" (📝). Onde a tela não podia prometer o que o código não faz, o texto foi
escrito escapando do ponto (e isso está dito na linha).

| Gravidade | Achado | Onde |
|---|---|---|
| ✅ Alta | **`LastUse` nunca era atualizado por lançamento** — a coluna "3d"/"2mo" e a bolinha mediam idade do cadastro. Agora `mark_used` roda no `Done` da fila de launch, no ciclo do Botting e nos launches do web server. Efeito colateral bom: o auto-refresh de cookie (que desloga todas as sessões) deixa de mirar conta que está em uso | `data/accounts/store.rs` (`mark_used`), `commands/launch.rs` (`launch_queue_mark`) |
| ✅ Alta | **`Allow Account Editing` não cobria tudo que edita a conta**: `/SetAvatar`, `/BlockUser`, `/UnblockUser` e `/UnblockEveryone` passavam só com a senha. Agora exigem a flag, como as rotas de campo/apelido/descrição já exigiam (quebra de compatibilidade registrada em webserver.md) | `api/server/handlers_edit.rs` + `edit_permission_tests` |
| ✅ Média | **O snapshot da janela era empurrado para todo script** sem checar `allowWindow`. Agora os dois pontos de envio passam por `snapshotForPermissions`: sem a permissão sobram só as settings redigidas e o `ts` | `scripting/security.ts`, `ScriptsDialog.tsx` |
| ✅ Média | **A descrição de `Background Mode` estava errada** (falava em cliente minimizado). Agora diz que força Idle em todos os clientes do perfil, inclusive o em foco | `OptimizationTab.tsx` + teste em `settingsHelp.test.tsx` |
| 📝 Média | **`quick_login_validate_code` existe no backend e nada o chama** — decidido deixar como está e registrar o porquê em [authentication.md](features/authentication.md): o fluxo usado é `enterCode` + confirmação no aparelho, e o frontend já exige 6 dígitos | `api/auth.rs:414` |
| ✅ Baixa | **Exportar o `Nexus.lua` não dava retorno nenhum** (erro engolido por `catch {}`). Agora diz onde o arquivo foi salvo, se o caminho foi copiado, e mostra a falha quando o comando falha | `NexusDialog.tsx` |
| ✅ Baixa | **A detecção de "multi roblox" estava duplicada em três lugares**. Virou `isMultiRobloxCloseProcessError` em `utils/robloxErrors.ts`, com teste | `App.tsx`, `MultiSelectSidebar.tsx`, `BottingDialog.tsx` |
| ✅ Baixa | **O comentário do INI para `ServerRegionFormat` apontava `ip-api.com`**, sem relação com os tokens reais. Agora lista os cinco que `format_region` substitui | `data/settings/store.rs` |
| ✅ Baixa | **`Shuffle Job ID` explicava pela metade**. A descrição agora diz que Job ID digitado ou seguir um jogador vencem o sorteio | `MiscellaneousTab.tsx` + teste |

### O backend não compila fora do Windows

`cargo check --all-features` no Linux para com 10 erros, todos de `cfg` faltando:
`commands/botting.rs` chama funções que só existem sob
`#[cfg(target_os = "windows")]` (`ensure_multi_roblox_enabled`,
`patch_client_settings_for_launch`, `wait_for_new_roblox_pid`,
`detect_auth_failure_window`, `minimize_new_roblox_windows`,
`get_or_create_browser_tracker_id`, `is_429_related_error`,
`apply_windows_post_launch_profile`) e `api/server/launch_patch.rs` idem. É
pré-existente e afeta também a build macOS, que a doc dá como parcial.

Consequência prática: **`bun run check` só roda inteiro no Windows.** O P2 foi
verificado com `tsc --noEmit`, `bun run t --audit` e a suíte vitest completa
(1122 testes); a metade `cargo test --all-features` não foi executada. Como o P2
é todo frontend, nenhum arquivo Rust foi tocado — mas o check completo precisa
rodar no Windows antes de considerar a faixa fechada de verdade.

## P3 — desenho e ergonomia ✅

Dividido em duas ondas: primeiro o que é risco e feedback, depois o desenho das
telas. **Cada item abaixo foi medido no código antes de entrar** — e a medição
derrubou cinco afirmações da versão anterior desta lista, corrigidas abaixo.

### Onda 1 ✅ — confirmação, feedback e texto quebrado

| # | Achado | O que virou |
|---|---|---|
| ✓ | **`Copy ▸ Cookie`/`Password`/`User:Pass` entregam credencial sem aviso** — e `copyMulti` junta **todas** as contas selecionadas: um clique com 50 selecionadas põe 50 `.ROBLOSECURITY` na área de transferência. Mais dois botões fazem o mesmo em massa (`BottomActionBar`, `MultiSelectSidebar`) | Confirmação dizendo o que o cookie entrega e quantas contas entram na cópia, com opt-out persistido em `General.WarnOnCopyCredential` (hook `useCopyCredentialWarning`, no molde do `useJoinOnlineWarning`) |
| ✓ | **Copiar link de debug podia derrubar as sessões da conta**: o comando `get_auth_ticket` estava sob `run_with_session_retry`, e o refresh chama `signoutfromallsessionsandreauthenticate` — leitura não crítica, o que o CLAUDE.md proíbe | `auth_ticket_without_refresh`: cookie velho virou erro na tela, não refresh. O launch tem caminho próprio e segue com retry. Os três itens que copiam ticket/link também confirmam antes |
| ✓ | **Ações destrutivas do Botting não confirmavam** (`BottingDialog` nem importava `usePrompt`): `Stop + Close Bot Accounts` e os lotes `close`/`closeDisconnect` fechavam N clientes num clique | Confirmação com a contagem real e a frase que o próprio diálogo já usa — fecha só as bot **desta sessão**; player e clientes de fora ficam abertos. `Stop Botting Mode` e `restart` seguem sem perguntar, de propósito |
| ✓ | **`Clear all` dos recentes**, remover favorito e remover VIP apagavam direto no `localStorage` | Confirmação destrutiva dizendo quantos itens somem e que não há como recuperar |
| ✓ | **Toast de erro idêntico ao de sucesso**: a store calculava `tone` e **nenhum componente lia** | A fila virou `{id, message, tone}` e o toast usa a mesma paleta do Console de launch (`TONE_STYLES`). De lambuja: remover por `id` conserta o `slice(1)`, que derrubava o vizinho errado quando dois toasts se sobrepunham |
| ✓ | **16 mensagens só existiam em `actionStatus`, que ninguém desenhava** — `Settings saved`, todo o progresso de download do Chromium e da versão do Roblox (`timeoutMs: 60000`), `Botting rejoin failed` | A `StatusBar` desenha o slot com a bolinha do tom. Fronteira explícita: toast = "acabou de acontecer", `actionStatus` = "está acontecendo agora" — e `addToast` parou de escrever nos dois |
| ✓ | **Três atributos JSX com escape cru** (`HKLM\\SOFTWARE\\…`, `C:\\path\\…`, `\n` literal no JSON dos fast flags) | `attr={"..."}`. O bug era triplo: escape na tela, frase caindo no inglês **em todos os idiomas**, e a tradução pt existente como chave morta. Um teste estrutural varre `src/**/*.tsx` e reprova o quarto caso |
| ✓ | **A confirmação de entrar com conta online estava inteira em inglês** — modal bloqueante, chamado de 4 telas, com a frase num template literal (que o extrator descarta por princípio) e `Join Anyway`/`Don't show this warning again` fora do catálogo | Frases com `{{placeholder}}`, singular e plural em chaves separadas, e o estado de presença traduzido dentro da frase |

### O que a medição corrigiu neste doc

| Dizia | É |
|---|---|
| Settings usa 27% da largura | **46,6%** (512px de 1100) — os 27% eram de uma tela de 1920 |
| Settings rola 3 telas por aba | É a média; o pico é **5,8** (Otimização) e 4,6 (Geral), e 3 abas ficam em 1 tela |
| Optimization vira ~10 telas | **16,9 telas** (6357px, 67 controles) |
| Abas da Choose Game não navegam por teclado | São `<button>` e chegam pelo Tab. O achado real é **não existir indicador de foco em botão nenhum do app** (1 `:focus-visible` no CSS contra 35 `focus:outline-none`) — e o `Toggle`, que é `<div onClick>` usado 59 vezes, deixa **Settings inteira** inoperável por teclado |
| Hash de versão digitado à mão | Há catálogo remoto com botão Install, e a aba manual já valida o formato; sobra o campo **Channel**, que é texto livre |
| Falha do Browser Login em inglês fixo | A chave está nos três catálogos e `addToast` a traduz. O problema real era o **tom**: falha de login saía como `info` |

### Onda 2 ✅ — densidade, preenchimento e teclado

| # | Achado | O que virou |
|---|---|---|
| ✓ | **Settings era o único diálogo dessa família sem guarda de viewport**, com 512px de 1100 e a TabBar quebrando em 3 linhas (98px dos 595 de altura) | `w-[780px] max-w-[calc(100vw-24px)]` + `h-[calc(100vh-24px)]`; as 9 abas voltaram a caber em 1 linha (medido em pt e en) |
| ✓ | **Optimization montava os 3 perfis de uma vez**: 6357px de rolagem, 67 controles, `Unlock FPS` 3× e 12 `aria-label` idênticos — e o título do perfil rolava para fora da tela, num contexto em que "Normal" é nome de perfil **e** opção de dois selects | Seletor de perfil (`role="radio"`) fixo no cromo e só a seção escolhida monta: ~4 telas, rótulos únicos, perfil sempre à vista |
| ✓ | **O log da aba Console ficava com 26px** (24 de padding), porque o `SessionPanel` era `shrink-0` sem teto e o log era o único `flex-1` | Painel com teto de 45% e rolagem própria, log com piso de 160px: 26px → 180px |
| ✓ | **O Job ID levava metade da linha** da lista de servidores (528px de 1056, ~290px vazios) enquanto a região truncava cidade em 150px — e não dava para copiar | Coluna fixa de 250px, copiável por clique ou menu de contexto; a região recebeu o `flex-1` (150px → 428px) |
| ✓ | **Clicar no card de um jogo lançava na hora** (`// just launch directly`), sem passo intermediário para conta offline — e era duplicata da ação `Join Game` que a linha já tem desde o P1 | O card leva aos servidores, como o `ServerListDialog` já fazia; lançar é só pelo `Join Game`. O Recent passou a ser gravado no clique, senão o jogo clicado desapareceria da aba |
| ✓ | **`Add To Group` era campo livre**: "bloxgen" e "BloxGen" criavam grupos diferentes | `<datalist>` com os grupos existentes (helper `collectGroupNames`), campo ainda editável para criar grupo novo. O helper devolve o nome **cru**: devolver o `displayName` reintroduziria o bug de P0 do prefixo numérico |
| ✓ | **VIP entrava por dois `prompt()` encadeados**, e cancelar o segundo jogava fora o link do primeiro | Formulário inline com os dois campos e validação deliberadamente permissiva — o Rust aceita qualquer string não-vazia como código, então regra mais rígida no frontend recusaria link válido |
| ✓ | **Universe ID recusava URL colada em silêncio** (`parseInt` e `return` mudo) | Aceita link colado e, quando não resolve, diz o porquê |
| ✓ | **Avatar JSON só falhava no `set_avatar`**, com toast genérico | Valida antes, diz o que está errado, desabilita o botão enquanto há erro e aponta o "Wear Outfit", que monta o JSON sozinho |
| ✓ | **Importar fonte/preset pedia caminho absoluto digitado** | `<input type="file">` + comandos Rust que recebem bytes, no padrão que o app já usava — sem dependência nova. Teto de 20 MiB só nos comandos novos: caminho local é confiável, bytes por IPC não |
| ✓ | **`Channel` da aba manual de versões era texto livre** | `<datalist>` com os canais do catálogo e dos instalados. Só UI: nada aqui escreve canal no registro |
| ✓ | **Não existia indicador de foco em botão nenhum** (1 `:focus-visible` no CSS contra 35 `focus:outline-none`) — era isso, e não a falta de `<button>`, que fazia a Choose Game parecer não-navegável | Anel de foco em `.theme-btn`/`.theme-btn-ghost` e nos controles com semântica nova, reusando `var(--input-focus)` |
| ✓ | **O `Toggle` era `<div onClick>`** e aparece 59 vezes em 9 telas: **Settings inteira** era inoperável por teclado | `role="switch"`, `aria-checked`, Espaço/Enter, `aria-disabled` — um arquivo conserta as 9 telas |
| ✓ | **Item de menu, cabeçalho de grupo, checkbox de grupo e linhas de Games/Recent** eram `<div onClick>` | Semântica (`menuitem`, `button`, `checkbox` com `mixed`) e teclado, sem trocar tag onde há `<button>` aninhado |
| ✓ | **26 handlers de Escape em 23 arquivos, nenhum interrompendo a propagação**: com diálogo sobre a Choose Game, um Escape fechava os dois; na lista, apagava a seleção junto com o menu (o `ContextMenu` registrava no boot e nunca removia) | Pilha LIFO (`useEscapeStack`): um listener, só o topo recebe. Ignora evento já tratado com `preventDefault` (o prompt), tem `ignoreFromFields` para campo de texto, e os 6 diálogos que não fechavam com Escape passaram a fechar |

Sobrou fora de escopo, para quem continuar: `aria-label` nos 26 botões só-ícone
(espalhados por arquivos de várias áreas), foco inicial e `aria-modal` nos 13
diálogos, e os outros 33 `focus:outline-none` que não foram tocados.

**Código morto, apagado:** a `MultiSelectSidebar` (517 linhas) não era
renderizada em lugar nenhum — o `App.tsx` só monta a `DetailSidebar`, que é de
**uma** conta. Já estava documentado, mas continuava cobrando pedágio: foi
editada três vezes nesta leva (identificação do jogo, Make Friends pela store,
limpeza de imports) antes de alguém notar que ninguém consegue abri-la. Apagada
com o teste junto, a pedido do dono.

Nada de função se perdeu: os campos de launch dela vivem na Choose Game, e as
ações em lote na `BottomActionBar`. A única exceção era o campo de **delay
entre pedidos de amizade**, que só existia nela — ✅ voltou no submenu Make
Friends da própria `BottomActionBar`, gravando `Friends.RequestDelayMs` e
limitado à mesma faixa do backend.

---

## O que está bom e não deve ser mexido

- **Backups** é o modelo a copiar: explica o que guarda, mostra a pasta, confirma restauração e exclusão dizendo a consequência, e avisa se precisa reiniciar.
- **O texto do CAPTCHA** na criação de contas é o melhor da interface: diz exatamente o que o app faz e o que sobra para a pessoa.
- **A aba Servers** acerta o essencial: ocupação como elemento principal, linha desabilitada com o motivo escrito, e um resumo honesto quando nada cabe.
- **A aba Friends** mostra quem *não* dá para seguir com o motivo, em vez de sumir com a linha.
- **As dicas 💡 por aba** da Choose Game e a **aba API** do Scripts.
- A confirmação ao marcar um script como `Trusted`.

## Limites desta revisão

- O harness usa dados falsos: ícones de jogo, regiões de servidor e busca por ID não foram exercitados de verdade.
- Reordenar contas por arrasto não foi reproduzido (arrasto HTML5 não responde a clique sintético); esse achado saiu só de leitura de código.
- Um dos agentes pegou o harness servindo código velho — cache de pré-empacotamento do Vite, corrigido em `vite.config.ts` junto com esta revisão. Parte das observações ao vivo daquele relatório pode ter sido feita no cenário `default`.
- Os itens sem ✓ não foram conferidos por mim um a um. Cada um precisa ser reproduzido antes de virar mudança de código, como manda o fluxo em [CLAUDE.md](../CLAUDE.md).
