# Site de divulgação

Página pública do app (recursos, download, segurança, dúvidas). HTML, CSS e JS puros — sem build, sem dependência, sem nada do app Tauri.

- `index.html` — conteúdo em inglês (o que buscador e quem está sem JS veem).
- `i18n.js` — português, espanhol e os textos montados na hora. Chave `data-i18n` nova no HTML precisa da tradução nos dois. O idioma vem de `?lang=`, depois da escolha salva no navegador, depois do idioma do navegador (pt e es; o resto cai no inglês).
- `main.js` — troca de idioma, links de download e a animação do hero.
- `styles.css` — paleta do próprio app: degradê do ícone e as cores da legenda de status.
- `assets/screens/` — fotos reais do app (em inglês, **com os nomes das contas ocultos**), usadas pela seção "Screens" e pela galeria do README. A seção é uma lista de abas no estilo da barra lateral do app: só a tela escolhida mostra a frase dela, e as setas do teclado trocam de tela. Tela nova: foto em `assets/screens/<nome>.png` (mesmo tamanho, 1586x993), um `<button class="tour-tab" data-shot="<nome>">` no `index.html` e as frases em `i18n.js`. O `assets/screenshot.png` (prévia do link no Discord/X) é a foto da tela Choose Game.

## Download sempre na versão nova

Os botões não têm versão escrita: o `main.js` lê `api.github.com/repos/luanmacea/roblox-account-manager/releases` e aponta para o `.msi` (sem `_full-nexus-ws`) da release mais recente. É a lista, e não `/releases/latest`, porque a série 0.x sai como pre-release e o `latest` do GitHub ignora pre-release. Se a API falhar (limite de 60 pedidos por hora por IP), os botões continuam levando à página de releases. **Renomear os arquivos da release quebra a escolha** — as regras estão em `pickAssets`.

## Rodar local

```bash
python -m http.server 4321 --directory site
```

Ou `preview_start` com `site` (está no `.claude/launch.json`).

## Publicação

O workflow [site.yml](../.github/workflows/site.yml) copia esta pasta para a branch `gh-pages` a cada push em `main` que mexe aqui (até 03/10/2026 era a `develop`; o site passou a acompanhar só o que já foi publicado). O GitHub Pages serve essa branch em **https://luanmacea.github.io/roblox-account-manager/** — grátis, sem servidor e sem domínio pago. Mudança só no site não dispara release (o `release-v4.yml` ignora `site/**`).

Ativar uma vez: no GitHub, **Settings → Pages → Build and deployment → Source: Deploy from a branch → `gh-pages` / `(root)` → Save**.

O endereço principal é **https://multialt.pages.dev/** (Cloudflare Pages, grátis), desde a troca de nome para MultiAlt (03/10/2026). O projeto `multialt` do Cloudflare está ligado a este repositório: branch de produção `main`, sem comando de build, pasta de saída `site`. Ele publica sozinho a cada push em `main`, sem passar pelo workflow (um push em `develop` vira no máximo uma prévia, num endereço próprio, nunca o site principal) — o `site.yml` só alimenta o endereço do GitHub Pages.

O projeto antigo, `roblox-account-manager-app` (https://roblox-account-manager-app.pages.dev/), continua ligado ao repositório com a mesma configuração e serve o mesmo site, para links antigos não quebrarem. Os três endereços servem o mesmo site; o `<link rel="canonical">` aponta para o do Cloudflare, para o buscador juntar os dois num resultado só. Todos os caminhos do site são relativos para funcionar nos dois.

## Buscadores (SEO)

O objetivo é aparecer para quem pesquisa "roblox account manager". O que o site já faz por isso:

- `<title>` e um trecho do `<h1>` com o nome exato ("Roblox Account Manager"), mais a descrição em `meta description`.
- Dados estruturados (`application/ld+json`, tipo `SoftwareApplication`) no `index.html`: nome, sistema, grátis, licença e link de download. Ao mudar o link do MSI ou a licença, atualize ali também.
- `robots.txt` e `sitemap.xml` reais. Sem eles, o Cloudflare respondia a página inicial no lugar dos dois (qualquer endereço inexistente caía no `index.html`), e o Google não recebia o mapa do site. Pelo mesmo motivo existe o `404.html`: endereço que não existe responde 404 de verdade.
- `og:image` com a captura do app, para o link ficar com prévia grande no Discord, no Reddit e no X.
- Uma página só, com o idioma trocado por JavaScript: o Google indexa a versão em inglês. Versões em português e espanhol com endereço próprio (`/pt/`, `/es/`) são o próximo passo, se quisermos aparecer para "gerenciador de contas roblox".

Fora do código, o que mais pesa é o Google descobrir o site e outros lugares apontarem para ele — o plano está em [docs/seo.md](../docs/seo.md).
