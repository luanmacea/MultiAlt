# AFK mode por clique — especificação

Aprovada pelo dono em 29/09/2026. Complementa [docs/features/afk-mode.md](../../features/afk-mode.md).

## Problema

O AFK mode só envia tecla, e as teclas da lista mexem no personagem (andar, pular, usar item). Quem quer a conta **parada** no lugar não tem opção. Um clique numa área vazia do jogo também conta como atividade para o Roblox e não move o personagem.

## Comportamento

- Seletor **Enviar: Tecla | Clique**, global para a sessão. O modo tecla fica exatamente como hoje.
- No modo clique, cada conta clica num **ponto relativo** à área interna da janela dela, em porcentagem (0–100% × 0–100%). O mesmo ponto cai no mesmo lugar relativo com a janela pequena, grande ou maximizada. **Nenhuma janela é redimensionada.**
- **Ponto padrão** para todas as contas (default 50% × 50%). Qualquer conta pode ter **ponto próprio**; sem ele, usa o padrão.
- **Marcar ponto:** o usuário clica em "Marcar", tem **3 s** para parar o mouse em cima do ponto numa janela de conta, e o app lê a posição do cursor **uma vez**, descobre a janela embaixo dela e converte para porcentagem. Existe um Marcar para o ponto padrão e um por conta.
- "Enviar agora" (já existe) dispara um ciclo no modo escolhido: é como se testa o clique.

## Ciclo no modo clique (por conta)

Igual ao modo tecla até o foco: resolve PID → confere que é Roblox → acha a janela → traz para frente → **confirma que está em primeiro plano** (sem isso, nada é enviado e a conta fica com `focusDenied`). Depois:

1. lê a posição atual do cursor do usuário;
2. calcula o pixel na tela a partir da porcentagem e da área interna **atual** da janela;
3. move o cursor até lá, clique esquerdo (pressiona, 40 ms, solta; o "solta" com até três tentativas, como a tecla);
4. devolve o cursor para onde estava.

Re-minimizar e devolver o foco seguem as regras de hoje.

## Regras de segurança

A trava do AFK continua valendo, com **uma porta estreita** a mais:

- Injeção de mouse só por **uma** função pública em `platform/windows/input.rs`, que recebe janela + porcentagem. Não existe chamador com coordenada de tela crua.
- Só **botão esquerdo**, clique simples, e só **dentro da área interna da janela alvo**: a porcentagem é travada em 0–100 e o pixel calculado nunca sai do retângulo da janela.
- O `afk_input_safety_tests` passa a permitir `INPUT_MOUSE` **só em `input.rs`**, e o `only_the_input_module_sends_input` conta também a porta do clique. Continuam proibidos: gancho global, estado de tecla **e de botão** (`GetAsyncKeyState` etc.), entrada crua, `mouse_event` e `keybd_event`.
- O **Marcar lê só a posição do cursor** (`GetCursorPos`) — nunca botão, nunca teclado. Ler a posição também é necessário para devolver o cursor depois do clique.
- O Marcar só aceita janela de **conta aberta pelo app** (PID da janela bate com um PID do tracker). Qualquer outra janela é recusada com mensagem.

## Armazenamento

| Onde | Chave | Default |
|---|---|---|
| INI | `Afk.Mode` | `key` (`key` ou `click`) |
| INI | `Afk.ClickX`, `Afk.ClickY` | `50`, `50` (porcentagem) |
| Campos da conta | `AfkClickX`, `AfkClickY` | ausente = usa o padrão |

## Fora de escopo

Botão direito, duplo clique, vários pontos por conta, redimensionar janelas, e prévia visual (foto da janela).

## Testes

- Conversão porcentagem ↔ pixel com janelas de tamanhos diferentes; porcentagem fora de 0–100 travada; pixel nunca fora do retângulo.
- Escolha do ponto: conta com ponto próprio usa o dela, sem ponto usa o padrão.
- Marcar: janela de conta vira porcentagem; janela que não é de conta é recusada.
- Trava de segurança: `INPUT_MOUSE` fora de `input.rs` reprova; ler estado de botão reprova; contagem de call sites inclui o clique.
- Tela: seletor de modo, Marcar com contagem regressiva, ponto por conta com "usar o padrão", start bloqueado no modo tecla sem tecla (o modo clique não precisa de tecla).
- `SendInput` e janela de verdade ficam fora de teste (precisam de cliente Roblox aberto): o dono valida.
