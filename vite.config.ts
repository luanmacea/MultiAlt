/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

const host = process.env.TAURI_DEV_HOST;

/**
 * Modo harness (`bun run dev:ui`): roda o frontend de verdade no navegador com
 * o lado Tauri trocado por dublês, para validar a interface sem compilar o app.
 * Ver `docs/development.md#validando-a-ui-no-navegador`.
 */
const uiHarness = process.env.UI_HARNESS === "1";

/** Caminho absoluto de um dublê do harness (alias do Vite não aceita relativo). */
function harnessModule(name: string): string {
  return fileURLToPath(new URL(`./src/dev/harness/${name}.ts`, import.meta.url));
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: uiHarness
    ? {
        alias: {
          "@tauri-apps/api/core": harnessModule("core"),
          "@tauri-apps/api/event": harnessModule("event"),
          "@tauri-apps/api/window": harnessModule("window"),
          "@tauri-apps/api/webview": harnessModule("webview"),
          "@tauri-apps/api/app": harnessModule("app"),
        },
      }
    : undefined,
  // O alias troca um pacote por arquivo nosso, e o Vite pre-empacota pacote em
  // `node_modules/.vite/deps` — cache que NAO invalida quando o dublê muda. Sem
  // este `exclude` o navegador recebe um harness velho sem avisar ninguém.
  optimizeDeps: uiHarness
    ? { exclude: ["@tauri-apps/api/core", "@tauri-apps/api/event", "@tauri-apps/api/window", "@tauri-apps/api/webview", "@tauri-apps/api/app"] }
    : undefined,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    environment: "happy-dom",
    // Os scripts de release (.github/scripts) também têm testes: a regra do
    // número da versão decide o que sai publicado.
    include: ["src/**/*.test.{ts,tsx}", ".github/scripts/**/*.test.mjs", "scripts/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: [
        "src/scripting/**",
        "src/components/server-list/types.ts",
        "src/components/dialogs/ScriptsDialog.tsx",
      ],
    },
  },
});
