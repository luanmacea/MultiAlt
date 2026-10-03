# Site de divulgação

Página pública do app (recursos, download, segurança, dúvidas). HTML, CSS e JS puros — sem build, sem dependência, sem nada do app Tauri.

- `index.html` — conteúdo em inglês (o que buscador e quem está sem JS veem).
- `i18n.js` — português e os textos montados na hora. Chave `data-i18n` nova no HTML precisa da tradução aqui.
- `main.js` — troca de idioma, links de download e a animação do hero.
- `styles.css` — paleta do próprio app: degradê do ícone e as cores da legenda de status.

## Download sempre na versão nova

Os botões não têm versão escrita: o `main.js` lê `api.github.com/repos/luanmacea/roblox-account-manager/releases` e aponta para o `.msi` (sem `_full-nexus-ws`) da release mais recente. É a lista, e não `/releases/latest`, porque a série 0.x sai como pre-release e o `latest` do GitHub ignora pre-release. Se a API falhar (limite de 60 pedidos por hora por IP), os botões continuam levando à página de releases. **Renomear os arquivos da release quebra a escolha** — as regras estão em `pickAssets`.

## Rodar local

```bash
python -m http.server 4321 --directory site
```

Ou `preview_start` com `site` (está no `.claude/launch.json`).

## Publicação

O workflow [site.yml](../.github/workflows/site.yml) copia esta pasta para a branch `gh-pages` a cada push em `develop` que mexe aqui. O GitHub Pages serve essa branch em **https://luanmacea.github.io/roblox-account-manager/** — grátis, sem servidor e sem domínio pago. Mudança só no site não dispara release (o `release-v4.yml` ignora `site/**`).

Ativar uma vez: no GitHub, **Settings → Pages → Build and deployment → Source: Deploy from a branch → `gh-pages` / `(root)` → Save**.

Endereço com nome próprio, ainda de graça: criar um projeto no Cloudflare Pages ligado a este repositório (sem comando de build, pasta de saída `site`) dá `<nome-escolhido>.pages.dev`. Todos os caminhos do site são relativos justamente para funcionar em qualquer um desses endereços.
