import { useEffect, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings";
import { TabContent } from "../settings/TabContent";
import { SETTINGS_TABS, type TabId } from "../settings/tabs";
import { useTr } from "../../i18n/text";
import { PageShell } from "./PageShell";

interface SettingsPageProps {
  active: boolean;
  onLeave: () => void;
  onSettingsChanged?: () => void;
  onRequestEncryptionSetup?: () => void;
}

const VISIBLE_TABS = SETTINGS_TABS.filter((tab) => !tab.hidden);

/**
 * Página Settings. As abas do antigo modal (que mal cabiam numa linha de
 * 780 px) viraram uma lista vertical à esquerda do conteúdo; o conteúdo tem
 * teto de largura para as linhas de ajuda não esticarem até a borda da janela.
 *
 * Fica montada o tempo todo, como o modal ficava: `active` só decide se
 * aparece. Ao entrar, recarrega as settings; ao sair, avisa a store
 * (`onSettingsChanged`) — o mesmo que o "Done" do modal fazia.
 */
export function SettingsPage({
  active,
  onLeave,
  onSettingsChanged,
  onRequestEncryptionSetup,
}: SettingsPageProps) {
  const t = useTr();
  const s = useSettings();
  const [activeTab, setActiveTab] = useState<TabId>("general");
  const scrollRef = useRef<HTMLDivElement>(null);
  const wasActiveRef = useRef(false);
  const onSettingsChangedRef = useRef(onSettingsChanged);
  onSettingsChangedRef.current = onSettingsChanged;

  useEffect(() => {
    if (active) {
      wasActiveRef.current = true;
      void s.load();
      return;
    }
    if (wasActiveRef.current) {
      wasActiveRef.current = false;
      onSettingsChangedRef.current?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    scrollRef.current?.scrollTo?.({ top: 0 });
  }, [activeTab]);

  if (!active) return null;

  const current = VISIBLE_TABS.find((tab) => tab.id === activeTab) ?? VISIBLE_TABS[0];

  return (
    <PageShell
      title={t("Settings")}
      description={t("Changes are saved automatically")}
      onLeave={onLeave}
      dataTour="settings-page"
      actions={
        s.saving ? (
          <span className="text-[11.5px] text-[var(--panel-muted)] animate-pulse" role="status">
            {t("saving...")}
          </span>
        ) : null
      }
      bodyClassName="flex"
    >
      <nav
        aria-label={t("Settings sections")}
        className="w-[208px] shrink-0 border-r theme-border overflow-y-auto py-4 px-3"
      >
        <ul className="space-y-0.5">
          {VISIBLE_TABS.map((tab) => {
            const selected = tab.id === current.id;
            return (
              <li key={tab.id}>
                <button
                  type="button"
                  onClick={() => setActiveTab(tab.id)}
                  aria-current={selected ? "true" : undefined}
                  className={`relative w-full flex items-center gap-2.5 h-8 px-2.5 rounded-lg text-left text-[12.5px] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
                    selected
                      ? "bg-[var(--panel-soft)] text-[var(--panel-fg)] font-medium"
                      : "text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)]"
                  }`}
                >
                  <span aria-hidden="true" className={selected ? "text-[var(--accent-color)]" : undefined}>
                    {tab.icon}
                  </span>
                  <span className="truncate">{t(tab.label)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </nav>

      <div ref={scrollRef} className="flex-1 min-w-0 overflow-y-auto">
        <div className="max-w-[760px] px-7 py-5">
          <h2 className="mb-3 text-[14px] font-semibold text-[var(--panel-fg)]">{t(current.label)}</h2>
          <TabContent
            activeTab={current.id}
            s={s}
            loaded={s.loaded}
            onRequestEncryptionSetup={onRequestEncryptionSetup}
          />
        </div>
      </div>
    </PageShell>
  );
}
