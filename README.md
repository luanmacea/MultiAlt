Full credit to [ic3w0lf22](https://github.com/ic3w0lf22) for the original Roblox Account Manager. This is a continuation of the project.

# Roblox Account Manager
![github-large](Images/Image5.png)

**[Baixar a versão mais recente](https://github.com/luanmacea/roblox-account-manager/releases/latest)**

[![Latest Release](https://img.shields.io/github/v/release/luanmacea/roblox-account-manager?include_prereleases&label=Latest%20Release)](https://github.com/luanmacea/roblox-account-manager/releases/latest)

Desktop app para gerenciar múltiplas contas Roblox: adicionar contas, rodar vários clientes ao mesmo tempo, alternar entre alts sem trocar de login, e automatizar o rejoin (Auto Rejoin).

Reporte bugs na aba Issues ou via Discord @niccdev.

# ⚠️ Aviso
Nunca gere um "rbx-player link" a pedido de terceiros — quem tiver esse link pode entrar em qualquer jogo (ou até o Roblox Studio) usando sua conta, gastar seu Robux, ou causar banimento.

# RAM v4 (Beta)
Reescrita em Rust + TypeScript com [Tauri](https://tauri.app/). Em desenvolvimento ativo — espere bugs e mudanças de comportamento entre versões beta. Para estabilidade máxima, use o release legado.

---

# Desenvolvimento

```bash
# instalar dependências do frontend
bun install

# rodar o app em modo dev (hot-reload, sem gerar executável)
bun run tauri dev

# gerar o executável de produção (instalador/portable)
bun run tauri build
```

Outros comandos úteis:

```bash
bun run dev              # só o frontend (Vite), sem a janela nativa Tauri — não tem acesso aos comandos IPC do backend
bun run build             # type-check + bundle do frontend, sem empacotar o app
bun run preview           # preview do build do frontend

bun scripts/i18n/extract-keys.ts   # extrair chaves de tradução (i18n)

cd src-tauri && cargo build                        # compilar só o backend Rust
cd src-tauri && cargo build --features webserver,nexus   # com as features opcionais (API HTTP local + Nexus WebSocket)
```

A primeira execução de `tauri dev`/`tauri build` compila todas as crates Rust do zero e pode levar alguns minutos; builds seguintes usam cache incremental do Cargo e são bem mais rápidas.

`bun run tauri build` gera o instalador em `src-tauri/target/release/bundle/nsis/` (o `tauri.conf.json` só pede `nsis`; o `.msi` só sai no CI, com `PUBLISH_MSI` ligado). O `.exe` solto (sem installer) fica direto em `src-tauri/target/release/`.

# Download
Releases prontas: [GitHub Releases](https://github.com/luanmacea/roblox-account-manager/releases).

A release publica **só o instalador** (`.exe`), em duas variantes:
- sem `_full-nexus-ws` = **recomendado** (não abre porta nenhuma)
- com `_full-nexus-ws.exe` = versão full-feature (API HTTP local e Nexus)

Não precisa instalar .NET Framework ou VC++ manualmente. Baixe só da página oficial de releases.

# Documentação da API
[Docs aqui](https://ic3w0lf22.gitbook.io/roblox-account-manager/).

# Principais Features
| Feature | Descrição |
| :--- | :--- |
| Add Accounts | Username/cookie, browser login, ou import de cookie/AccountData |
| Multi Roblox | Rodar vários clientes ao mesmo tempo (ativar manualmente em Settings) |
| Pre-launch Isolation | Limpa cache/registro/prefetch antes de cada launch (Windows) |
| Version Manager | Instala builds do Roblox lado a lado; a versão escolhida vale para todas as contas (por conta, só editando o campo `RobloxVersion` da conta) |
| Multi Launch | Lançar várias contas de uma vez com Place/Job/Launch Data |
| VIP/Private Server | Entrar via Job ID com `vip:` ou link |
| Server List | Navegar servidores, jogos, favoritos e recentes |
| Auto Rejoin | Relança as contas alt em intervalo fixo; as contas main abrem uma vez e ficam, e dá para desconectar uma conta do ciclo |
| Watcher | Fecha clientes por timeout, memória, ou detecção de beta |
| Local Web API / Nexus | API HTTP local e WebSocket para scripts externos (Nexus.lua) |
| Script Manager | Scripts JS custom com acesso a comandos Rust, HTTP/WebSocket, UI |
| Themes + i18n | Editor de tema embutido, localização via Crowdin |

# FAQ

**Por que é detectado como vírus?**
Falso positivo comum em ferramentas que automatizam launch de processos/jogos. Código é público (Rust + Tauri) e pode ser auditado/compilado por você mesmo. Baixe só do GitHub oficial.

**Como ativo multi-roblox?**
Settings → `General` → `Multi Roblox` (com o Roblox fechado).

**Por que multi-roblox vem desativado?**
A Byfron já declarou que múltiplos clientes pode ser visto como comportamento suspeito — ative por sua conta e risco.

**Posso ser banido por usar isso?**
Não viola o ToS do Roblox, mas alguns jogos podem proibir alts — pesquise antes.

**Como faço backup das minhas contas?**
Pelo próprio app: Settings → `Misc` → `Data` → `Backups` → `Manage` cria, lista e restaura backups (detalhes em [docs/features/backups.md](docs/features/backups.md)). O zip leva o `AccountData.key` junto — sem ele o backup não restaura —, então, **sem senha no app, quem tiver o zip consegue abrir as contas**. Se você guarda backup em nuvem, ponha senha antes (Settings → `Misc` → `Security` → `Change Encryption Method` → `Open` → `Pass Lock`). Não use o RAMDecrypt para "descriptografar e salvar": ele é do app antigo, não conhece o `AccountData.key`, e o que ele entregaria é um arquivo com os cookies em texto puro.

**O app abriu com a janela em branco (ou preta). E agora?**
A interface é desenhada pelo Microsoft Edge WebView2 Runtime, então reinstalar o app não resolve. O app tenta se recuperar sozinho: se a interface não aparecer em 25 s, ele avisa e reabre com a aceleração de vídeo desligada. Para forçar esse modo, **segure Shift** enquanto o app abre (ou acrescente `--safe-mode` ao campo *Destino* do atalho). Se continuar em branco, repare o **Microsoft Edge WebView2 Runtime** em Configurações do Windows → Aplicativos → Aplicativos instalados → Modificar → Reparar, reinicie o Windows e atualize o driver de vídeo. Detalhes em [docs/features/webview-recovery.md](docs/features/webview-recovery.md).

**O app está no "modo de vídeo seguro". Como volto ao normal?**
Quando o modo está ligado aparece uma faixa amarela no topo do app com o botão **Voltar ao modo normal** — ele apaga o marcador e reabre o app com a aceleração de vídeo de volta. Se a faixa não aparecer, ou o botão falhar, apague na mão o arquivo `webview.safemode` que fica junto do `RAMSettings.ini` (por padrão em `%LOCALAPPDATA%\Roblox Account Manager`) e abra o app de novo. Se você estiver segurando Shift, solte antes de abrir: Shift força o modo por essa sessão.

**Funciona no Mac?**
Ainda não — suporte parcial, chegando com a reescrita v4.

# Preview
![github-large](Images/Image5.png)
