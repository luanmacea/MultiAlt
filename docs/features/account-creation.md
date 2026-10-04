# Criação de contas

## Objetivo

Conseguir contas novas em série sem digitar cadastro por cadastro. Duas formas, no mesmo diálogo — cada uma com a sua entrada no menu **Add** (*Create Accounts* e *Account Generator*), que abre o diálogo já na aba certa:

1. **Comprar de um provedor** (BloxGen) — a conta vem pronta por API;
2. **Criar no navegador** — o app abre a página de cadastro do Roblox, preenche tudo, e **o usuário resolve o CAPTCHA e confirma**.

> **O gerador pago (BloxGen) está escondido desde 03/10/2026** atrás de `ENABLE_ACCOUNT_GENERATOR` ([featureFlags.ts](../../src/featureFlags.ts), desligado por padrão): o dono acha que o serviço parou de funcionar e ninguém deve gastar crédito tentando. Sem a flag somem a entrada *Account Generator* do menu **Add** (Toolbar e AddAccountDialog), a aba paga do diálogo Contas novas — que abre direto na criação grátis, sem seletor de abas — e a seção *Account Generator* das Settings. O código fica todo (comandos Rust, `GeneratorTab`, painel do diálogo, API de scripts `start_generator`). Exceção: com um gerador pago **rodando** (ligado pela API de scripts), o diálogo mostra o painel para dar para parar. Para religar: compile com `VITE_ENABLE_ACCOUNT_GENERATOR=true` ou troque o padrão na flag. Testes: `accountGeneratorHidden.test.tsx` (desligado); os do gerador ligam a flag com `vi.mock`.

> O app **não** resolve, burla nem esconde a verificação do Roblox. A parte humana continua humana; o que se economiza é a digitação.

## Onde fica o código

| Peça | Arquivo |
|---|---|
| Identidade gerada e script de preenchimento (puro) | [chromium/signup.rs](../../src-tauri/src/chromium/signup.rs) |
| Sessão (browser, espera, salvar conta) | [chromium/signup_session.rs](../../src-tauri/src/chromium/signup_session.rs) — comandos `start_signup_session`, `stop_signup_session`, `get_signup_status` |
| Nome livre no Roblox | [api/roblox/username_check.rs](../../src-tauri/src/api/roblox/username_check.rs) (`check_signup_username`) |
| Provedor (BloxGen) | [commands/generators.rs](../../src-tauri/src/commands/generators.rs) |
| UI | [components/signup/SignupPanel.tsx](../../src/components/signup/SignupPanel.tsx), aba do [GeneratorDialog.tsx](../../src/components/dialogs/GeneratorDialog.tsx) |
| Eventos | `signup-progress`, `generator-status`, `generator-account-added` |
| Suíte de teste | `bun run t signup` |

## Fluxo — criar no navegador

1. O usuário escolhe quantas contas (1–50) e clica **Start**.
2. O app abre **uma** janela do Chromium (perfil limpo) e, para cada conta:
   1. limpa os cookies do Roblox e recarrega `roblox.com/CreateAccount`;
   2. gera a identidade, seleciona dia/mês/ano e gênero e **digita** usuário e senha;
   3. publica `signup-progress` com a identidade — a UI mostra **usuário e senha**, copiáveis;
   4. espera o `.ROBLOSECURITY` aparecer (até 5 min), que é o sinal de que o cadastro foi concluído;
   5. valida o cookie, guarda a conta com a senha gerada e vai para a próxima.
3. Fechar a janela encerra a sessão; **Stop** também.

## Regras de negócio

- **O app nunca clica em "Criar conta"** — há teste de regressão para isso. O envio é do usuário, depois do CAPTCHA.
- **Os campos de texto são digitados pelo CDP** (`Input.insertText`), não escritos por `value`. O React de alguns componentes ignora um valor escrito por fora — era o que fazia o nome de usuário chegar vazio enquanto a senha, no mesmo formulário, era preenchida.
- **O preenchimento roda em laço** enquanto se espera o usuário (a cada ~2 s, idempotente). É o que cobre campo que remonta e, principalmente, a **segunda versão** do cadastro que o Roblox serve por experimento: nela a senha só é pedida depois do "Continue", e o laço preenche essa tela sozinho quando ela aparece.
- **Conta salva aparece na lista na hora**: o evento `generator-account-added` recarrega a lista principal, então parar a sessão no meio não esconde o que já foi criado.
- **A senha gerada aparece na tela.** Ela não existe em nenhum outro lugar antes de a conta ser salva; sem mostrá-la, a conta ficaria presa ao cookie.
- **Nome de usuário sempre válido para o Roblox**: 3–20 caracteres, letras/dígitos e **um** underscore, nunca nas pontas, com sufixo de 4 dígitos.
- **Prefixo escolhido pelo usuário** (`Generator.SignupUsernamePrefix`, campo no painel de criação): com "arvore", as contas saem `arvore_k3p9z` — prefixo, underscore e **5 caracteres sorteados** (minúsculas e dígitos, 60 milhões de combinações). Vazio mantém o nome de palavras de sempre. O prefixo é limpo antes de entrar no nome: só letras e dígitos ASCII (o underscore do separador já gasta o único que o Roblox permite) e no máximo **14 caracteres**, que é 20 − 1 − 5. Prefixo que não sobrevive à limpeza cai no nome de palavras, em vez de gerar `_k3p9z`.
  - O prefixo é **parâmetro obrigatório** de `generate_username`: quem sorteia um nome novo — inclusive o re-sorteio de `pick_free_username`, quando o Roblox diz que o nome está em uso — é obrigado pelo compilador a passar o prefixo, e não dá para perdê-lo no meio do caminho.
  - **Não vale para o BloxGen**: naquele fluxo o nome vem pronto do provedor, o app não escolhe.
- **O nome é conferido antes de preencher** (`auth/v2/usernames/validate`, sem autenticação): se já existir conta com ele, sorteia outro, até 6 tentativas. Descobrir isso só no envio queimaria o CAPTCHA que a pessoa acabou de resolver. Serviço fora do ar não trava a sessão — segue com o nome sorteado e o formulário valida como sempre.
- **O laço de reparo só preenche campo vazio.** Ele nunca sobrescreve o que está na tela: era isso que impedia de aceitar uma sugestão do Roblox ("este nome já está em uso → tente Ultra_Lynx7440") ou de corrigir qualquer campo na mão.
- **Idade entre 18 e 40 anos.** Abaixo de 13 o Roblox liga o modo infantil (chat e experiências limitados). O dia sorteado fica em 1–28 para nenhum mês cair num dia inexistente.
- **Senha sem caracteres ambíguos** (`0`/`O`/`l`/`1`): ela é lida da tela pelo usuário.
- Os valores entram no script como **literais JSON** — nada digitado ou gerado pode virar código na página.
- **Teto de 50 contas por sessão**, para um clique errado não virar uma maratona.
- Timeout de uma conta **pula** aquela conta; janela fechada **encerra** a sessão.

## Seletores da página de cadastro

A página é React, então escrever `.value` direto não atualiza o estado: o script usa o setter nativo do prototype e dispara os eventos (mesma técnica do `login_fill_script`).

| Campo | Seletor | Reserva (página antiga) |
|---|---|---|
| Dia / Mês / Ano | `[data-testid="birthday-day"] select` etc. | `#DayDropdown`, `#MonthDropdown`, `#YearDropdown` |
| Usuário | `#signup-username` | — |
| Senha | `#signup-password` | — |
| Gênero masculino | botão com `.icon-regular-head-male` | `#MaleButton` |

O `value` do mês é o inglês de três letras (`Mar`), **não** o rótulo traduzido; o do dia tem dois dígitos (`07`). O botão de gênero é achado pelo ícone porque o texto muda com o idioma da página.

## Fluxo — provedor (BloxGen)

Sessão em laço: pede uma conta, adiciona, espera o cooldown, repete até o limite de contas ou até o orçamento de falhas acabar.

- **Falha transitória conta para o limite.** Antes, `Transient` (estoque vazio, saldo zerado, chave vencida) não contava: o gerador tentava de novo a cada 15 s **para sempre**, em silêncio, e o usuário só via "gerando". Agora, depois de `Generator.MaxConsecutiveFailures` tentativas seguidas sem conta (padrão 3), a sessão para com erro.
- **Cooldown não é falha**: o provedor pedindo para esperar zera o contador.
- `MaxConsecutiveFailures = 0` é a escolha explícita de "tentar para sempre".

## Configurações relacionadas

| Chave | O quê |
|---|---|
| `Generator.Provider`, `Generator.TargetGroup`, `Generator.MaxAccounts`, `Generator.ExtraDelaySeconds` | Sessão do provedor |
| `Generator.MaxConsecutiveFailures` | Tentativas seguidas sem conta antes de parar (padrão 3) |
| `BloxGen.Endpoint`, `BloxGen.ApiKey`, `BloxGen.AccountType` | Credenciais do provedor |
| `Login.StealthMode` | Vale também para o navegador de cadastro |

## Armadilhas / cuidados

- O Roblox muda o formulário de cadastro com frequência; por isso os seletores novos **e** os antigos convivem. Se o preenchimento parar de funcionar, comece conferindo os `data-testid` da página.
- O Roblox pode recusar o nome sorteado (já em uso, ou filtrado). Isso aparece na própria página: o usuário corrige na tela e segue — a conta é salva com o nome que o Roblox aceitou.
- Uma sessão por vez: começar outra enquanto uma roda devolve erro.
- **O mesmo navegador do login.** `start_signup_session` passa por `resolve_browser_binary`: o caminho manual (`Login.ManualBinaryPath`) vence, depois o Chromium baixado, e o navegador do sistema se o download falhar (ver [chromium.md](chromium.md#qual-binário-abre-manual-baixado-ou-navegador-do-sistema)). Até 27/09/2026 era sempre o Chromium baixado.
