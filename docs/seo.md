# Aparecer no Google para "roblox account manager"

Plano montado em 03/10/2026. Site: **https://roblox-account-manager-app.pages.dev/** (código em `site/`, ver [site/README.md](../site/README.md)).

## Onde estamos

- **O Google ainda não conhece o site.** A busca `site:roblox-account-manager-app.pages.dev` não devolve nada: sem página indexada, o site não aparece para pesquisa nenhuma, nem para o próprio nome.
- **A primeira página de "roblox account manager"** (03/10/2026) tem:
  - o GitHub do RAM original (ic3w0lf22), duas vezes: releases e repositório;
  - o DevForum do Roblox;
  - sites de terceiros (betterroblox.com, rbxaccountmanager.com, download.it);
  - o espelho no SourceForge;
  - um tópico no Reddit.

  Quase todos ganham por **autoridade de domínio** (GitHub, Reddit, SourceForge) ou por estarem lá há anos. Site novo, num subdomínio `pages.dev`, sem nenhum link apontando para ele, não entra nessa lista de um dia para o outro.
- **Expectativa honesta:** indexação em dias depois do Search Console. Para o termo exato na primeira página, conte meses, e só com links de fora. As vitórias rápidas vêm antes por buscas mais específicas ("roblox account manager open source", "roblox account manager msi", "gerenciador de contas roblox") e pelo **repositório no GitHub**, que já nasce com autoridade e pode subir junto do original.

## Já feito no site (03/10/2026)

- Título e `<h1>` com o nome exato, e descrição.
- Dados estruturados `SoftwareApplication` (ld+json).
- `robots.txt`, `sitemap.xml` e `404.html` reais. Antes, o Cloudflare devolvia a página inicial no lugar deles.
- Imagem de prévia grande (`og:image`).
- O site agora é publicado a partir da `main` (Cloudflare e GitHub Pages).

## Próximos passos, na ordem de impacto

### 1. Fazer o Google e o Bing descobrirem o site (dias)

1. **Google Search Console** (https://search.google.com/search-console), com a conta Google do dono:
   1. adicionar a propriedade do tipo **prefixo de URL** com `https://roblox-account-manager-app.pages.dev/`;
   2. verificar por **tag HTML**: o Google dá uma `<meta name="google-site-verification" ...>`, que entra no `<head>` do `site/index.html` e é publicada pela `main`;
   3. em **Sitemaps**, enviar `sitemap.xml`;
   4. em **Inspeção de URL**, colar a página inicial e pedir **Solicitar indexação**.
2. **Bing Webmaster Tools** (https://www.bing.com/webmasters). Dá para importar a propriedade direto do Search Console, sem verificar de novo. O Bing também alimenta o DuckDuckGo e o Yahoo.

### 2. Links apontando para o site (o que mais pesa)

Cada lugar abaixo é um link de domínio forte. Nada de comprar link ou spam: o Google pune.

- [ ] **GitHub, campo "Website" do repositório** (About → engrenagem → Website): o link mais valioso e o mais fácil.
- [x] README do repositório (botão e link do site, 03/10/2026).
- [ ] **Artigo no dev.to**: acrescentar o link do site no fim, ao lado do repositório.
- [ ] **AlternativeTo**: preencher o site oficial na página do RAM.
- [ ] **Posts no Reddit** (r/ROBLOXExploiting e afins, já rascunhados): link do site junto do GitHub.
- [ ] **Vídeos no YouTube** (próprio ou de criadores de tutorial): link do site na descrição.
- [ ] **SourceForge**: o espelho que aparece na primeira página é do projeto original. Um projeto nosso lá, com o site no campo "Homepage", disputa a mesma posição.

### 3. Mais páginas, cada uma respondendo uma busca (semanas)

Hoje o site é uma página só. Cada página nova é mais uma porta de entrada pelo Google:

- `/pt/` e `/es/` com o texto já traduzido em HTML. Hoje a troca de idioma é por JavaScript e o Google só vê o inglês. É o que faz o site aparecer para "gerenciador de contas roblox".
- Guias curtos com o nome no título, por exemplo:
  - "How to play on multiple Roblox accounts at once";
  - "Is Roblox Account Manager safe?" (o código aberto e o 0/75 no VirusTotal);
  - "Roblox Account Manager download (.msi)".
- Cada página nova entra no `sitemap.xml`.

### 4. Domínio próprio (opcional)

Um domínio curto (cerca de US$ 10 por ano) passa mais confiança que `pages.dev` e ajuda quem divulga. ⚠️ **Evitar "roblox" no domínio:** as diretrizes de marca da Roblox proíbem usar o nome deles em domínio de terceiros, e um aviso de marca pode derrubar o endereço. Algo como `ram-app.gg` ou `rammanager.app` é mais seguro. Se adotar, ligar o domínio no Cloudflare Pages e fazer o `pages.dev` redirecionar para ele.

## Como medir

No Search Console, em **Desempenho**: impressões e posição média para "roblox account manager" e variações. Vale olhar uma vez por semana. Posição média abaixo de 10 = primeira página.
