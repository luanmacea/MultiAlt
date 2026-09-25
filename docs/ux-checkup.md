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
| **P3** | Desenho e ergonomia | ⬜ a fazer (~40 itens) |
| **Tradução** | Não existe pt-BR | ✅ **feito** (1535 chaves, catálogo `pt` completo) |

Cada correção entrou com o teste que falha primeiro e `bun run check` verde.

---

## O pano de fundo: o app não falava português ✅

`src/i18n/index.ts` declarava só `en` e `de` — e o alemão cobre 823 das chaves,
o resto caindo no inglês. Boa parte da sensação de "não entendo o que isso faz"
não era falta de texto explicativo: era o texto estar num idioma que não é o seu.

**Concluído.** `src/locales/pt/common.json` traduz as **1535** chaves do
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
que estes apareceram. **Nenhum foi corrigido**: são mudança de comportamento, não
de texto. Onde a tela não podia prometer o que o código não faz, o texto foi
escrito escapando do ponto (e isso está dito na linha).

| Gravidade | Achado | Onde |
|---|---|---|
| Alta | **`LastUse` nunca é atualizado por lançamento.** Só é escrito ao criar/re-adicionar a conta. A coluna "3d"/"2mo" e a bolinha de envelhecimento medem idade do **cadastro**, não inatividade de jogo | `data/accounts/model.rs:154,174`, `data/accounts/store.rs:289` |
| Alta | **`Allow Account Editing` não cobre tudo que edita a conta**: `/SetAvatar`, `/BlockUser`, `/UnblockUser` e `/UnblockEveryone` ficam liberados com ele desligado (pedem só a senha) | `api/server/handlers_edit.rs:301,337,372,456` |
| Média | **O snapshot da janela é empurrado para todo script** assim que ele sobe, sem checar `allowWindow` — a permissão só barra a leitura sob demanda | `ScriptsDialog.tsx:2387-2391` |
| Média | **A descrição de `Background Mode` está errada**: fala em cliente minimizado/fora da tela, mas o código força `IDLE_PRIORITY_CLASS` no processo sem olhar janela nenhuma | `platform/windows/optimization.rs:280` |
| Média | **`quick_login_validate_code` existe no backend e nada no frontend o chama** — o fluxo do Roblox normalmente é `enterCode` + confirmação | `api/auth.rs:414`, `commands/account_api.rs:1317` |
| Baixa | **Exportar o `Nexus.lua` não dá retorno nenhum**: sucesso só copia o caminho, erro é engolido por `catch {}`, e com a feature `nexus` desligada o clique não faz nada visível | `NexusDialog.tsx`, `commands/services.rs:297` |
| Baixa | **A detecção de "multi roblox" por substring em inglês está duplicada em três lugares** | `App.tsx`, `MultiSelectSidebar.tsx:63`, `BottingDialog.tsx:94` |
| Baixa | **O comentário gravado no INI para `ServerRegionFormat` aponta `ip-api.com`**, que não corresponde aos cinco tokens que o código substitui | `data/settings/store.rs:49` |
| Baixa | **`Shuffle Job ID` explica pela metade**: o sorteio é ignorado quando há Job ID digitado ou quando se usa "follow user" | `commands/launch.rs:112-114` |

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

## P3 — desenho e ergonomia ⬜

Itens de acabamento, agrupados por tema (lista completa nos relatórios da revisão):

- **Confirmação e reversibilidade**: `Copy ▸ Cookie`/`Password` entregam credencial sem aviso; ações destrutivas do Botting não confirmam; `Clear all` dos recentes é destrutivo e críptico; clicar num jogo lança na hora, sem passo intermediário.
- **Escala e densidade**: Settings usa 27% da largura e rola 3 telas por aba; com perfis de Botting separados, Optimization vira ~10 telas com `Unlock FPS` repetido 3× sem cabeçalho fixo; a aba Console dá 35 px ao log; a lista de servidores gasta a maior coluna com o Job ID, que nem dá para copiar.
- **Feedback**: toast de erro é idêntico ao de sucesso — a store já calcula `tone` em `actionStatus` e **nenhum componente renderiza isso** (`store.tsx:2360`), além de o tone ser detectado por substring em inglês.
- **Preenchimento**: `Add To Group` é campo livre sem lista dos grupos existentes; caminho de arquivo sem botão de procurar; hash de versão digitado à mão; VIP adicionado por dois `prompt()` encadeados; Universe ID e avatar JSON pedem dado bruto.
- **Teclado e acessibilidade**: abas e linhas da Choose Game não são navegáveis por teclado; `Esc` fecha a Choose Game por baixo do diálogo aberto e, na lista, apaga a seleção junto com o menu.
- **Texto quebrado**: três lugares mostram escape cru (`HKLM\\SOFTWARE\\…`, `\n` literal no placeholder do JSON); a falha do Browser Login está em inglês fixo, fora da tradução.

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
