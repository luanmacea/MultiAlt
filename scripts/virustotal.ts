/**
 * Manda um arquivo para o VirusTotal e mostra o veredito.
 *
 *   bun run vt <arquivo>              # usa o relatório existente, se houver
 *   bun run vt <arquivo> --rescan     # força uma análise nova
 *
 * Precisa de uma chave da API (conta gratuita em virustotal.com → seu perfil →
 * API key). **Nunca** ponha a chave no repositório: exporte no ambiente.
 *
 *   PowerShell:  $env:VT_API_KEY = "..."
 *   bash:        export VT_API_KEY="..."
 *
 * Limites da conta gratuita: 4 pedidos por minuto, 500 por dia.
 *
 * ⚠️ Subir um arquivo ao VirusTotal o compartilha com os fabricantes de
 * antivírus parceiros. Para os instaladores deste projeto isso não é problema
 * (eles já são públicos nas releases), mas não mande arquivo com dado seu.
 */
const API = "https://www.virustotal.com/api/v3";

const args = process.argv.slice(2);
const rescan = args.includes("--rescan");
const caminho = args.find((a) => !a.startsWith("--"));

if (!caminho) {
  console.error("uso: bun run vt <arquivo> [--rescan]");
  process.exit(1);
}

/**
 * A chave vem do ambiente ou de `~/.tauri/virustotal.key` — a mesma pasta onde
 * moram as chaves de assinatura do projeto, **fora** do repositório. O arquivo
 * vence o ambiente só se o ambiente não trouxer nada.
 */
async function lerChave(): Promise<string | null> {
  const doAmbiente = process.env.VT_API_KEY?.trim();
  if (doAmbiente) return doAmbiente;

  const lar = process.env.USERPROFILE || process.env.HOME;
  if (!lar) return null;
  const arquivoChave = Bun.file(`${lar}/.tauri/virustotal.key`);
  if (!(await arquivoChave.exists())) return null;
  return (await arquivoChave.text()).trim() || null;
}

const chave = await lerChave();
if (!chave) {
  console.error(
    "Falta a chave da API do VirusTotal (conta gratuita → seu perfil → API key).\n" +
      "Escolha um dos dois:\n" +
      "  1) guarde num arquivo (vale para as próximas vezes):\n" +
      "     %USERPROFILE%/.tauri/virustotal.key\n" +
      "  2) exporte no ambiente desta sessão:\n" +
      '     PowerShell:  $env:VT_API_KEY = "sua-chave"\n' +
      '     bash:        export VT_API_KEY="sua-chave"'
  );
  process.exit(1);
}

const cabecalho = { "x-apikey": chave };

const arquivo = Bun.file(caminho);
if (!(await arquivo.exists())) {
  console.error(`arquivo não encontrado: ${caminho}`);
  process.exit(1);
}

const bytes = new Uint8Array(await arquivo.arrayBuffer());
const digest = new Bun.CryptoHasher("sha256").update(bytes).digest("hex");

console.log(`arquivo : ${caminho}`);
console.log(`tamanho : ${(bytes.length / 1048576).toFixed(1)} MB`);
console.log(`sha256  : ${digest}\n`);

/** Mostra o placar e quem marcou. Devolve quantos marcaram como malicioso. */
function mostrar(stats: Record<string, number>, resultados: Record<string, any>, quando?: string) {
  const mal = stats.malicious ?? 0;
  const susp = stats.suspicious ?? 0;
  const total = Object.values(stats).reduce((a, b) => a + b, 0);
  console.log(`veredito: ${mal + susp}/${total} marcaram${quando ? `  (análise de ${quando})` : ""}`);
  const marcaram = Object.entries(resultados)
    .filter(([, r]) => r?.category === "malicious" || r?.category === "suspicious")
    .map(([motor, r]) => `  ${motor}: ${r.result} [${r.category}]`);
  if (marcaram.length > 0) console.log(marcaram.join("\n"));
  else console.log("  nenhum motor marcou o arquivo");
  return mal + susp;
}

async function esperarAnalise(id: string) {
  process.stdout.write("analisando");
  for (let i = 0; i < 40; i++) {
    await Bun.sleep(15000);
    process.stdout.write(".");
    const r = await fetch(`${API}/analyses/${id}`, { headers: cabecalho });
    if (!r.ok) continue;
    const dados = (await r.json()) as any;
    if (dados.data?.attributes?.status === "completed") {
      console.log("\n");
      return dados.data.attributes;
    }
  }
  console.log("\na análise não terminou a tempo; rode de novo para ler o resultado");
  return null;
}

// 1) Já existe relatório para este hash?
const existente = await fetch(`${API}/files/${digest}`, { headers: cabecalho });

if (existente.ok && !rescan) {
  const dados = (await existente.json()) as any;
  const attr = dados.data.attributes;
  const quando = new Date((attr.last_analysis_date ?? 0) * 1000).toLocaleString("pt-BR");
  const n = mostrar(attr.last_analysis_stats, attr.last_analysis_results, quando);
  console.log(`\nhttps://www.virustotal.com/gui/file/${digest}`);
  console.log(n > 0 ? "\n(--rescan força uma análise nova com os motores de hoje)" : "");
  process.exit(0);
}

// 2) Reanálise de um arquivo que o VirusTotal já conhece.
if (existente.ok && rescan) {
  console.log("pedindo análise nova...");
  const r = await fetch(`${API}/files/${digest}/analyse`, { method: "POST", headers: cabecalho });
  if (!r.ok) {
    console.error(`falha ao pedir reanálise: ${r.status} ${await r.text()}`);
    process.exit(1);
  }
  const attr = await esperarAnalise(((await r.json()) as any).data.id);
  if (attr) mostrar(attr.stats, attr.results);
  console.log(`\nhttps://www.virustotal.com/gui/file/${digest}`);
  process.exit(0);
}

// 3) Arquivo novo: envia. Acima de 32 MB o VirusTotal exige uma URL própria.
console.log("arquivo desconhecido pelo VirusTotal; enviando...");
let destino = `${API}/files`;
if (bytes.length > 32 * 1024 * 1024) {
  const r = await fetch(`${API}/files/upload_url`, { headers: cabecalho });
  if (!r.ok) {
    console.error(`falha ao pegar a URL de envio: ${r.status}`);
    process.exit(1);
  }
  destino = ((await r.json()) as any).data;
}

const form = new FormData();
form.append("file", new Blob([bytes]), caminho.split(/[\\/]/).pop());
const envio = await fetch(destino, { method: "POST", headers: cabecalho, body: form });
if (!envio.ok) {
  console.error(`falha no envio: ${envio.status} ${await envio.text()}`);
  process.exit(1);
}

const attr = await esperarAnalise(((await envio.json()) as any).data.id);
if (attr) mostrar(attr.stats, attr.results);
console.log(`\nhttps://www.virustotal.com/gui/file/${digest}`);
