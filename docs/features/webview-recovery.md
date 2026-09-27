# Recuperação de tela branca/preta (WebView2)

## Objetivo

A interface do app é desenhada pelo **Microsoft Edge WebView2 Runtime**. Quando o WebView2 falha — driver de vídeo, runtime atualizado no meio, composição de janela — a janela abre **em branco** (ou preta). O usuário não tem como chegar nas configurações para desligar a aceleração de vídeo, porque a tela que teria esse botão é exatamente a que não apareceu; reinstalar o app não resolve, porque não reinstala o WebView2 nem limpa o perfil dele.

Esta funcionalidade faz o app se recuperar sozinho: detecta que a interface não pintou, reabre com a aceleração de vídeo desligada e, se nem assim funcionar, explica ao usuário o que consertar.

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Decisão do safe mode, mesclagem dos argumentos do WebView2, marcador e watchdog | [webview_recovery.rs](../../src-tauri/src/webview_recovery.rs) |
| Chamada antes da janela existir + comando `frontend_painted` | [lib.rs](../../src-tauri/src/lib.rs) |
| Sinal de "pintou o primeiro quadro" | [main.tsx](../../src/main.tsx) |
| Erro de render do React não virar tela branca | [AppErrorBoundary.tsx](../../src/components/layout/AppErrorBoundary.tsx) |

O módulo Rust é **Windows-only** (`#[cfg(target_os = "windows")] mod webview_recovery;`): WebView2, `GetAsyncKeyState` e `MessageBoxW` só existem lá. O comando `frontend_painted` existe em todo SO e não faz nada fora do Windows.

## Fluxo

1. `run()` chama `prepare_environment()` **antes** de qualquer coisa do Tauri — é a última hora de mexer nos argumentos que o WebView2 vai receber.
2. Ele decide o safe mode (`decide_safe_mode`), apaga o marcador se ele estiver velho e grava `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` **mesclando** o que já estava na variável (`merge_browser_arguments`).
3. No `setup`, `start_watchdog` dispara uma thread que dorme 25 s.
4. O frontend, dois `requestAnimationFrame` depois do render, chama `frontend_painted` → `mark_painted()`.
5. Passados os 25 s sem esse aviso:
   - **já estava em safe mode** → aviso nativo com o passo a passo de reparo do WebView2, e o app **não** reabre (reabrir seria um laço);
   - **não estava** → grava o marcador com a versão do runtime, avisa que vai reabrir em safe mode e chama `app.restart()`;
   - **marcador não pôde ser gravado** → aviso de reparo, sem reabrir (sem marcador o safe mode não sobreviveria ao reinício, então reabrir só repetiria a tela em branco).

## Regras de negócio

- **Safe mode de vídeo** = `--disable-gpu --disable-gpu-compositing`. Liga por qualquer um destes:
  - `--safe-mode` na linha de comando (dá para pôr no campo **Destino** do atalho do Windows);
  - variável `RAM_WEBVIEW_SAFE_MODE` com `1`, `true`, `yes` ou `on` (sem diferenciar maiúsculas, espaços nas pontas ignorados);
  - **Shift segurado** enquanto o app abre;
  - marcador em disco do boot anterior.
- **O marcador é preso à versão do runtime.** `webview.safemode` fica ao lado do `RAMSettings.ini` (pasta de dados do usuário) e guarda a versão do WebView2 que falhou. Na abertura seguinte:
  - versão igual → safe mode ligado;
  - **versão diferente, ou arquivo vazio → o marcador é apagado e o safe mode não liga.** Sem isso o app ficaria degradado para sempre, muito depois de o WebView2 ter sido corrigido pela atualização que o próprio Windows aplica.
  - Um marcador velho é apagado mesmo quando o safe mode veio forçado por outro motivo: a informação dele não vale mais de qualquer jeito.
- **A mesclagem não perde nem duplica.** O que já estava em `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` é preservado (o usuário ou um lançador externo pode ter posto algo lá), vários `--disable-features=` são juntados numa lista só, e nenhuma flag nossa entra duas vezes.
- **`mark_painted` só apaga o marcador quando o boot não estava em safe mode.** Ter pintado com a GPU desligada não prova que ligá-la de novo funciona — a prova errada custaria outra tela em branco. Quem tira o app do safe mode é a troca de versão do runtime (ou apagar o arquivo na mão).
- **O watchdog não roda em build de debug.** Em `bun run tauri dev` o frontend vem do servidor do Vite e pode demorar (ou estar caído) por motivos que não têm nada a ver com o WebView2; reabrir o app no meio de uma sessão de desenvolvimento só atrapalharia.
- **O aviso ao usuário é `MessageBoxW`** (`windows-sys`, já dependência do projeto), não um diálogo do Tauri nem um toast da store: quando esse aviso é necessário, o WebView2 é justamente a peça que não funciona, então qualquer aviso desenhado pelo frontend apareceria na mesma tela em branco que se está tentando consertar. Por isso também não foi adicionado o `tauri-plugin-dialog` que o upstream usa.
- **Erro de render do React não é falha do WebView2.** O `AppErrorBoundary` envolve a árvore inteira (inclusive a `StoreProvider`) e desenha uma tela de erro com a mensagem e um botão de recarregar. Como ele desenha algo, o sinal de pintura chega e o app **não** reabre em safe mode por causa de um bug nosso.

## Configurações relacionadas

Nenhuma chave de `RAMSettings.ini`. Tudo é decidido por linha de comando, ambiente, teclado e o marcador — de propósito: se dependesse das settings, um app que não abre não teria como mudar a configuração que o faria abrir.

## Armadilhas / cuidados

- `prepare_environment()` roda antes de `crypto::init()` e já resolve a pasta de dados (`get_settings_path`). Se algum dia essa resolução passar a depender de algo inicializado depois, esta chamada tem que ser reavaliada.
- `std::env::set_var` depois de a janela existir não tem efeito nenhum: o WebView2 lê a variável na criação. Qualquer flag nova entra em `prepare_environment`.
- O prazo de 25 s é o teto de uma máquina lenta, não uma média. Encurtar troca "usuário esperando" por "app reabrindo sozinho sem motivo".
- As mensagens do `MessageBoxW` ficam em inglês: o backend não tem i18n, e o catálogo do frontend não está carregado quando o frontend é o que falhou.

## Testes

`webview_recovery_tests` (suíte `ui`) cobre a parte pura: a decisão do safe mode a partir de (argumento, variável, Shift, marcador, versão do runtime) e a mesclagem dos argumentos do browser. `GetAsyncKeyState`, `MessageBoxW`, disco e `app.restart()` ficam fora do teste. `AppErrorBoundary.test.tsx` cobre a tela de erro do React.
