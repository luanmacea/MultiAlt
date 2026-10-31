import { useEffect, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings";
import { TabContent } from "../settings/TabContent";
import { tabsFor, type TabId } from "../settings/tabs";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { PageShell } from "./PageShell";

interface SettingsPageProps {
  active: boolean;
  onLeave: () => void;
  onSettingsChanged?: () => void;
  onRequestEncryptionSetup?: () => void;
}

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
  const store = useStore();
  const [activeTab, setActiveTab] = useState<TabId>("launch");
  const sectionRequest = store.settingsSectionRequest;
  // Seções do build e do SO: no Linux somem as do Windows (e aparece "Linux").
  const VISIBLE_TABS = tabsFor(store.platformCapabilities);
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

  // Outra tela pediu uma seção (`store.openSettingsSection`): o Auto Rejoin
  // desligado leva a Keep in game, o CAPTCHA do login a Accounts.
  useEffect(() => {
    if (sectionRequest && VISIBLE_TABS.some((tab) => tab.id === sectionRequest.section)) {
      setActiveTab(sectionRequest.section);
    }
  }, [sectionRequest]);

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
      tour="settings"
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
        data-tour="settings-sections"
        className="w-[208px] shrink-0 border-r theme-border overflow-y-auto py-4 px-3"
      >
        <ul className="space-y-0.5">
          {VISIBLE_TABS.map((tab, index) => {
            const selected = tab.id === current.id;
            // Primeira seção técnica: abre o grupo "Advanced", separado do dia a dia.
            const opensAdvanced = tab.advanced === true && VISIBLE_TABS[index - 1]?.advanced !== true;
            return (
              <li key={tab.id}>
                {opensAdvanced && (
                  <div
                    className="mt-3 mb-1 border-t theme-border px-2.5 pt-3 text-[11px] font-medium text-[var(--panel-muted)]"
                  >
                    {t("Advanced")}
                  </div>
                )}
                <button
                  type="button"
                  onClick={() => setActiveTab(tab.id)}
                  aria-current={selected ? "true" : undefined}
                  data-tour={`settings-section-${tab.id}`}
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

      <div ref={scrollRef} data-tour="settings-content" className="flex-1 min-w-0 overflow-y-auto">
        {/* Formulário: rótulo à esquerda, controle à direita. Esticado na
            janela de um monitor 4K, o controle ia parar longe do rótulo —
            fica numa medida confortável, um pouco mais larga em tela grande. */}
        <div className="max-w-[760px] 2xl:max-w-[920px] px-7 py-5">
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
