// Cloudflare Pages: os endereços antigos *.pages.dev redirecionam (301) para os
// domínios próprios, para o Google trocar o resultado e parar de mostrar
// "Cloudflare" como nome do site. Português vai para multialt.com.br, o resto
// para multialt.net. Os arquivos de verificação de buscadores ficam no ar
// (o Search Console precisa deles para a mudança de endereço).
const OLD_HOSTS = new Set(["multialt.pages.dev", "roblox-account-manager-app.pages.dev"]);
const KEEP = /^\/(google[0-9a-f]+\.html|[0-9a-f]{32}\.txt|robots\.txt)$/;

export async function onRequest(context) {
  const url = new URL(context.request.url);
  if (!OLD_HOSTS.has(url.hostname) || KEEP.test(url.pathname)) return context.next();
  const pt = url.pathname === "/pt" || url.pathname.startsWith("/pt/");
  const target = (pt ? "https://multialt.com.br" : "https://multialt.net") + url.pathname + url.search;
  return Response.redirect(target, 301);
}
