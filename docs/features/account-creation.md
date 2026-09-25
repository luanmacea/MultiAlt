# Criação de contas

## Objetivo

Conseguir contas novas em série sem digitar cadastro por cadastro. Duas formas, no mesmo diálogo — cada uma com a sua entrada no menu **Add** (*Create Accounts* e *Account Generator*), que abre o diálogo já na aba certa:

1. **Comprar de um provedor** (BloxGen) — a conta vem pronta por API;
2. **Criar no navegador** — o app abre a página de cadastro do Roblox, preenche tudo, e **o usuário resolve o CAPTCHA e confirma**.

> O app **não** resolve, burla nem esconde a verificação do Roblox. A parte humana continua humana; o que se economiza é a digitação.

## Onde fica o código

| Peça | Arquivo |
|---|---|
| Identidade gerada e script de preenchimento (puro) | [chromium/signup.rs](../../src-tauri/src/chromium/signup.rs) |
| Sessão (browser, espera, salvar conta) | [chromium/signup_session.rs](../../src-tauri/src/chromium/signup_session.rs) |
| Provedor (BloxGen) | [commands/generators.rs](../../src-tauri/src/commands/generators.rs) |
| UI | [components/signup/SignupPanel.tsx](../../src/components/signup/SignupPanel.tsx), aba do [GeneratorDialog.tsx](../../src/components/dialogs/GeneratorDialog.tsx) |
| Eventos | `signup-progress`, `generator-status`, `generator-account-added` |
| Suíte de teste | `bun run t signup` |

## Fluxo — criar no navegador

1. O usuário escolhe quantas contas (1–50) e clica **Start**.
2. O app abre **uma** janela do Chromium (perfil limpo) e, para cada conta:
   1. limpa os cookies do Roblox e recarrega `roblox.com/CreateAccount`;
   2. gera a identidade e preenche usuário, senha, dia/mês/ano e gênero;
   3. publica `signup-progress` com a identidade — a UI mostra **usuário e senha**, copiáveis;
   4. espera o `.ROBLOSECURITY` aparecer (até 5 min), que é o sinal de que o cadastro foi concluído;
   5. valida o cookie, guarda a conta com a senha gerada e vai para a próxima.
3. Fechar a janela encerra a sessão; **Stop** também.

## Regras de negócio

- **O app nunca clica em "Criar conta"** — há teste de regressão para isso. O envio é do usuário, depois do CAPTCHA.
- **A senha gerada aparece na tela.** Ela não existe em nenhum outro lugar antes de a conta ser salva; sem mostrá-la, a conta ficaria presa ao cookie.
- **Nome de usuário sempre válido para o Roblox**: 3–20 caracteres, letras/dígitos e **um** underscore, nunca nas pontas.
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
