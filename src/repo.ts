/**
 * Onde este projeto mora.
 *
 * Um lugar só, porque já esteve em seis: o app foi bifurcado de
 * `niccsprojects/Roblox-Account-Manager` e continuava apontando para lá no
 * botão do repositório, nos links do diálogo de atualização, na documentação do
 * Nexus e — o pior — no updater, que anunciava a versão **do outro projeto** e
 * a instalaria por cima desta. Com o endereço espalhado em literais, cada um
 * era corrigido num dia diferente.
 *
 * O lado Rust tem o seu próprio (`commands/services.rs`, `commands/updater.rs`);
 * [repoOwnership.test.ts](./repoOwnership.test.ts) varre os dois lados.
 */
export const REPO_SLUG = "luanmacea/roblox-account-manager";

export const REPO_URL = `https://github.com/${REPO_SLUG}`;

/** Base da API usada para ler as notas de uma release. */
export const REPO_API_URL = `https://api.github.com/repos/${REPO_SLUG}`;
