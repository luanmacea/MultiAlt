# macOS (branch `macos`, desde 04/11/2026)

Trabalho isolado na branch `macos` (pedido do dono, 04/11/2026), que nasceu da
`linux` porque as duas dividem o código de "Unix com launch"
(`platform::unix_host`, `run_launch_queue_unix`). Nada daqui vai para a
`develop` até alguém testar num Mac de verdade — o dono não tem Mac; o build
sai do CI e vai para um testador.

## Por que é mais fácil que o Linux

O Roblox tem **cliente nativo** para macOS (`/Applications/Roblox.app`), então
o launch é o mesmo do Windows: o link `roblox-player:` com o ticket da conta,
aberto pelo `open`. Não há Sober, perfil por conta nem rede isolada. O código
de macOS veio do projeto original (`src-tauri/src/platform/macos/`: launch,
Multi Roblox, rastreio de processos, logs, patch de settings do cliente) e
**nunca tinha sido compilado nem testado** por nós.

## O que mudou nesta branch

- **Multi Roblox abria a 2ª conta no cliente da 1ª.** `open <link>` entrega o
  link ao Roblox já aberto (Launch Services), e o `open -n -a <Roblox.app>` só
  existia como plano B depois de um `spawn` que sempre dava certo — nunca
  rodava. Agora, com o Multi Roblox ligado e o app achado, vai direto por
  `open -n -a` (instância nova); se isso falhar, tenta o link puro (regra nº 1).
  Decisão pura em [mac_open.rs](../../src-tauri/src/platform/mac_open.rs)
  (`mac_open_tests`, suíte `macos`, roda no Windows). Antes de cada launch com o
  Multi Roblox ligado o app apaga o semáforo `/RobloxPlayerUniq` (código
  herdado, `core.rs`).
- **Tela**: o macOS manda `false` nas capacidades do que é Win32 (Live, AFK,
  isolamento, versões, otimização, clientes do site, Python), como o Linux, e a
  UI esconde ([platform_info.rs](../../src-tauri/src/commands/platform_info.rs)).
  Ficam o toggle do Multi Roblox (Settings › Launch), o patch de settings do
  cliente e o watcher. Conferir no navegador: `bun run dev:ui` e
  `?scenario=macos`.
- **Atualização**: canal próprio `<canal>-macos` (`PLATFORM_CHANNEL_SUFFIX`,
  [updater.rs](../../src-tauri/src/commands/updater.rs)). Sem isso o Mac lia o
  manifesto do Windows, que não tem entrada `darwin-*`. Ainda **não há** job de
  release que publique esses manifestos: o app de teste não acha atualização.
- **CI** (só nesta branch): `macos-check.yml` (`cargo check` das duas edições
  com `--tests` e `cargo test --all-features`, num `macos-14`) e
  `macos-build.yml` (um `.dmg` universal — Apple Silicon e Intel — de cada
  edição, como artefato do run, 14 dias). Daqui do Windows não dá para
  compilar para Mac: `ring` e `objc2-exception-helper` precisam do SDK da Apple
  (tentado no Docker com clang em 04/11/2026).

Ler o resultado do CI sem `gh` (mesmo esquema do Linux):

```bash
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/actions/runs?branch=macos&per_page=4"
curl -s "https://api.github.com/repos/luanmacea/MultiAlt/check-runs/<job>/annotations"
```

## Assinatura

Sem conta de desenvolvedor da Apple (US$ 99/ano) o app sai com assinatura
**ad-hoc** (`signingIdentity: "-"`) e sem notarização. O Gatekeeper diz que o
app "está danificado e não pode ser aberto". O testador resolve com
`xattr -cr /Applications/MultiAlt.app` (passo 2 do roteiro). Para publicar de
verdade: conta da Apple, certificado "Developer ID Application" nos segredos do
CI e notarização (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` no `tauri build`).

## Pendente (nada rodou num Mac)

- O CI compilar e passar os testes (primeiro run em 04/11/2026).
- Se `open -n -a` com o link realmente entrega o link à instância nova (é o que
  o `open` documenta, mas o Roblox pode ignorar e repassar à instância antiga).
- Se o Roblox atual ainda usa o semáforo `/RobloxPlayerUniq` para barrar a
  segunda instância.
- O aviso de "Roblox.app não achado" vai para o stderr, não para o Console do
  app (o `launch_url` não tem o `AppHandle`).
- Pasta de dados, chave do aparelho (`AccountData.key` sem DPAPI no Mac: fica
  só com a permissão do arquivo), bandeja, atalho de inicialização.

## Roteiro para o testador

Precisa: Mac com macOS 11 ou mais novo, o Roblox instalado pelo site
(`/Applications/Roblox.app`) e **duas contas de teste** (nunca a conta
principal de ninguém).

1. Baixar o `.dmg` (artefato `multialt-macos-standard` do run "macOS build"),
   abrir e arrastar o MultiAlt para Aplicativos.
2. No Terminal: `xattr -cr /Applications/MultiAlt.app` e abrir o app.
   *Esperado:* abre a tela de primeiro uso. *Anotar:* se pediu permissão de
   algo (Acessibilidade, Rede local...).
3. Adicionar a conta 1 (login pelo navegador do app ou cookie). *Esperado:*
   aparece com avatar e nome.
4. Escolher um jogo e clicar em Jogar com a conta 1. *Esperado:* o Roblox abre
   já logado na conta 1 e entra no jogo. *Anotar:* o que o Console do app
   mostrou.
5. Settings › Launch: ligar o **Multi Roblox**. Com a conta 1 ainda no jogo,
   jogar com a conta 2. *Esperado:* abre uma **segunda janela** do Roblox na
   conta 2 e a conta 1 continua no jogo. *Anotar:* se a conta 1 caiu, se a
   segunda janela não abriu, ou se a conta 1 trocou de jogo.
6. Com o Multi Roblox **desligado**, repetir o passo 5. *Esperado:* só uma
   conta fica aberta (é o comportamento do Roblox; serve de comparação).
7. Entrar por um servidor privado (link `share?code=` ou `privateServerLinkCode`).
   *Esperado:* entra no servidor privado.
8. Fechar uma conta pelo app (menu da conta › fechar). *Esperado:* fecha só
   aquela janela.
9. Fechar o MultiAlt com as duas contas abertas. *Esperado:* as duas continuam
   no jogo.
10. Mandar: prints de cada passo que falhou, a versão do macOS, se é Apple
    Silicon ou Intel, e a pasta `~/Library/Logs/Roblox` compactada (logs do
    cliente — **não** mandar a pasta de dados do MultiAlt, que tem os cookies).
