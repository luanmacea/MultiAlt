import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

/**
 * Arrastar dentro da janela do app.
 *
 * O Tauri intercepta o drag-and-drop do sistema operacional na janela do
 * WebView quando `dragDropEnabled` é `true` (o **default**). Com isso o
 * WebView2 engole o gesto e os eventos HTML5 (`dragstart`/`dragover`/`drop`)
 * nunca chegam à página: o cursor vira o de "bloqueado" e nada se move. Foi o
 * que o dono viu ao tentar arrastar um grupo — no navegador o mesmo código
 * funciona, porque lá não existe essa interceptação.
 *
 * Desligar não custa nada aqui: o app **não** usa a API de drag-drop do Tauri
 * (`onDragDropEvent`) em lugar nenhum; ele depende dos eventos HTML5 para
 * reordenar contas e grupos, para mover conta de grupo, para soltar cookie em
 * texto e para soltar arquivo no diálogo de importação.
 */
describe("configuração da janela do Tauri", () => {
  it("não deixa o Tauri interceptar o drag-and-drop da janela", () => {
    const conf = JSON.parse(
      readFileSync(path.resolve(__dirname, "../src-tauri/tauri.conf.json"), "utf8")
    );
    const janela = conf.app.windows[0];

    expect(janela.dragDropEnabled).toBe(false);
  });

  /**
   * Tamanho da interface (`hooks/useUiScale.ts`) usa `getCurrentWebview().setZoom`.
   * Sem esta permissão o Tauri recusa o comando, o hook engole o erro e a
   * interface fica em 100% sem ninguém perceber — o `core:default` não a inclui.
   */
  it("deixa a janela principal mudar o zoom do WebView", () => {
    const caps = JSON.parse(
      readFileSync(path.resolve(__dirname, "../src-tauri/capabilities/default.json"), "utf8")
    );

    expect(caps.windows).toContain("main");
    expect(caps.permissions).toContain("core:webview:allow-set-webview-zoom");
  });
});
