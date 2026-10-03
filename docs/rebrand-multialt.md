# Troca de nome: Roblox Account Manager → MultiAlt

Decidido em 03/10/2026. Motivo: "roblox account manager" é um termo disputado (o GitHub do RAM original, o DevForum, sites de download), e um nome próprio pega a primeira página para si mesmo. "MultiAlt" foi validado antes da troca:
- no Google não há ferramenta de jogo com o nome;
- no GitHub não há nada de Roblox com ele;
- os domínios `.app` e `.gg` estão livres.

Os nomes descartados, e por quê, estão no histórico da conversa: MultiBlox já existe como concorrente direto; MultiRoblox e MultiRBX são nomes ocupados e carregam a marca Roblox.

**Frase de descrição:** "MultiAlt — Multi Roblox & Account Manager". O termo genérico continua na descrição, para disputar "multi roblox" e "roblox account manager" sem usar o nome de ninguém na marca.

## O que muda

| Onde | De | Para |
|---|---|---|
| README (en/pt), site, descrição do GitHub | Roblox Account Manager / RAM | MultiAlt (com a frase de descrição) |
| Janela, barra de título, ícone da bandeja, mensagens do app | Roblox Account Manager | MultiAlt |
| Nome do produto no instalador: menu Iniciar, atalho, "Aplicativos" do Windows, nome dos arquivos da release | Roblox Account Manager / `Roblox.Account.Manager_…` | MultiAlt / `MultiAlt_…` |
| Pasta onde o MSI instala o programa | `%LOCALAPPDATA%\Programs\Roblox Account Manager` | `%LOCALAPPDATA%\Programs\MultiAlt` |
| MSI de nome fixo (botão do README) | `Roblox-Account-Manager-Setup.msi` | `MultiAlt-Setup.msi` |
| Título da release | `Roblox Account Manager vX` | `MultiAlt vX` |
| Endereço do site | `roblox-account-manager-app.pages.dev` | `multialt.pages.dev`; o antigo continua no ar com o canônico apontando para o novo |

## O que **não** muda (identidade interna — trocar quebra)

| O quê | Por quê fica |
|---|---|
| Pasta de dados `%LOCALAPPDATA%\Roblox Account Manager` (`APP_DIR_NAME`) | é onde estão as contas, as configurações e os backups de quem já usa. Mudar = app "vazio" depois de atualizar. |
| Seção `[Roblox Account Manager]` do `RAMSettings.ini` | o arquivo é lido por esse nome, e também pelo do RAM original na importação |
| Cabeçalho do `AccountData.json` (`RAM_HEADER`, `crypto.rs`) | é o formato do arquivo criptografado; trocar tranca as contas |
| Detecção do RAM antigo (`Roblox Account Manager.exe`, mensagem "legacy") | é o nome **do outro app**, que continua existindo |
| Identificador `com.luanmacea.roblox-account-manager` | o WebView2 guarda o `localStorage` por ele, e o updater e o instalador dependem dele |
| Nome do executável `roblox-account-manager.exe` | os atalhos, a entrada de iniciar com o Windows e as exceções de antivírus/firewall apontam para ele |
| ~~Repositório `luanmacea/roblox-account-manager`~~ — **renomeado para `luanmacea/MultiAlt` na 1.0.0** (03/10/2026, pedido do dono, para a busca por "MultiAlt" achar o projeto) | o GitHub redireciona páginas, downloads e `git`, mas **não** `raw.githubusercontent.com`, de onde o updater lê o manifesto: quem estava numa versão anterior à 1.0.0 parou de receber atualização automática (na época não havia usuários além do dono). A 1.0.0 já sai lendo o endereço novo ([updater.rs](../src-tauri/src/commands/updater.rs), [repo.ts](../src/repo.ts)). |
| **Código de atualização do MSI** `ea97e4e5-4634-554b-8c91-822b0b369761` | o Tauri o calcula a partir do nome do produto. Trocar o nome sem fixá-lo faria a versão nova instalar **ao lado** da antiga, em vez de atualizar. Fixado em `bundle.windows.wix.upgradeCode`. |
| Nome da entrada "iniciar com o Windows" | o plugin de autostart usa o nome do produto como nome da entrada no registro. Fixado em "Roblox Account Manager" (`app_name`), senão quem tem a opção ligada ganharia uma segunda entrada. |

## A atualização que troca o nome (instalador)

Na atualização de uma versão "Roblox Account Manager" para a "MultiAlt":

1. o mesmo código de atualização faz o MSI reconhecer a versão antiga, instalar a nova (na pasta nova) e remover a antiga;
2. o atalho da área de trabalho e o do menu Iniciar mudam de nome. Arquivo com outro nome é outro arquivo, então **nesta única atualização** o ícone vai para outra posição. O template remove o `.lnk` antigo e cria o novo; nas atualizações seguintes vale de novo a regra de não mexer no atalho (mudança 6 do template, em docs/development.md);
3. dados, contas e configurações ficam onde estão, porque a pasta de dados não muda.

## Teste (03/10/2026, no PC do dono)

Atualização silenciosa, do jeito que o updater roda (`msiexec /i … /quiet AUTOLAUNCHAPP=True`), de "Roblox Account Manager 0.1.8" para um MSI "MultiAlt" local:

- **dados:** os 7 arquivos da pasta `%LOCALAPPDATA%\Roblox Account Manager` (contas, configurações, avatares, scripts…) com o hash idêntico antes e depois;
- **atalhos:** `MultiAlt.lnk` na área de trabalho e no menu Iniciar, apontando para `Programs\MultiAlt\roblox-account-manager.exe`, que existe. Os `.lnk` antigos sumiram;
- **pastas:** `Programs\MultiAlt` criada e `Programs\Roblox Account Manager` removida;
- **Aplicativos:** um produto só, "MultiAlt 0.1.8";
- **app:** reabriu sozinho;
- **registro:** a chave `HKCU\Software\Luan Silveira\Roblox Account Manager` saiu junto. Por isso as atualizações seguintes (MultiAlt → MultiAlt) voltam a não mexer no atalho.

## Ordem de execução

1. Marca e textos: README, site, app, release, documentação.
2. Instalador: nome do produto, código de atualização e autostart fixados, troca dos atalhos e teste de atualização real.
3. Site no endereço novo (Cloudflare) e Search Console do endereço novo.
4. Publicar na `main` (com o dono de acordo) e conferir a release.
