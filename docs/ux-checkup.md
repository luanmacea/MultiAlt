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

Total: **131 achados** — 25 altos, 60 médios, 46 baixos. Nenhuma correção foi
feita nesta passada; isto é a lista de trabalho.

---

## O pano de fundo: o app não fala português

`src/i18n/index.ts:6` só declara `en` e `de` — 1286 chaves em inglês, 823 em
alemão, **nenhuma em português**. Boa parte da sensação de "não entendo o que
isso faz" não é falta de texto explicativo: é o texto estar num idioma que não é
o seu. Traduzir é o item de maior alcance desta lista, e o mais mecânico.

## Cinco padrões que se repetem

1. **O rótulo diz uma coisa e o código faz outra.** Não é falta de documentação — é informação errada na tela. Quatro casos confirmados (`Async Launching`, versão do Roblox "desta conta", `Read Interval`, o passo do tour sobre as bolinhas).
2. **Ajuste que não tem efeito, sem dizer que não tem.** Piso invisível no atraso de lançamento, fast flags descartados num `eprintln!`, senha curta no web server, campo numérico editável com o toggle desligado.
3. **A funcionalidade existe e não se anuncia.** Ícone sem nome acessível, ação que só aparece no hover, recurso destravado por um toggle escondido em outra aba, `Join link` enterrado na aba Follow.
4. **Pede o dado cru em vez de deixar escolher.** Place ID, Job ID, Universe ID, hash de versão, caminho de arquivo — e o campo de Place ID ainda corrompe link colado em silêncio.
5. **Ação perigosa com o mesmo peso visual de ação inofensiva.** Trocar senha e e-mail da conta Roblox não confirmam; "Sign out of other sessions", que é menos grave, confirma.

---

## P0 — a tela mente ou o ajuste não funciona

Corrigir primeiro: aqui o usuário toma decisão errada com base no que lê.

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

## P1 — existe e ninguém acha

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

## P2 — está na tela e não se explica

| # | Achado |
|---|---|
| ✓ | **Scripts não diz o que é**: JavaScript num Web Worker *dentro do RAM*, não executor, sem injector. O único texto é o subtítulo (`ScriptsDialog.tsx:3033`) |
| | **As 8 permissões de script são nomes técnicos** sem descrição (`PermissionRow` nem aceita uma) |
| | **Botting não diz que fecha e reabre o cliente a cada ciclo**, e os campos de tempo perdem a unidade na vista padrão |
| | **Nexus**: o ícone diz "Nexus", o diálogo diz "Account Control", e o Help não menciona que é preciso um executor de terceiros |
| | **A aba WebServer inteira é uma lista de permissões sem uma linha de explicação** — `Allow GetCookie` entrega o cookie da conta |
| | **A aba Watcher liga um sistema inteiro** sem dizer o que ele faz |
| | **Nada diz que o `Account Generator` é serviço pago de terceiro**, nem que existe a alternativa gratuita ao lado |
| | **O cookie `.ROBLOSECURITY` é pedido em três lugares** e em nenhum se explica onde achá-lo |
| | **`aged` não diz de que envelheceu** e a bolinha vira vermelho puro aos 30 dias, idêntica à de sessão inválida (`types.ts:174`) |
| | **Quando o launch falha, o porquê está em outra tela** — o log explicativo só existe em Choose Game › Console |
| | **A faixa de erro corta o texto** numa linha e só oferece ação se o erro contiver "failed to enable multi roblox" (`App.tsx:96`) |
| | **`Quick Login` não diz de onde vem o código** de 6 dígitos |
| | **`Unlock` com PIN de 4 dígitos** sem explicação do que destrava |
| | **`Region Format` pede um template** sem dizer que tokens existem |
| | **21 opções de Settings são indecifráveis** só pela tela (de 109 catalogadas); outras 34 explicam pela metade |
| | **Quatro nomes diferentes para as mesmas duas funções** de criar conta |

## P3 — desenho e ergonomia

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
