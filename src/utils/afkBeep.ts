/**
 * Bipe curto do AFK mode: avisa que um ciclo de envio acabou de acontecer.
 *
 * O usuário está usando o PC, e o piscar de foco do envio fica sem explicação se
 * nada avisa que foi o app. O som é **opcional** (`Afk.BeepOnCycle`, default
 * desligado).
 *
 * Por que sintetizado e não um arquivo de áudio: um `.wav` no repositório é asset
 * novo com licença para rastrear, e um bipe de 120 ms não justifica isso.
 *
 * Por que **um** contexto para todo o módulo: um `AudioContext` por ciclo vaza
 * quando o `onended` não dispara (contexto que nasce `suspended` porque a janela
 * ainda não recebeu interação), e o navegador limita quantos contextos um
 * documento pode abrir — passado o limite, `new AudioContext()` lança e o bipe
 * morre em silêncio.
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

/** O contexto compartilhado, criado na primeira vez que alguém bipa. */
let shared: AudioContext | null = null;

function sharedContext(): AudioContext | null {
  if (shared && shared.state !== "closed") return shared;
  const Ctor = audioContextCtor();
  if (!Ctor) return null;
  try {
    shared = new Ctor();
    return shared;
  } catch {
    shared = null;
    return null;
  }
}

/** Só para teste: esquece o contexto compartilhado. */
export function resetAfkBeepContextForTests(): void {
  shared = null;
}

/**
 * Toca o bipe. Nunca lança: som é conforto, não função — janela sem Web Audio (ou
 * com áudio bloqueado) segue sem som e sem erro na tela.
 *
 * Devolve `true` quando o bipe foi disparado, para o teste conseguir afirmar.
 */
export function playAfkBeep(volume = 0.08, durationMs = 120, frequency = 880): boolean {
  const ctx = sharedContext();
  if (!ctx) return false;
  try {
    // Contexto criado antes de qualquer interação nasce suspenso; sem isto o
    // oscilador toca no vazio e o `onended` nunca chega.
    void ctx.resume?.();
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
    // O contexto fica de pé para o próximo ciclo; só os nós desta vez saem.
    oscillator.onended = () => {
      try {
        oscillator.disconnect();
        gain.disconnect();
      } catch {
        // Nó já desconectado pelo navegador: nada a fazer.
      }
    };
    return true;
  } catch {
    return false;
  }
}
