/**
 * Bipe curto do AFK mode: avisa que um ciclo de envio acabou de acontecer.
 *
 * O usuário está usando o PC normalmente, e o piscar de foco do envio fica sem
 * explicação se nada avisa que foi o app. O som é **opcional** (`Afk.BeepOnCycle`,
 * default desligado).
 *
 * Por que sintetizado e não um arquivo de áudio: um `.wav` no repositório é
 * asset novo com licença para rastrear, e um bipe de 120 ms não justifica isso.
 * Web Audio API dá o mesmo resultado sem arquivo nenhum.
 */

/** O construtor de `AudioContext` que existir neste navegador. */
type AudioContextCtor = new () => AudioContext;

function audioContextCtor(): AudioContextCtor | null {
  const w = globalThis as unknown as {
    AudioContext?: AudioContextCtor;
    webkitAudioContext?: AudioContextCtor;
  };
  return w.AudioContext ?? w.webkitAudioContext ?? null;
}

/**
 * Toca o bipe. Nunca lança: som é conforto, não função — janela sem Web Audio
 * (ou com áudio bloqueado) segue sem som e sem erro na tela.
 *
 * Devolve `true` quando o bipe foi disparado, para o teste conseguir afirmar.
 */
export function playAfkBeep(volume = 0.08, durationMs = 120, frequency = 880): boolean {
  const Ctor = audioContextCtor();
  if (!Ctor) return false;
  try {
    const ctx = new Ctor();
    const oscillator = ctx.createOscillator();
    const gain = ctx.createGain();
    oscillator.type = "sine";
    oscillator.frequency.value = frequency;
    // Volume baixo e rampa até zero: bipe seco estala no fim.
    gain.gain.value = volume;
    const endsAt = ctx.currentTime + durationMs / 1000;
    gain.gain.exponentialRampToValueAtTime?.(0.0001, endsAt);
    oscillator.connect(gain);
    gain.connect(ctx.destination);
    oscillator.start();
    oscillator.stop(endsAt);
    oscillator.onended = () => {
      // Cada bipe abre o seu contexto; sem fechar, dez ciclos deixam dez
      // contextos de áudio abertos no processo.
      void ctx.close?.();
    };
    return true;
  } catch {
    return false;
  }
}
