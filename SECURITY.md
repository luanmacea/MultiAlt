# Security Policy

## Supported versions

Only the latest release gets fixes. The app updates itself; you can also download it again from the [latest release](https://github.com/luanmacea/MultiAlt/releases/latest).

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report it privately through GitHub: on the repository's **Security** tab, choose **Report a vulnerability** ([direct link](https://github.com/luanmacea/MultiAlt/security/advisories/new)). Include what you found, how to reproduce it and which version you used. Never include real cookies or passwords — a description of where a secret leaks is enough.

You will get an answer as soon as possible, and you will be credited in the fix's release notes unless you prefer not to be.

## What MultiAlt does with your data

- Your accounts are stored only on your computer, encrypted (`AccountData.json`, in `%LOCALAPPDATA%\Roblox Account Manager`).
- The app has no server of its own and sends no analytics. It talks to Roblox, to this GitHub repository (update checks and release notes), and to a few public services only when you use the feature that needs them: the Roblox version list (weao.xyz), the server region lookup (ipwho.is, ip-api.com as a fallback) and the browser used for signing in (Google's Chrome for Testing). Your cookies and passwords go only to Roblox.
- The optional local web server and Nexus are off by default and only exist in the complete edition.

## Downloading safely

Download MultiAlt only from this repository's [Releases](https://github.com/luanmacea/MultiAlt/releases) or from the official site linked in the README. Copies from other places may be modified.
