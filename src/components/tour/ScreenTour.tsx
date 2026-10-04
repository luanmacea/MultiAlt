import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, GraduationCap, X } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useEscapeStack } from "../../hooks/useEscapeStack";
import { placePanel } from "./placement";
import { Spotlight, useSpotlight } from "./useSpotlight";
import { TOURS, type TourDefinition, type TourId } from "./tours";
import { closeTour, useActiveTour } from "./tourState";

/** Tamanho de reserva do painel antes de ele ser medido (e no jsdom, que não mede). */
const FALLBACK_PANEL = { width: 340, height: 230 };
/** Quantas checagens seguidas sem a tela antes de fechar (cada uma a 250 ms). */
const ROOT_MISSES_TO_CLOSE = 2;

function isField(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/**
 * Tutorial curto de uma tela: aponta uma parte da tela por vez, com Voltar,
 * Próximo, contador de passos e fechar (X ou Escape).
 *
 * Só olha e aponta. O único clique que ele dá sozinho é o `reveal` de um passo
 * — abrir a aba ou seção que o passo explica (ver `tours.ts`).
 */
export function ScreenTour({ tour, onClose }: { tour: TourDefinition; onClose: () => void }) {
  const t = useTr();
  const titleId = useId();
  const descriptionId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);
  const [index, setIndex] = useState(0);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);

  const steps = tour.steps;
  const step = steps[Math.min(index, steps.length - 1)];
  const isFirst = index === 0;
  const isLast = index >= steps.length - 1;

  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const close = useCallback(() => onCloseRef.current(), []);

  const goNext = useCallback(() => setIndex((i) => Math.min(steps.length - 1, i + 1)), [steps.length]);
  const goBack = useCallback(() => setIndex((i) => Math.max(0, i - 1)), []);

  useEscapeStack(true, close, { ignoreFromFields: true });

  // Abre a aba/seção do passo. Uma vez por entrada no passo.
  useEffect(() => {
    if (!step.reveal) return;
    const control = document.querySelector<HTMLElement>(step.reveal);
    control?.click();
  }, [step]);

  const resolve = useCallback(() => {
    for (const selector of step.targets ?? []) {
      const element = document.querySelector<HTMLElement>(selector);
      if (element) return element;
    }
    return null;
  }, [step]);

  const { target, rect } = useSpotlight(resolve, step.id);

  // Alvo dentro de uma lista rolada para fora da vista: traz para a tela.
  const scrolledStepRef = useRef<string | null>(null);
  useEffect(() => {
    if (!target || scrolledStepRef.current === step.id) return;
    // Logo depois da troca de passo o `target` ainda é o do passo anterior:
    // rolar até ele (e marcar o passo como rolado) deixava o alvo novo fora da tela.
    if (resolve() !== target) return;
    scrolledStepRef.current = step.id;
    target.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [target, step.id, resolve]);

  // A pessoa saiu da tela (barra lateral, Voltar): o tutorial não tem mais o que mostrar.
  useEffect(() => {
    let misses = 0;
    const timer = window.setInterval(() => {
      if (document.querySelector(tour.root)) {
        misses = 0;
        return;
      }
      misses += 1;
      if (misses >= ROOT_MISSES_TO_CLOSE) close();
    }, 250);
    return () => window.clearInterval(timer);
  }, [tour.root, close]);

  useLayoutEffect(() => {
    const update = () => {
      const panel = panelRef.current;
      const size = {
        width: panel?.offsetWidth || FALLBACK_PANEL.width,
        height: panel?.offsetHeight || FALLBACK_PANEL.height,
      };
      setPosition(placePanel(rect, size, { width: window.innerWidth, height: window.innerHeight }));
    };
    update();
    window.addEventListener("resize", update);
    const observer =
      panelRef.current && typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
    if (observer && panelRef.current) observer.observe(panelRef.current);
    return () => {
      window.removeEventListener("resize", update);
      observer?.disconnect();
    };
  }, [rect, index]);

  // Setas andam pelos passos (fora de campo de texto).
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isField(event.target)) return;
      if (event.key === "ArrowRight") {
        event.preventDefault();
        goNext();
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        goBack();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [goNext, goBack]);

  // Foco no painel enquanto ele está aberto; ao fechar, volta para quem abriu.
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    nextRef.current?.focus();
    return () => {
      if (opener && opener.isConnected) opener.focus();
    };
  }, []);

  const found = !!target;
  const progress = ((index + 1) / steps.length) * 100;

  return (
    <div className="fixed inset-0 z-[95] pointer-events-none" data-testid="screen-tour">
      <Spotlight rect={rect} />

      <div
        ref={panelRef}
        role="dialog"
        aria-modal="false"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        data-tour-id={tour.id}
        data-step-id={step.id}
        data-target-found={found ? "true" : "false"}
        className="walkthrough-panel screen-tour-panel pointer-events-auto animate-scale-in"
        style={
          position
            ? { left: position.left, top: position.top }
            : { visibility: "hidden" }
        }
      >
        <div className="flex items-center gap-2">
          <span className="walkthrough-chip">
            <GraduationCap size={12} strokeWidth={2} aria-hidden="true" />
            {t(tour.label)}
          </span>
          <span className="ml-auto text-[11.5px] tabular-nums theme-muted">
            {t("Step {{current}} of {{total}}", { current: index + 1, total: steps.length })}
          </span>
          <button
            type="button"
            onClick={close}
            aria-label={t("Close tutorial")}
            title={t("Close tutorial")}
            className="walkthrough-close-btn"
          >
            <X size={14} strokeWidth={2} />
          </button>
        </div>

        <div className="mt-3 walkthrough-progress" aria-hidden="true">
          <div className="walkthrough-progress-bar" style={{ width: `${progress}%` }} />
        </div>

        <div key={step.id} className="mt-3 animate-fade-in-up">
          <h2 id={titleId} className="text-[14.5px] font-semibold tracking-tight text-[var(--panel-fg)]">
            {t(step.title)}
          </h2>
          <p id={descriptionId} className="mt-1.5 text-[12.5px] leading-relaxed text-[var(--panel-fg)] opacity-85">
            {t(step.description)}
          </p>
          {!found && step.targets?.length ? (
            <p className="walkthrough-inline-tip mt-2.5">{t("This part is not on screen right now.")}</p>
          ) : null}
        </div>

        <div className="mt-4 flex items-center justify-between gap-2">
          <button type="button" onClick={goBack} disabled={isFirst} className="walkthrough-secondary-btn">
            <ArrowLeft size={13} strokeWidth={2} aria-hidden="true" />
            <span>{t("Back")}</span>
          </button>
          <button
            ref={nextRef}
            type="button"
            onClick={isLast ? close : goNext}
            className="walkthrough-primary-btn"
          >
            <span>{isLast ? t("Done") : t("Next")}</span>
            {!isLast ? <ArrowRight size={13} strokeWidth={2} aria-hidden="true" /> : null}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Mostra o tutorial aberto pelo botão Tutorial de alguma tela. Fica no App,
 * fora das páginas, para o painel passar por cima de tudo.
 *
 * O tour de boas-vindas manda: se ele abrir (Ajuda), o tutorial de tela fecha.
 */
export function ScreenTourHost({ tours = TOURS }: { tours?: Partial<Record<TourId, TourDefinition>> }) {
  const active = useActiveTour();
  const store = useStore();
  const introOpen = !!store.firstRunWalkthroughOpen;

  useEffect(() => {
    if (introOpen && active) closeTour();
  }, [introOpen, active]);

  if (!active || introOpen) return null;
  const tour = tours[active];
  if (!tour) return null;
  return <ScreenTour key={active} tour={tour} onClose={closeTour} />;
}
