/**
 * Prepara o checkout do job `linux` do release-v4.yml: a versão já foi decidida
 * pelo job `release` (o `prepare-release.mjs` roda lá; rodar de novo aqui
 * calcularia a versão seguinte, porque a tag já existe), então só se aplica.
 * Também põe a chave pública do updater e o endereço do manifesto da edição.
 *
 * Entrada (ambiente): RELEASE_APP_VERSION, UPDATER_PUBKEY, UPDATER_ENDPOINT.
 * Idempotente: roda uma vez por edição (só o endereço muda).
 */
import fs from "node:fs";
import path from "node:path";
import { setCargoPackageVersion } from "./release-version.mjs";

const version = process.env.RELEASE_APP_VERSION;
const pubkey = process.env.UPDATER_PUBKEY;
const endpoint = process.env.UPDATER_ENDPOINT;
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`RELEASE_APP_VERSION invalid: ${version}`);
if (!pubkey) throw new Error("UPDATER_PUBKEY is required");
if (!endpoint) throw new Error("UPDATER_ENDPOINT is required");

const root = process.cwd();
const read = (rel) => fs.readFileSync(path.join(root, rel), "utf8");
const write = (rel, text) => fs.writeFileSync(path.join(root, rel), text, "utf8");

const pkg = JSON.parse(read("package.json"));
pkg.version = version;
write("package.json", `${JSON.stringify(pkg, null, 2)}\n`);

const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
conf.version = version;
conf.plugins ??= {};
conf.plugins.updater ??= {};
conf.plugins.updater.pubkey = pubkey;
conf.plugins.updater.endpoints = [endpoint];
write("src-tauri/tauri.conf.json", `${JSON.stringify(conf, null, 2)}\n`);

write("src-tauri/Cargo.toml", setCargoPackageVersion(read("src-tauri/Cargo.toml"), version));
console.log(`Linux build prepared: ${version}, ${endpoint}`);
