import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Radio, X } from "lucide-react";
import { useSettings } from "../../hooks/useSettings";
import { useListSelection } from "../../hooks/useListSelection";
import { applyToList } from "../../utils/listSelection";
import { useTr } from "../../i18n/text";
import { Toggle } from "../ui/Toggle";
import { Select } from "../ui/Select";
import { SectionLabel } from "../ui/SectionLabel";
import { Divider } from "../ui/Divider";
import type { LiveAccount } from "../../live/LiveWindow";
import { PageShell } from "./PageShell";

const POLL_MS = 3000;

/**
 * Página Live (pedido do dono, 28/10/2026): escolhe quais contas abertas vão
 * para a janela "MultiAlt Live" e abre a janela — quem transmite captura só ela
 * no OBS, sem mostrar o resto da tela. Ver docs/features/live.md.
 */
export function LivePage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const s = useSettings();
  const [accounts, setAccounts] = useState<LiveAccount[]>([]);
  const [open, setOpen] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [list, isOpen] = await Promise.all([
        invoke<LiveAccount[]>("live_accounts"),
        invoke<boolean>("live_window_open"),
      ]);
      setAccounts(list);
      setOpen(isOpen);
    } catch {
      // Sem backend (teste) ou app fechando: a lista fica como estava.
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void s.load();
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    const unlisten = listen("live-changed", () => void refresh());
    return () => {
      window.clearInterval(timer);
      void unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, refresh]);

  // O que foi pedido e o backend ainda não devolveu: o arrasto e o Shift mandam
  // várias trocas seguidas, cada uma parte da anterior e vai em ordem.
  const [pending, setPending] = useState<number[] | null>(null);
  const pendingRef = useRef<number[] | null>(null);
  const writes = useRef(0);
  const chain = useRef<Promise<void>>(Promise.resolve());
  const shown = pending ?? accounts.filter((a) => a.selected).map((a) => a.userId);

  /** Passa `userIds` para dentro (`on`) ou para fora da janela Live. */
  function setMany(userIds: number[], on: boolean) {
    const next = applyToList(pendingRef.current ?? accounts.filter((a) => a.selected).map((a) => a.userId), userIds, on);
    pendingRef.current = next;
    setPending(next);
    writes.current += 1;
    chain.current = chain.current
      .then(async () => {
        // Lista vazia aqui é "nenhuma" (desmarcou a última), não "todas".
        await invoke("set_live_accounts", { userIds: next, all: false });
      })
      .catch(() => {})
      .then(async () => {
        writes.current -= 1;
        if (writes.current > 0) return;
        await s.load();
        await refresh();
        if (writes.current === 0) {
          pendingRef.current = null;
          setPending(null);
        }
      });
  }

  const selection = useListSelection<number>({
    order: accounts.map((a) => a.userId),
    isSelected: (id) => shown.includes(id),
    apply: setMany,
  });

  if (!active) return null;

  const everyone = !s.get("Live", "Accounts", "").trim();

  async function selectAll() {
    await invoke("set_live_accounts", { userIds: [], all: true });
    await s.load();
    await refresh();
  }

  return (
    <PageShell
      title={t("Live")}
      description={t(
        "Stream your accounts without showing the rest of the screen: the Live window shows the accounts you pick, side by side, always using the whole window. In OBS, add a Window Capture of \"MultiAlt Live\"."
      )}
      onLeave={onLeave}
      actions={
        open ? (
          <button
            type="button"
            onClick={() => void invoke("close_live_window").then(refresh)}
            className="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/70 bg-zinc-800 px-3 py-1.5 text-[12px] font-medium text-zinc-200 hover:bg-zinc-700"
          >
            <X size={14} aria-hidden="true" />
            {t("Close the Live window")}
          </button>
        ) : (
          <button
            type="button"
            onClick={() => void invoke("open_live_window").then(refresh)}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--accent-color)] px-3 py-1.5 text-[12px] font-medium text-white"
          >
            <Radio size={14} aria-hidden="true" />
            {t("Open the Live window")}
          </button>
        )
      }
    >
      <div className="max-w-[760px] 2xl:max-w-[920px]">
        <SectionLabel>Accounts to show</SectionLabel>
        <div className="flex items-center justify-between px-1 pb-2 text-[12px] text-zinc-500">
          <span>
            {everyone
              ? t("Every open account is shown, including the ones you open later.")
              : t("Only the accounts ticked below are shown.")}
          </span>
          <span className="flex items-center gap-3">
            {!everyone && (
              <button type="button" onClick={() => void selectAll()} className="text-sky-400 hover:text-sky-300">
                {t("Show every open account")}
              </button>
            )}
            {accounts.length > 0 && shown.length > 0 && (
              <button type="button" onClick={() => setMany(shown, false)} className="text-sky-400 hover:text-sky-300">
                {t("Clear")}
              </button>
            )}
          </span>
        </div>
        {accounts.length === 0 ? (
          <p className="px-1 py-3 text-[13px] text-zinc-500">
            {t("No Roblox client open. Launch accounts from MultiAlt (or open them from the website) and they show up here.")}
          </p>
        ) : (
          <ul className="divide-y divide-zinc-800/70 rounded-lg border border-zinc-800/70">
            {accounts.map((account) => (
              <li
                key={account.userId}
                {...selection.rowProps(account.userId)}
                className="flex items-center gap-3 px-3 py-2 text-[13px] text-zinc-200"
              >
                <input
                  type="checkbox"
                  aria-label={account.label || String(account.userId)}
                  checked={shown.includes(account.userId)}
                  onChange={(e) => selection.toggle(account.userId, e.nativeEvent)}
                />
                <span
                  className={`inline-block h-2 w-2 rounded-full ${account.inGame ? "bg-emerald-400" : "bg-amber-400"}`}
                  aria-hidden="true"
                />
                <span className="truncate">{account.label || t("Account")}</span>
                <span className="ml-auto text-[11px] text-zinc-500">{account.inGame ? t("In game") : t("Not in a game")}</span>
              </li>
            ))}
          </ul>
        )}

        <Divider />
        <SectionLabel>Live window</SectionLabel>
        <Toggle
          checked={s.get("Live", "ShowNames", "true") !== "false"}
          onChange={(v) => s.setBool("Live", "ShowNames", v)}
          label="Show account names"
          description="Above each account. With Names hidden on, they are masked the same way."
        />
        <Toggle
          checked={s.get("Live", "KeepMinimized", "true") !== "false"}
          onChange={(v) => s.setBool("Live", "KeepMinimized", v)}
          label="Keep minimized accounts live"
          description="Windows stops drawing a minimized window. While the Live window is open, MultiAlt restores it invisibly (it does not show up or take clicks) and minimizes it again when the Live window closes."
        />
        <div className="flex items-center gap-3 px-1 py-2">
          <div className="min-w-0">
            <div className="text-[13px] text-zinc-300">{t("Background")}</div>
            <div className="mt-0.5 text-[12px] text-zinc-500 leading-snug">
              {t("Green works as a chroma key in OBS.")}
            </div>
          </div>
          <div className="ml-auto w-[170px] shrink-0">
            <Select
              ariaLabel="Background"
              value={s.get("Live", "Background", "dark")}
              onChange={(value) => s.set("Live", "Background", value)}
              options={[
                { value: "dark", label: "Dark" },
                { value: "black", label: "Black" },
                { value: "green", label: "Green" },
              ]}
            />
          </div>
        </div>
      </div>
    </PageShell>
  );
}
