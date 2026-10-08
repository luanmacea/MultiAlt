# Contributing to MultiAlt

Thanks for wanting to help! Bug reports and ideas are as welcome as code.

## Reporting a problem or suggesting an idea

Use the forms on the [Issues page](https://github.com/luanmacea/MultiAlt/issues/new/choose) — or the **Send feedback** button inside the app, which opens the same forms. English, Português or Español are fine.

**Never paste cookies, passwords or `.ROBLOSECURITY` values.** Issues are public. Hide account names in screenshots (the app's *Names hidden* button).

Security problems go through the [security policy](SECURITY.md), not public issues.

## Working on the code

MultiAlt is a [Tauri 2](https://tauri.app) app: Rust backend (`src-tauri/`) and React + TypeScript frontend (`src/`). Windows is the main platform.

```bash
bun install
bun run dev:ui      # the real interface in the browser with a fake backend (no Roblox, no accounts)
bun run tauri dev   # the full desktop app
bun run check       # typecheck + tests (frontend and Rust) — must pass before a pull request
```

- Day-to-day work happens on `develop`; open pull requests against `develop`. `main` is the release branch: every merge into it publishes a new version.
- Write the failing test first when fixing a bug, and keep the test in the feature's suite (`bun run t --list`, map in `scripts/test-suites.ts`).
- Code and commit messages in English. Project documentation (`docs/`) is in Portuguese.
- The app never opens Roblox through the raw `roblox-player:` handler, never changes the user's Roblox channel and never closes other accounts' clients — see `docs/features/launch.md` before touching the launch flow.
- Keep pull requests focused, and fill in the **What's new** section of the template: it becomes the release notes users read, so write it in short, plain sentences.

The full guide is in [docs/development.md](docs/development.md).

## License

By contributing you agree that your contribution is licensed under the [GPL-3.0](LICENSE), like the rest of the project.
