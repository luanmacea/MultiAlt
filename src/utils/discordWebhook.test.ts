import { describe, expect, it } from "vitest";
import { INVALID_WEBHOOK_URL_MESSAGE, isDiscordWebhookUrl, webhookUrlProblem } from "./discordWebhook";

const ID = "123456789012345678";
const TOKEN = "AbCdEf_ghIJ-klmnopQRSTuvwxyz0123456789";
const url = (prefix: string) => `${prefix}/${ID}/${TOKEN}`;

// Os mesmos casos de `discord_webhook_url_tests` (Rust): os dois lados
// precisam aceitar e recusar igual.
describe("discord webhook URL", () => {
  it("accepts every Discord webhook host", () => {
    for (const prefix of [
      "https://discord.com/api/webhooks",
      "https://discordapp.com/api/webhooks",
      "https://ptb.discord.com/api/webhooks",
      "https://canary.discord.com/api/webhooks",
      "https://ptb.discordapp.com/api/webhooks",
      "https://canary.discordapp.com/api/webhooks",
      "https://DISCORD.com/api/webhooks",
      "https://discord.com/api/v10/webhooks",
    ]) {
      expect(isDiscordWebhookUrl(url(prefix)), prefix).toBe(true);
    }
    expect(isDiscordWebhookUrl(`  ${url("https://discord.com/api/webhooks")}/  `)).toBe(true);
  });

  it("refuses anything else", () => {
    for (const bad of [
      "",
      "not a url",
      url("http://discord.com/api/webhooks"),
      url("https://discord.com.evil.com/api/webhooks"),
      url("https://evil.com/api/webhooks"),
      url("https://discord.com:8443/api/webhooks"),
      url("https://user:pw@discord.com/api/webhooks"),
      url("https://discord.com/api/channels"),
      url("https://discord.com/webhooks"),
      `${url("https://discord.com/api/webhooks")}?x=1`,
      `${url("https://discord.com/api/webhooks")}#x`,
      `https://discord.com/api/webhooks/${ID}`,
      `https://discord.com/api/webhooks/${ID}/${TOKEN}/extra`,
      `https://discord.com/api/webhooks/abc/${TOKEN}`,
      `https://discord.com/api/webhooks/${ID}/short`,
      `https://discord.com/api/webhooks/${ID}/tok%20en${TOKEN}`,
    ]) {
      expect(isDiscordWebhookUrl(bad), bad).toBe(false);
    }
  });

  it("an empty field is fine (sending stays off); a wrong one says why", () => {
    expect(webhookUrlProblem("  ")).toBeNull();
    expect(webhookUrlProblem(url("https://discord.com/api/webhooks"))).toBeNull();
    expect(webhookUrlProblem("https://example.com")).toBe(INVALID_WEBHOOK_URL_MESSAGE);
  });
});
