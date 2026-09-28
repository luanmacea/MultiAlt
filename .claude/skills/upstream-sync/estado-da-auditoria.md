# Estado da auditoria do upstream

Onde a última passada parou. A próxima execução da skill audita a janela
`<último commit auditado>..v4` — o que está aqui já foi decidido.

## Ponto de fork

- Upstream: `niccsprojects/Roblox-Account-Manager`, branch `v4`
- Commit de origem: **`ddcb4e48`** — "Merge pull request #63 …", 2026-06-08
- Árvore: `a4feb866a82d769add3c720ea180ec6ad8c65878`, **idêntica** ao commit raiz
  deste fork (`305fcb2`, "primeiro commit seguro", 2026-06-24)

Não há ancestral comum no git: o fork foi feito copiando arquivos. O ponto acima
foi achado comparando hashes de blob (ver `scripts/find-fork-point.sh`).

## Passada de 2026-09-26

- Janela auditada: `ddcb4e48..v4`
- Último commit upstream auditado: **`e737c52`** (2026-09-23)
- 84 commits (68 sem merges), 84 arquivos, +6798 −1245

### Triagem de segurança: limpa

Nenhum host novo fora de Roblox/GitHub/shields.io/ko-fi/developer.microsoft.com
(os três últimos só no README); nenhum log de cookie, senha ou chave; nenhuma
leitura de teclado ou hook (`input.rs` só **envia** teclas, de uma lista de 14, e
`GetAsyncKeyState(VK_SHIFT)` é o atalho de safe mode do WebView); nenhuma
ofuscação; dependências novas são conhecidas (`@dnd-kit`, `tauri-plugin-dialog`);
CI mudou uma linha (`prerelease: false`). A única escrita em `HKLM` é opt-in
(`create_restore_point`, default `false`) e cria um ponto de restauração antes do
spoof de hardware, devolvendo a chave ao valor anterior.

### Decisões

Preenchido conforme o dono aprova cada item. Item recusado fica recusado; se o
upstream mexer nele de novo, reavaliar só aí.

| Tema | Categoria | Veredito | Onde ficou |
|---|---|---|---|
| (a preencher) | | | |
