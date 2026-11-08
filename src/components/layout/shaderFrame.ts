// Ritmo e tamanho do fundo em shader da tela de senha (PasswordScreen).
//
// O fundo é um campo de ruído suave: desenhá-lo a 30 quadros por segundo e na
// metade da resolução da janela não muda o que se vê, e custa uma fração do que
// custava a cada quadro do monitor a 1,5x a resolução (~20% da GPU num 2560x1440
// a 144 Hz, 08/11/2026). O canvas é esticado pelo CSS até o tamanho da janela.

export const SHADER_FPS = 30;
export const SHADER_RENDER_SCALE = 0.5;

// Folga para o quadro que chega um pouco antes da hora: sem ela, um monitor de
// 60 Hz com variação de 0,2 ms pularia de 30 para 20 quadros por segundo.
const FRAME_SLACK = 0.75;

export function shaderFrameDue(now: number, lastDrawn: number, fps: number = SHADER_FPS): boolean {
  return now - lastDrawn >= (1000 / fps) * FRAME_SLACK;
}

export function shaderCanvasSize(cssWidth: number, cssHeight: number): { width: number; height: number } {
  return {
    width: Math.max(1, Math.round(cssWidth * SHADER_RENDER_SCALE)),
    height: Math.max(1, Math.round(cssHeight * SHADER_RENDER_SCALE)),
  };
}
