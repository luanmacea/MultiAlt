/**
 * Diagnóstico "o launch não faz nada" (ideia 16). O backend
 * (`run_launch_diagnostics`, commands/diagnostics.rs) devolve só `id` +
 * `reason` (+ `count`); a frase que a pessoa lê sai daqui, traduzida. Assim
 * nenhum caminho, nome de conta ou PID chega à tela — e o resumo do "Reportar
 * problema" leva o resultado sem precisar anonimizar.
 */

export type DiagnosticStatus = "ok" | "warn" | "problem";

export interface DiagnosticCheck {
  id: string;
  status: DiagnosticStatus;
  reason: string;
  count?: number;
}

export interface DiagnosticText {
  title: string;
  /** O que fazer, numa frase. Vazio quando está tudo bem. */
  detail: string;
}

type T = (text: string, options?: Record<string, unknown>) => string;

/** A frase de cada resultado. Combinação desconhecida cai num texto genérico. */
export function diagnosticText(check: DiagnosticCheck, t: T): DiagnosticText {
  const count = check.count ?? 0;
  switch (`${check.id}.${check.reason}`) {
    case "robloxInstall.found":
      return { title: t("Roblox is installed"), detail: "" };
    case "robloxInstall.missing":
      return {
        title: t("No Roblox build found on this PC"),
        detail: t(
          "MultiAlt downloads it on the first launch. If launching still does nothing, check the internet line below or install Roblox from roblox.com."
        ),
      };
    case "dataFolder.writable":
      return { title: t("MultiAlt can save its data"), detail: "" };
    case "dataFolder.notWritable":
      return {
        title: t("MultiAlt can't write to its data folder"),
        detail: t(
          "Accounts and settings can't be saved. Check that the folder isn't read-only or blocked by another program, then restart MultiAlt."
        ),
      };
    case "dataFolder.unknown":
      return {
        title: t("Couldn't find MultiAlt's data folder"),
        detail: t("Restart the PC and open MultiAlt again."),
      };
    case "versionsFolder.writable":
      return { title: t("Roblox builds can be downloaded to disk"), detail: "" };
    case "versionsFolder.notWritable":
      return {
        title: t("MultiAlt can't write to the Roblox builds folder"),
        detail: t(
          "It can't download the Roblox build it needs. Free some disk space and check that the folder isn't read-only or blocked by another program."
        ),
      };
    case "versionsFolder.unknown":
      return {
        title: t("Couldn't find the Roblox builds folder"),
        detail: t("Windows didn't say where local app data lives. Restart the PC and try again."),
      };
    case "internet.reachable":
      return { title: t("Roblox can be reached"), detail: "" };
    case "internet.partial":
      return {
        title: t("Only part of Roblox answered"),
        detail: t(
          "Roblox may be having trouble, or something on your network blocks part of it. Try again in a few minutes, or without a VPN or proxy."
        ),
      };
    case "internet.unreachable":
      return {
        title: t("Roblox can't be reached"),
        detail: t("Check your internet connection, VPN or proxy, then try again."),
      };
    case "stuckProcesses.none":
      return { title: t("No stuck Roblox processes"), detail: "" };
    case "stuckProcesses.stuck":
      return {
        title: t("{{count}} Roblox process(es) running with no window", { count }),
        detail: t(
          "They have had no window for over 2 minutes and can block new launches. If you aren't playing on them, end RobloxPlayerBeta.exe in Task Manager. MultiAlt doesn't close them for you."
        ),
      };
    case "multiRoblox.off":
      return {
        title: t("Multi Roblox is off"),
        detail: t("Only one Roblox window at a time. Turn it on in Settings › Launch to play several accounts at once."),
      };
    case "multiRoblox.offWithClients":
      return {
        title: t("Multi Roblox is off and Roblox is already open"),
        detail: t(
          "Opening another account closes the one that's open, or doesn't start at all. Turn on Multi Roblox in Settings › Launch."
        ),
      };
    case "multiRoblox.legacyRam":
      return {
        title: t("The old Roblox Account Manager is holding Roblox's lock"),
        detail: t("Close the old Roblox Account Manager, then launch again."),
      };
    case "multiRoblox.held":
    case "multiRoblox.free":
      return { title: t("Multi Roblox is ready"), detail: "" };
    case "singletonReservation.reserved":
      return { title: t("Experimental teleport protection is active"), detail: "" };
    case "singletonReservation.notReserved":
      return {
        title: t("Experimental teleport protection is not active yet"),
        detail: t(
          "It starts on the next launch, once no Roblox window is holding the name. Until then the usual Multi Roblox method is used."
        ),
      };
    case "multiRoblox.clientOpen":
      return {
        title: t("Multi Roblox is ready"),
        detail: t("A Roblox window is already open; the next account opens next to it without closing it."),
      };
    // Navegador de login (`run_browser_diagnostics`, chromium/diagnose.rs).
    case "browserBinary.downloaded":
      return { title: t("MultiAlt's own browser is installed"), detail: "" };
    case "browserBinary.manual":
      return { title: t("Using the browser you picked in Settings"), detail: "" };
    case "browserBinary.manualInvalid":
      return {
        title: t("The custom browser path doesn't point to a browser file"),
        detail: t("Fix or clear Custom browser executable in Settings > Accounts. Logins and account creation stop until then."),
      };
    case "browserBinary.damaged":
      return {
        title: t("MultiAlt's browser is missing files"),
        detail: t("Usually an antivirus removed part of it. Use Reinstall in Settings > Accounts > Bundled Browser."),
      };
    case "browserBinary.systemFallback":
      return {
        title: t("Using the Chrome, Edge or Brave installed on this PC"),
        detail: t(
          "MultiAlt's own browser isn't downloaded, so logins open the browser installed on this PC. If it does nothing, download MultiAlt's copy with Download in Settings > Accounts > Bundled Browser, or reinstall that browser."
        ),
      };
    case "browserBinary.notDownloaded":
      return {
        title: t("MultiAlt's browser isn't downloaded yet"),
        detail: t("It downloads on the first browser login. You can download it now in Settings > Accounts > Bundled Browser."),
      };
    case "browserFolder.writable":
      return { title: t("The browser can save its profile"), detail: "" };
    case "browserFolder.notWritable":
      return {
        title: t("The browser can't write its profile folder"),
        detail: t("Check that MultiAlt's data folder isn't read-only or blocked by an antivirus, then restart MultiAlt."),
      };
    case "browserFolder.unknown":
      return { title: t("Couldn't find the browser profile folder"), detail: t("Restart the PC and open MultiAlt again.") };
    case "browserLoginOpen.open":
      return {
        title: t("A login window is open right now"),
        detail: t("If it looks stuck, close it and try the login again."),
      };
    case "browserStart.opened":
      return {
        title: t("The browser opens and answers MultiAlt"),
        detail: count > 10 ? t("It took {{count}} seconds; an antivirus scanning it can make logins slow.", { count }) : "",
      };
    case "browserStart.spawnFailed":
      return {
        title: t("Windows did not let the browser start"),
        detail: t(
          "An antivirus may be blocking it. Allow MultiAlt and its browser in the antivirus, or use Reinstall in Settings > Accounts > Bundled Browser."
        ),
      };
    case "browserStart.didNotOpen":
      return {
        title: t("The browser never answered MultiAlt"),
        detail: t(
          "This is the browser-does-nothing case. Use Reinstall in Settings > Accounts > Bundled Browser; if logins use your own Chrome or Edge, reinstall that browser. Check the antivirus too."
        ),
      };
    case "browserStart.skipped":
      return {
        title: t("There is no browser to test"),
        detail: t("Download MultiAlt's browser in Settings > Accounts > Bundled Browser and check again."),
      };
    case "browserRoblox.reachable":
      return { title: t("The Roblox login page loads in the browser"), detail: "" };
    case "browserRoblox.unreachable":
      return {
        title: t("The Roblox login page didn't load in the browser"),
        detail: t("Check the internet, a VPN or proxy, and whether a firewall blocks the browser."),
      };
    default:
      return { title: `${check.id}: ${check.reason}`, detail: "" };
  }
}

/** O pior estado da lista — o que a tela diz no topo. */
export function overallStatus(checks: DiagnosticCheck[]): DiagnosticStatus {
  if (checks.some((c) => c.status === "problem")) return "problem";
  if (checks.some((c) => c.status === "warn")) return "warn";
  return "ok";
}

/**
 * Uma linha por checagem, em inglês e sem tradução: é o que vai no resumo do
 * "Reportar problema" (quem lê é o dono do projeto, no GitHub).
 */
export function diagnosticsSummaryLines(checks: DiagnosticCheck[]): string[] {
  return checks.map(
    (c) => `- ${c.id}: ${c.status} (${c.reason}${c.count !== undefined ? `, ${c.count}` : ""})`
  );
}
