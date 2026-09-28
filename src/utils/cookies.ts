/**
 * O `.ROBLOSECURITY` e as linhas de import que o carregam.
 *
 * O padrão vivia copiado em cada tela que aceita cookie (arrastar-e-soltar da
 * lista de contas, campo de adicionar conta, import). Três cópias da mesma
 * expressão é três lugares para esquecer quando o Roblox mudar o texto do
 * aviso.
 */
export const COOKIE_PATTERN =
  /_\|WARNING:-DO-NOT-SHARE-THIS\.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items\.\|\w+/;

/**
 * Como o cookie começa. Serve de rede de segurança quando o aviso vem torto —
 * e é o que as telas de "colar cookie ou usuário" usam para saber qual dos dois
 * foi colado, em vez de cada uma repetir um pedaço do aviso.
 */
export const COOKIE_MARKER = "_|WARNING";

export type ImportLineKind =
  /** Tem cookie (com ou sem `username:password` na frente). */
  | "cookie"
  /** Só `username:password` — credencial incompleta para o import por cookie. */
  | "userpass"
  /** Nem uma coisa nem outra: token solto, linha pela metade. */
  | "unknown"
  /** Linha vazia. */
  | "empty";

export interface ParsedImportLine {
  kind: ImportLineKind;
  username: string;
  password: string;
  cookie: string;
}

/** Onde o cookie começa na linha, ou `-1` se não houver nenhum. */
function cookieStart(line: string): number {
  const match = line.match(COOKIE_PATTERN);
  if (match && match.index !== undefined) return match.index;
  // Cookie com o aviso torto (Roblox mudou o texto, paste truncado) ainda é
  // cookie: melhor mandar para o backend validar que tratar como `user:pass`.
  return line.indexOf(COOKIE_MARKER);
}

/**
 * Lê uma linha da caixa de import: `cookie` ou `username:password:cookie`.
 *
 * **O cookie tem `:` dentro dele** (`_|WARNING:-DO-NOT-SHARE...`), então o
 * corte nunca é por `split(":")`: acha-se primeiro onde o cookie começa, o
 * resto é o prefixo, e dele tira-se **só** o delimitador (`\s*:\s*$`) — um
 * `[\s:;,]+$` comeria a pontuação final de uma senha legítima.
 */
export function parseImportLine(raw: string): ParsedImportLine {
  const line = raw.trim();
  if (!line) return { kind: "empty", username: "", password: "", cookie: "" };

  const start = cookieStart(line);
  if (start >= 0) {
    const cookie = line.slice(start).trim();
    const prefix = line.slice(0, start).replace(/\s*:\s*$/, "");
    const sep = prefix.indexOf(":");
    const username = sep >= 0 ? prefix.slice(0, sep).trim() : "";
    const password = sep >= 0 ? prefix.slice(sep + 1).trim() : "";
    // Sem os dois lados o prefixo não é credencial nenhuma (`user:cookie`):
    // vale o cookie sozinho, e o nome real vem do `validate_cookie`.
    return username && password
      ? { kind: "cookie", username, password, cookie }
      : { kind: "cookie", username: "", password: "", cookie };
  }

  const sep = line.indexOf(":");
  const username = sep >= 0 ? line.slice(0, sep).trim() : "";
  const password = sep >= 0 ? line.slice(sep + 1).trim() : "";
  if (username && password) return { kind: "userpass", username, password, cookie: "" };

  // Nem cookie reconhecível nem `user:pass`. `cookie` continua com a linha
  // inteira: no import por cookie ela vai para o backend validar, como antes
  // de este parser existir.
  return { kind: "unknown", username: "", password: "", cookie: line };
}
