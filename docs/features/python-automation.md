# Automação do Roblox Player em Python

> Documento em construção (stream R do plano
> `docs/superpowers/plans/2026-10-21-python-automation.md`).

## Spike P0 (21/10/2026)

**Provado:**

- `PrintWindow(hwnd, hdc, PW_CLIENTONLY | PW_RENDERFULLCONTENT)` numa janela
  Chromium (classe `Chrome_WidgetWin_1`, renderização por GPU) **atrás** de
  outra janela devolve pixels reais: três janelas de 2560×1392, todas fora do
  primeiro plano, saíram com 100% dos pixels não pretos. Teste manual
  `python_capture_tests::spike_printwindow_on_a_background_gpu_window`
  (`cargo test --all-features --lib python_capture_tests -- --ignored --nocapture`);
  ele imprime só a classe, o tamanho e a contagem, nada do conteúdo da tela.
- A memória compartilhada nomeada (`CreateFileMappingW`, nome
  `Local\multialt-cap-...`) é lida pelo `mmap.mmap(-1, n, tagname=nome)` do
  Python 3.12.10 byte a byte (`python_capture_tests::python_mmap_reads_the_shared_frame`,
  roda na suíte e pula quando não há `python` no PATH).

**Não provado:** a captura num cliente **Roblox** de verdade — não havia
cliente aberto no PC durante o spike (e nenhuma conta foi aberta para isso). O
Roblox desenha por Direct3D como o Chromium; se o `PrintWindow` vier preto nele
com a janela na frente, o código já cai para a cópia da tela
(`capture_client_bgra`). Com a janela atrás de outra e preta, o erro sobe como
`capture_failed` — conferir no primeiro teste com cliente real.
