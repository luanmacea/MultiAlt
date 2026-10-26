# Chromium embutido (login e browser por conta)

## Objetivo

Baixar e controlar uma cópia própria do Chromium (Chrome for Testing) para dois usos:

- **Login**: abrir a página de login do Roblox, esperar o usuário (ou preencher usuário/senha por ele) e capturar o cookie `.ROBLOSECURITY` que sai do login, importando a conta sem que o usuário precise colar cookie na mão.
- **Browser por conta**: abrir uma janela já autenticada com o cookie de uma conta salva, em um perfil separado por `UserID` — que, por padrão (`Login.PersistentProfile` desligado), começa limpo e some quando a janela fecha.

O controle é feito por **CDP** (Chrome DevTools Protocol) sobre WebSocket.

## Onde fica o código

| Arquivo | Papel |
|---|---|
| [chromium/download.rs](../../src-tauri/src/chromium/download.rs) | Resolve a build estável no catálogo Chrome for Testing, baixa, extrai e cacheia o binário (`ensure_chromium`, `is_installed`, `chromium_dir`); `resolve_browser_binary` decide entre caminho manual, download e navegador do sistema; `reinstall_chromium` apaga e baixa de novo |
| [chromium/cdp.rs](../../src-tauri/src/chromium/cdp.rs) | `chrome_args` (linha de comando), `spawn_chrome` (processo + leitura do `DevToolsActivePort`), `CdpClient` (WebSocket CDP: `Network.*`, `Page.*`, `Runtime.evaluate`, `Browser.close`) |
| [chromium/manager.rs](../../src-tauri/src/chromium/manager.rs) | `ChromiumManager`: processos vivos por conta (`LOGIN_KEY` = `i64::MIN` para a janela de login), cookie de login capturado, caminhos dos perfis |
| [chromium/commands.rs](../../src-tauri/src/chromium/commands.rs) | Comandos Tauri: `is_browser_ready`, `ensure_browser`, `open_login_browser`, `extract_browser_cookie`, `close_login_browser`, `open_account_browser`, `import_userpass`, `run_browser_diagnostics` |
| [chromium/diagnose.rs](../../src-tauri/src/chromium/diagnose.rs) | Checagem do navegador (`browserBinary`, `browserFolder`, `browserLoginOpen`, `browserStart`, `browserRoblox`) — ver "Checagem do navegador" abaixo |
| [store.tsx](../../src/store.tsx) | `openLoginBrowser` / `openAccountBrowser` e o listener do evento `browser-login-detected` |
| [ImportDialog.tsx](../../src/components/dialogs/ImportDialog.tsx) | Chama `import_userpass` (usuário + senha) |

Perfis ficam em `<dir de dados>/chromium-profiles/<UserID>` e `<...>/chromium-profiles/_login`.

## Fluxo

### Login (`open_login_browser`)

1. `ensure_chromium` garante o binário; qualquer sessão de login anterior é encerrada (`close_login_session`, que também limpa o cookie em memória).
2. Se `Login.PersistentProfile` estiver desligado, o perfil `_login` é apagado antes de subir.
3. `spawn_chrome(..., debug = true, ...)` sobe o browser com `--remote-debugging-port=0` e `--remote-debugging-address=127.0.0.1`. A porta real é lida da primeira linha do arquivo `DevToolsActivePort` dentro do perfil (polling de até 8 s).
4. `CdpClient::connect(port)` busca em `http://127.0.0.1:<porta>/json` o primeiro alvo `page` e abre o WebSocket `webSocketDebuggerUrl`.
5. `setup_login_session`: injeta o script de stealth (se ligado), apaga um `.ROBLOSECURITY` remanescente (perfil persistente) e navega para a página de login.
6. Uma task de fundo pergunta `Network.getAllCookies` a cada 500 ms (até 4 min). Quando o `.ROBLOSECURITY` aparece: guarda em memória no `ChromiumManager`, **fecha a janela na hora** (`close_login_window`), apaga o perfil se ele não for persistente e emite `browser-login-detected`.
7. O frontend busca o valor com `extract_browser_cookie` (só lê a memória do backend, não o browser), adiciona a conta e chama `close_login_browser`, que limpa o cookie em memória.

### Login com usuário e senha (`import_userpass`)

Igual aos passos 1–5, mas depois espera o seletor `#login-username` e manda um `Runtime.evaluate` que preenche os campos e clica em entrar. Continua fazendo o polling do cookie **na própria chamada** (o usuário pode ter 2FA/captcha pela frente); assim que o cookie sai, fecha a janela, apaga o perfil não persistente, valida o cookie (`api::auth::validate_cookie`) e grava a conta.

### Browser por conta (`open_account_browser`) — duas fases

1. Normaliza o token da conta (`normalize_security_token`) e mata uma janela anterior da mesma conta (duas instâncias no mesmo perfil brigam pelo lock do Chromium). Com `Login.PersistentProfile` desligado (o padrão), o perfil da conta é apagado aqui, antes da fase 1, e um vigia o apaga de novo quando a janela da fase 2 fecha (ver "Anônimo por padrão em tudo").
2. **Fase 1 — browser de setup**: sobe com a porta de debug, parado em `about:blank`, **sem nenhuma página do Roblox carregada**. Via CDP grava o `.ROBLOSECURITY` em `.roblox.com` e `www.roblox.com` **com validade de 1 ano** (`persistent_cookie_expiry`), o que faz o Chromium persistir o cookie no perfil em vez de mantê-lo só em memória.
3. `close_browser` pede o encerramento gracioso (`Browser.close`, com `Page.close` como alternativa) — é o shutdown normal do Chromium que grava o cookie no disco. Esperamos a saída por até 10 s e, se travar, matamos o processo.
4. **Fase 2 — a janela do usuário**: sobe de novo no mesmo perfil, direto em `https://www.roblox.com/home` — ou, com `groupId` (página Groups, depois de um captcha; ver [groups.md](groups.md)), em `https://www.roblox.com/communities/<groupId>`, montada no backend (`account_browser_start_url`) a partir só do id numérico —, **sem `--remote-debugging-port`**. É essa instância que o `ChromiumManager` rastreia.

## Sem abas antigas nem pop-ups (26/10/2026)

Relato do dono: o navegador de login abria com as abas antigas e cheio de oferta (salvar senha, "Fazer login no Chromium?", a faixa "Chrome for Testing é destinado apenas a testes automatizados"). Toda abertura (login, criação de contas, browser por conta) passa por `spawn_chrome`, que agora:

- **Prepara o perfil** (`prepare_profile`, [cdp.rs](../../src-tauri/src/chromium/cdp.rs)): apaga `Default/Sessions` (as abas da vez anterior) e grava em `Default/Preferences` as chaves de `PROFILE_PREFERENCES` — sem salvar senha, sem alerta de senha vazada, sem preenchimento de endereço/cartão, notificações bloqueadas e saída "Normal". A causa das abas antigas, reproduzida com uma cópia do perfil `_login` real: o app fecha a janela de login **matando** o processo, o perfil fica `exit_type: Crashed` e a abertura seguinte mostrava "Restaurar páginas? O Chromium não foi encerrado corretamente" — e o "Restaurar" trazia tudo de volta. Nenhuma chave das `Secure Preferences` (assinadas) é tocada: mexer nelas faz o Chrome avisar que as configurações foram alteradas. Preferências que não dão para ler ficam como estão. Cookie, cache e histórico ficam — é o perfil persistente funcionando.
- **Teste real pelo app (26/10/2026, conta de teste Chorardeamanah, "Navegador" no painel):** ao abrir o build novo, os 20 perfis antigos sumiram; a conta abriu logada num perfil novo e, ao fechar a janela, o perfil sumiu e nenhum processo ficou. O mesmo teste achou mais dois avisos num perfil limpo: a extensão que o Google Drive do PC instala em todo Chrome ("Application Launcher For Drive... added" e "Action required") — saiu com `--disable-extensions` (o app não usa extensão) — e a oferta de tradução (navegador em inglês no stealth, Roblox em português), que o `--disable-features=Translate` já não segura no Chrome 154 — saiu com `translate.enabled: false` nas preferências. Conferido de novo depois das duas: nenhum aviso.
- **Flags novas** (`chrome_args`), medidas com o Chrome for Testing 154: `--disable-infobars` tira a faixa do Chrome for Testing (`--test-type` **não** tira), `--disable-sync` tira o "Fazer login no Chromium?", e `--hide-crash-restore-bubble`, `--disable-search-engine-choice-screen` e `PasswordLeakDetection` em `--disable-features` fecham o resto.
- Testes: `chromium_cdp_tests` (flags e preparação do perfil em pasta temporária) e o real `real_chromium_reopens_a_profile_without_the_old_tabs` (`#[ignore]`, `RAM_CHROME_BINARY=<chrome.exe>`; `RAM_CHROME_KILL=1` fecha matando, como o login).

**Anônimo por padrão em tudo (26/10/2026, pedido do dono).** `Login.PersistentProfile` agora nasce **desligado** ("Keep browser data between uses", Settings › Accounts) e vale para **todo** uso do navegador (`keeps_browser_data`, [commands.rs](../../src-tauri/src/chromium/commands.rs)):
- **Login:** o perfil `_login` é apagado antes e logo depois da captura do cookie (já era assim com a opção desligada).
- **Criação de contas:** sempre começa num perfil apagado (com a opção ligada ou não).
- **Abrir a conta no navegador** (`open_account_browser`) e as páginas automáticas (`AccountPageBrowser`, grupos/perfil): o perfil da conta é apagado antes da fase 1 e **some quando a janela fecha** — um vigia segue o PID da janela (`private_watch_step`: janela trocada por outra da mesma conta não apaga, quem abriu a nova já começou do zero) e a automática apaga no `close`.
- **Ao abrir o app** com a opção desligada, os perfis que sobraram de antes (de quando era persistente, ou de janela fechada com o app fora) são apagados em segundo plano (`wipe_private_profiles`).
- **Migração única** (`Login.PrivateBrowserDefaultV1`): quem já tinha o app vinha com `PersistentProfile=true` gravado (o padrão antigo); a primeira abertura da versão nova desliga, e depois respeita quem ligar de novo.

Ligada, o perfil acumula confiança do Roblox e pede menos captcha, mas todas as contas passam pelo mesmo perfil. Testes: `the_private_browser_migration_turns_persistence_off_once`, `a_new_install_starts_with_the_private_browser` (settings), `a_closed_private_window_gets_its_profile_wiped_and_a_replaced_one_does_not` e `starting_private_wipes_every_profile_left_from_before` (`chromium_commands_tests`).

## Checagem do navegador (`run_browser_diagnostics`, 26/10/2026)

Um usuário relatou que o navegador "não fazia nada" ao criar contas e que reinstalar o Google Chrome resolveu: o download do Chromium próprio tinha falhado e o app estava usando o Chrome do PC (`find_system_chromium`). O cartão "Login browser does nothing?" (Settings › Checks, ao lado do checklist do launch) abre o mesmo diálogo do launch (`DiagnosticsDialog kind="browser"`), com as checagens de [chromium/diagnose.rs](../../src-tauri/src/chromium/diagnose.rs):

| id | O que diz |
|---|---|
| `browserBinary` | Qual navegador o login usa: o escolhido em Settings (`manual` / `manualInvalid`), o próprio (`downloaded` / `damaged`, faltando arquivo) ou, sem ele, o do PC (`systemFallback`, aviso — o caso do relato) / nenhum (`notDownloaded`) |
| `browserFolder` | A pasta dos perfis aceita escrita |
| `browserLoginOpen` | Há uma janela de login aberta agora (só aparece nesse caso) |
| `browserStart` | Abertura de teste num perfil só dela (`_diagnostic`, apagado no fim): `opened` (com os segundos), `spawnFailed` (o Windows não deixou começar — antivírus), `didNotOpen` (subiu e não respondeu em 30 s — o "não faz nada"), `skipped` |
| `browserRoblox` | A página de login do Roblox carregou dentro do navegador de teste |

Não baixa nada (o download fica no botão Reinstall/Download) e não toca no perfil de login nem nos das contas. Como no launch, só `id`/`reason`/`count` saem do backend; a frase é de [utils/diagnostics.ts](../../src/utils/diagnostics.ts). Testes: `browser_diagnose_tests` (inclui o real `real_browser_check_opens_reaches_roblox_and_cleans_up`, `#[ignore]`, `RAM_CHROME_BINARY`) e `DiagnosticsDialog.test.tsx`.

## Qual binário abre: manual, baixado ou navegador do sistema

`resolve_browser_binary` (chromium/download.rs) é chamado pelos três fluxos acima (`open_login_browser`, `import_userpass`, `open_account_browser`) **e pela criação de contas** (`start_signup_session`, [signup_session.rs](../../src-tauri/src/chromium/signup_session.rs)) em vez de `ensure_chromium` direto. Até 27/09/2026 a criação chamava `ensure_chromium` direto e ignorava o caminho manual e o navegador do sistema — quem configurou o caminho porque o download falha ficava preso no download ao criar contas. Ordem de preferência:

1. **`Login.ManualBinaryPath`**, se configurado (Settings > Accounts > Login browser > "Custom browser executable"). É a escolha explícita do usuário — ele digitou aquele caminho para *não* depender do download nem da detecção automática, então vence os outros dois. O caminho **tem** que apontar para um arquivo comum de verdade (`is_regular_file`, via `symlink_metadata` — recusa pasta e link simbólico, ao contrário de `Path::exists`/`is_file`, que seguem o link). Inválido = erro na hora, sem cair silenciosamente para o download ou para o navegador do sistema: cair para outra coisa seria exatamente o que o usuário configurou o campo para evitar. `is_regular_file` só garante o *tipo* do caminho, não que o arquivo é de fato um navegador — se não for, o erro aparece mais adiante (timeout lendo `DevToolsActivePort`, ou falha ao conectar o CDP), como já acontecia com qualquer processo que não sobe corretamente.
2. **Chromium baixado** (`ensure_chromium`): o caminho de sempre, cacheado em `chromium_dir`.
3. **Navegador do sistema** (`find_system_chromium`), só quando o download falhar *e* não houver caminho manual configurado. Procura Chrome, Chromium, Brave e Edge em locais padrão (`Program Files`, `Program Files (x86)`, `%LOCALAPPDATA%` no Windows; `/Applications` no macOS; `/usr/bin` no Linux) e usa o primeiro que for um arquivo comum — mesma regra do caminho manual, mesmo motivo: login e criação de conta passam perto de credencial, e um link simbólico ou uma pasta com nome de executável não podem contar como candidato. O frontend recebe o evento `chromium-fallback` (`{ browser, error }`) e mostra um aviso; a sessão continua com o navegador achado, mas fora do nosso controle (versão e flags variam).

Se nenhuma das três resolver, o erro do download original é devolvido (com a dica de `with_download_hint`).

## Robustez do download

- **Retry na consulta de versão**: `resolve_download` tenta `fetch_version_list` até 3 vezes com espera crescente (400 ms, 800 ms) antes de desistir. `fetch_version_list` usa `error_for_status`, então uma resposta não-2xx do catálogo (o CDN do chrome-for-testing devolve 5xx de vez em quando) conta como falha e entra no retry — sem isso o corpo do erro seria lido como se fosse o JSON do catálogo e o retry nunca disparava.
- **Timeouts**: `fetch_version_list` usa `connect_timeout` de 10 s e `timeout` de 30 s. Sem isso um DNS ou proxy travado prendia o fluxo de login inteiro em vez de falhar rápido e cair no navegador do sistema. O **download do zip** (`download_archive`, cliente de `archive_client`) tem `connect_timeout` de 10 s e teto de **inatividade** de 30 s (`ARCHIVE_IDLE_TIMEOUT`: 30 s sem receber nenhum byte, desiste), mas **não** tem teto para o download inteiro — o zip tem ~150 MB e numa conexão lenta leva minutos de verdade. Antes era `reqwest::get`, sem teto nenhum, e um download parado no meio prendia o login e a criação de contas para sempre.
- **Extração atômica**: a extração vai para `<chromium_dir>/<versão>.tmp`, nunca direto em `<chromium_dir>/<versão>` (o caminho que `cached_binary` lê do `version.json`). Só depois que a extração inteira termina com sucesso é que `finalize_extraction` troca os nomes (`remove_dir_all` se já existir algo ali, depois `rename`). **Se o processo morrer no meio da extração** (energia, antivírus, disco cheio), o que sobra é só o `.tmp` — o caminho final nunca existe pela metade, então a próxima abertura do app detecta que não há instalação válida e baixa de novo, em vez de aceitar um Chromium com `chrome.exe` presente mas DLLs/recursos faltando (o que aconteceria se o zip gravasse o executável antes dos outros arquivos e a extração fosse interrompida escrevendo direto no destino final).
- **Reinstalar substitui**: `reinstall_chromium` apaga `chromium_dir` inteiro antes de chamar `ensure_chromium` de novo — é o que o botão "Reinstall" de Settings > Accounts usa (`ensure_browser` com `force: true`). Sem isso, trocar de versão deixava a pasta da versão antiga para trás, ocupando espaço à toa.

## Navegador que abre e não responde (23/10/2026)

Relato: no criador de contas o Chromium abria e ficava em `about:blank` para sempre, com o painel em "Inativo". Causas achadas no código e o que mudou:

- **Instalação incompleta passava por boa.** `cached_binary` só olhava se o `chrome.exe` existia. Agora `installation_problem` confere também `chrome.dll`, `resources.pak`, `icudtl.dat` e `v8_context_snapshot.bin` (tamanho mínimo bem abaixo do real, medido numa instalação 154: `chrome.dll` ~300 MB). Faltando ou cortado (antivírus que apagou um arquivo depois da extração, extração interrompida), a instalação conta como ausente e `ensure_chromium` baixa de novo (antes o `binary.exists()` pulava o download). Só no Windows; nas outras plataformas confere só o executável.
- **A espera pela porta de debug era de 8 s** e, estourando, o comando devolvia erro **sem fechar a janela** — ela ficava aberta em `about:blank` e o painel nunca saía de "Inativo". Agora espera 30 s (`ACTIVE_PORT_POLLS`), desiste na hora se o processo já saiu, e quem devolve o erro (criador de contas e os dois logins) fecha a janela do app (`close_login_session`) com a frase `BROWSER_DID_NOT_OPEN` (antivírus/firewall, "Reinstall" em Configurações › Contas), traduzida no frontend (`chromiumErrorText`).
- **O CDP não tinha prazo de conexão:** `CdpClient::connect` usava `reqwest::get` (com proxy do sistema, e sem prazo) e `connect_async` sem timeout. Agora o cliente local ignora proxy, tem 3 s por tentativa, e o WebSocket 10 s.
- **`Page.navigate` que falha** (DNS, proxy, firewall) respondia sucesso com `errorText` e o app seguia esperando o formulário; agora vira erro na hora (`navigation_error`).

Não confirmado: por que o navegador do relato (Chrome for Testing 136, versão antiga: o app baixa a "stable" da hora e guarda para sempre) não respondia; as checagens acima cobrem as causas que o código permite, e o próximo relato mostra qual era pela mensagem.

## Segurança: a porta de debug

### O risco

O endpoint CDP **não tem autenticação**. Enquanto ele existir, qualquer processo local rodando com o mesmo usuário pode:

- varrer as portas de loopback, achar o `/json/version` do Chromium e abrir o WebSocket;
- pedir `Network.getAllCookies` e sair com o `.ROBLOSECURITY` — que é a sessão inteira da conta;
- usar `Runtime.evaluate` / `Page.navigate` para agir como o usuário logado.

O `DevToolsActivePort` só evita a varredura; ele não é um segredo (fica no perfil, legível pelo mesmo usuário).

### O que foi mitigado

| Mitigação | Onde | Efeito |
|---|---|---|
| A janela de navegação da conta não tem mais porta de debug | `open_account_browser` ([commands.rs](../../src-tauri/src/chromium/commands.rs)) | Antes, a porta ficava aberta **durante toda a sessão de navegação** (horas), com o Roblox logado na tela. Agora ela existe por ~1 s, com o browser em `about:blank` |
| A janela de login fecha no instante em que o cookie é capturado | task de polling em `open_login_browser` | Antes ela ficava aberta até o frontend mandar fechar; a sobreposição "sessão autenticada + CDP aberto" caiu de indefinida para ~0,5 s |
| Perfil não persistente é apagado logo após a captura | idem | O `Cookies` do perfil sai do disco sem esperar o `close_login_browser` |
| Falha ao ler a porta ou ao conectar no CDP derruba o browser | `open_account_browser` | Antes um browser com porta aberta e ninguém do nosso lado ficava vivo e era devolvido como sucesso |
| `--remote-debugging-address=127.0.0.1` explícito | `chrome_args` ([cdp.rs](../../src-tauri/src/chromium/cdp.rs)) | É o default do Chromium, mas fixá-lo impede que uma policy ou flag herdada exponha o CDP para a rede — aí o roubo nem exigiria processo local |
| Nunca passamos `--remote-allow-origins` | `chrome_args`, com teste que o garante | Desde o Chrome 111 um handshake WebSocket com cabeçalho `Origin` é recusado por padrão. É o que impede uma **página web** aberta no próprio Chromium de falar CDP. O flag afrouxaria isso |
| `ChromiumManager` tolera lock envenenado | [manager.rs](../../src-tauri/src/chromium/manager.rs) | Um panic segurando o `Mutex` envenenava tudo e, com `.lock().unwrap()`, fazia toda chamada seguinte entrar em panic — inclusive `close_login_session`, que é quem fecha o browser e limpa o cookie |

### Por que não `--remote-debugging-pipe`

É a mitigação ideal (elimina a superfície de rede: o CDP passa por um par de descritores herdados, que só o processo pai enxerga), e foi **descartada por incompatibilidade com o cliente atual**:

- No Windows, o Chromium espera o pipe nos **descritores CRT 3 e 4** do processo filho. `std::process::Command` só sabe redirecionar stdin/stdout/stderr; passar os descritores 3 e 4 exigiria `CreateProcess` cru com o bloco `lpReserved2` de herança do CRT.
- O `CdpClient` deste projeto é um cliente **WebSocket** (`tokio-tungstenite`) que descobre o alvo pelo HTTP `/json`. No modo pipe não existe nem HTTP nem WebSocket: as mensagens são JSON cru separado por `\0`, e o alvo tem que ser achado por `Target.getTargets` + `Target.attachToTarget` com `sessionId`. Seria reescrever o transporte inteiro.

Trocar o transporte quebraria os dois fluxos de login de uma vez, sem como testar automaticamente (nenhum teste sobe browser real). Fica registrado como o próximo passo se a exposição residual incomodar.

### Risco aceito (o que um processo local malicioso ainda consegue)

1. **Durante o login** (do `open_login_browser`/`import_userpass` até a captura do cookie) a porta CDP existe e é atacável. Um processo que esteja esperando esse momento consegue ler o cookie assim que ele nasce, ou observar o `Runtime.evaluate` que preenche usuário e senha. Essa janela é inerente ao recurso: sem CDP não há como detectar o login nem preencher o formulário.
2. **Durante a fase 1 do browser por conta** (~1 s, browser em `about:blank`) o cookie é gravado via CDP e pode ser lido por quem estiver conectado naquele instante.
3. **O perfil no disco.** Um perfil persistente guarda o cookie no `Cookies` do Chromium, protegido só pelas permissões do usuário (no Windows, com DPAPI do próprio usuário). Qualquer processo do mesmo usuário lê. Vale igual para o `AccountData.json`.
4. **O próprio `AccountData.json`** já contém os cookies de todas as contas; um processo local com acesso ao perfil do usuário não precisa do CDP para nada. O CDP importa porque é a via mais fácil e silenciosa.

Ou seja: a mudança não torna o app resistente a um atacante já rodando como o usuário — ela reduz a janela de oportunidade de "horas, com a sessão logada na tela" para "segundos, no momento do login".

## Regras de negócio

- `LOGIN_KEY` é `i64::MIN`: IDs de usuário do Roblox são positivos, então o sentinela nunca colide com uma conta.
- `close_login_window()` fecha o processo **mantendo** o cookie em memória (o frontend ainda vai buscá-lo); `close_login_session()` fecha **e** limpa. Nunca troque um pelo outro.
- O cookie capturado no login vive só em memória, no `ChromiumManager`; nunca é gravado em arquivo pelo módulo do Chromium.
- `normalize_security_token` tira espaço **antes** das aspas. Na ordem inversa, `.ROBLOSECURITY="token" ; Path=/` devolvia `token"` e o cookie chegava quebrado no Roblox (corrigido; há teste de regressão).
- O cookie plantado no `open_account_browser` tem validade explícita — sem ela seria cookie de sessão e não sobreviveria ao restart entre as duas fases.
- Na fase 2 não injetamos o script de stealth: sem CDP anexado o `navigator.webdriver` já nasce indefinido, então a janela do usuário é *mais* discreta que antes.
- Credenciais e seletores entram nos scripts sempre como literais JSON (`serde_json::to_string`), nunca concatenados — há testes de injeção para os dois.

## Configurações relacionadas

| Chave (`RAMSettings.ini`) | Default | Efeito |
|---|---|---|
| `Login.PersistentProfile` | `false` | "Keep browser data between uses": ligado, os perfis (login e contas) ficam entre um uso e outro. Desligado (o padrão desde 26/10/2026), todo uso começa num perfil limpo e ele some ao fechar — ver "Anônimo por padrão em tudo" |
| `Login.StealthMode` | `true` | `--lang=en-US` e injeção de `navigator.webdriver = undefined` antes do primeiro documento |
| `Login.ManualBinaryPath` | `""` | Caminho de um Chrome/Edge/Chromium/Brave próprio, digitado pelo usuário. Vazio (default) segue para o download; preenchido, vence o download e o navegador do sistema no login **e** na criação de contas (ver "Qual binário abre" acima). Vazio nunca é gravado no INI (`EMPTY_STRING_DEFAULTS` em `data/settings/store.rs`) |

Todas ficam na seção Accounts das configurações ([AccountsTab.tsx](../../src/components/settings/AccountsTab.tsx)), no grupo "Login browser". A tela de criar contas leva direto até lá ("Open login settings" → `store.openSettingsSection("accounts")`).

## Armadilhas / cuidados

- **Nunca deixe um browser com `debug = true` vivo sem estar falando com ele.** Se o CDP falhar, mate o processo em vez de devolver a janela ao usuário.
- **Nunca troque o `close_browser()` gracioso por um `kill()`** no `open_account_browser`: é o shutdown do Chromium que grava o cookie no perfil, e sem ele a fase 2 abre deslogada.
- Duas instâncias no mesmo `--user-data-dir` não viram dois processos: a segunda entrega a URL para a primeira e sai. Por isso a fase 2 só sobe depois de o processo da fase 1 ter saído de fato.
- Não use `.lock().unwrap()` no `ChromiumManager`; use o helper `lock()` do arquivo.
- Nenhum teste sobe browser de verdade. Depois de mexer neste módulo, **teste manualmente**: login pela janela, login por usuário/senha (incluindo com 2FA), e abrir o browser de uma conta já salva (tem que abrir **logada** em `roblox.com/home`), com `PersistentProfile` ligado e desligado.
- Testado também manualmente, por não dar para simular sem baixar o Chromium de verdade: apontar `Login.ManualBinaryPath` para um Chrome/Edge instalado e confirmar que ele é quem abre; apagar a conexão de rede e confirmar o fallback para o navegador do sistema (evento `chromium-fallback`); clicar "Reinstall" em Settings > Accounts e confirmar que a pasta da versão anterior não sobra em `chromium_dir`.
