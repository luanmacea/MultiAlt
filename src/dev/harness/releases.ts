/**
 * Releases do GitHub para o cenário `changelog` do harness, no formato que
 * `GET /repos/<dono>/<repo>/releases` devolve. Os corpos copiam o formato real
 * de cada época (ver release-v4.yml): quem tem que limpar é o app.
 */
const REPO = "https://github.com/luanmacea/roblox-account-manager";

const CONTRIBUTORS = [
  "## Contributors",
  '<a href="https://github.com/luanmacea"><img src="https://github.com/luanmacea.png?size=64" width="32" height="32" alt="@luanmacea" /></a>',
  "",
  "[@luanmacea](https://github.com/luanmacea)",
].join("\n");

function oldBody(version: string, items: string[], previous: string): string {
  return [
    "> [!WARNING]",
    "> This is a beta release. Missing features, bugs and crashes are possible. Run at your own risk.",
    "",
    "Channel: Beta",
    "Release commit: 15f422b",
    `App version: ${version}`,
    "",
    "### Which file to download",
    "**Start here: `MultiAlt-Setup.msi`.** It installs for your user only (no admin prompt).",
    "",
    ...(items.length
      ? ["## What's Changed", ...items.map((item) => `* ${item}`), "", ""]
      : []),
    `**Full Changelog**: ${REPO}/compare/v${previous}-beta...v${version}-beta`,
    "",
    CONTRIBUTORS,
  ].join("\n");
}

function release(version: string, publishedAt: string, body: string, extra: Record<string, unknown> = {}) {
  return {
    tag_name: `v${version}-beta`,
    name: `MultiAlt (Roblox Account Manager) v${version}-beta`,
    html_url: `${REPO}/releases/tag/v${version}-beta`,
    draft: false,
    prerelease: false,
    created_at: publishedAt,
    published_at: publishedAt,
    body,
    ...extra,
  };
}

export const HARNESS_RELEASES = [
  release(
    "0.1.10",
    "2026-10-03T22:20:37Z",
    [
      "## Download",
      "",
      `### [⬇ Download MultiAlt-Setup.msi](${REPO}/releases/download/v0.1.10-beta/MultiAlt-Setup.msi)`,
      "",
      "Open the file and follow the installer. That's it.",
      "",
      "> [!NOTE]",
      "> This is a beta version: some things may still change.",
      "",
      "<details>",
      "<summary>Other files (you can ignore these)</summary>",
      "",
      "- `.sig` files: used by the automatic update.",
      "",
      "</details>",
      "",
      "## What's Changed",
      "- New menu on the left, with every screen named.",
      "- AFK Mode in one place: AFK clicks and Auto Rejoin.",
      "- Accounts open side by side instead of on top of each other.",
      "- Games opened from the Roblox website now show up in the app.",
      "- New language: Spanish.",
      '- "Hide names" now hides names everywhere.',
      "",
      "<details>",
      "<summary>Technical details</summary>",
      "",
      `* Sidebar navigation, unified AFK Mode, window grid on launch by @luanmacea in ${REPO}/pull/16`,
      "",
      `**Full Changelog**: ${REPO}/compare/v0.1.9-beta...v0.1.10-beta`,
      "",
      "</details>",
      "",
      CONTRIBUTORS,
    ].join("\n")
  ),
  release(
    "0.1.9",
    "2026-10-03T08:01:20Z",
    oldBody(
      "0.1.9",
      [
        `README: blue download button and website link [skip release] by @luanmacea in ${REPO}/pull/11`,
        `Website deploys from main instead of develop [skip release] by @luanmacea in ${REPO}/pull/13`,
        `Rename to MultiAlt (Roblox Account Manager) by @luanmacea in ${REPO}/pull/15`,
      ],
      "0.1.8"
    )
  ),
  // Rascunho: nunca aparece.
  release("0.1.11", "2026-10-04T10:00:00Z", "## What's Changed\n- Draft that never shipped", { draft: true }),
  release(
    "0.1.8",
    "2026-10-03T06:07:02Z",
    oldBody("0.1.8", [`MSI-only releases and a one-click download button in the README by @luanmacea in ${REPO}/pull/10`], "0.1.7")
  ),
  release(
    "0.1.7",
    "2026-10-03T05:11:36Z",
    oldBody(
      "0.1.7",
      [`Smoother updates: silent install with an in-app installing screen, desktop shortcut kept in place by @luanmacea in ${REPO}/pull/9`],
      "0.1.6"
    ),
    { prerelease: true }
  ),
  release(
    "0.1.6",
    "2026-10-03T03:44:53Z",
    oldBody(
      "0.1.6",
      [`Free avatars: build avatars from free official Roblox items and distribute them across accounts by @luanmacea in ${REPO}/pull/8`],
      "0.1.5"
    ),
    { prerelease: true }
  ),
  release(
    "0.1.5",
    "2026-09-30T04:15:47Z",
    oldBody(
      "0.1.5",
      [
        `README in English with an updated feature list by @luanmacea in ${REPO}/pull/6`,
        `AFK click that Roblox registers, and per-account exceptions that don't leak by @luanmacea in ${REPO}/pull/7`,
      ],
      "0.1.4"
    ),
    { prerelease: true }
  ),
  // Sem "What's Changed" (v0.1.2 de verdade): a página pula.
  release("0.1.2", "2026-09-28T18:01:45Z", oldBody("0.1.2", [], "0.1.1"), { prerelease: true }),
  release(
    "0.1.0",
    "2026-09-28T08:07:25Z",
    oldBody("0.1.0", [`Instaladores sem falso positivo e versão 0.1.0 by @luanmacea in ${REPO}/pull/3`], "0.0.9"),
    { prerelease: true }
  ),
];
