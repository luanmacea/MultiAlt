import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { RecentJobsList } from "./RecentJobsList";
import { loadRecentJobs, saveRecentJobs, type RecentJobEntry } from "./types";
import { resetPromptMocks } from "../../test-utils/promptMocks";

const JOB = "11111111-2222-3333-4444-555555555555";

function entry(overrides: Partial<RecentJobEntry> = {}): RecentJobEntry {
  return {
    kind: "job",
    raw: JOB,
    placeId: 606849621,
    lastUsed: Date.now(),
    userIds: [1],
    ...overrides,
  };
}

function renderJobs(entries: RecentJobEntry[], userId: number | null) {
  saveRecentJobs(entries);
  const onSelect = vi.fn();
  render(<RecentJobsList userId={userId} maxRecent={12} onSelect={onSelect} />);
  return onSelect;
}

beforeEach(() => {
  localStorage.clear();
  resetPromptMocks();
});

afterEach(cleanup);

describe("RecentJobsList", () => {
  it("fills the Job ID with the raw target, place included", async () => {
    const onSelect = renderJobs([entry()], 1);

    await userEvent.click(screen.getByRole("button", { name: new RegExp(JOB.slice(0, 12)) }));

    expect(onSelect).toHaveBeenCalledWith(606849621, JOB);
  });

  it("shows the vip: target as typed, so the meaning is not lost", () => {
    renderJobs([entry({ kind: "vip", raw: "vip:abc123", userIds: [1] })], 1);
    expect(screen.getByText("vip:abc123")).toBeInTheDocument();
    expect(screen.getByText("VIP")).toBeInTheDocument();
  });

  /**
   * O que impede o vazamento é esta tela usar `visibleRecentJobs`: um link
   * privado de outra conta não pode aparecer para a conta selecionada.
   */
  it("does not show another account's private target", () => {
    renderJobs([entry({ kind: "vip", raw: "vip:abc123", userIds: [7] })], 8);
    expect(screen.queryByText("vip:abc123")).not.toBeInTheDocument();
    expect(screen.getByText("No recent servers")).toBeInTheDocument();
  });

  it("still shows a public job id to another account", () => {
    renderJobs([entry({ userIds: [7] })], 8);
    expect(screen.getByRole("button", { name: new RegExp(JOB.slice(0, 12)) })).toBeInTheDocument();
  });

  it("removes one entry without touching the others", async () => {
    renderJobs([entry(), entry({ raw: "job-two", placeId: null })], 1);

    await userEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]);

    expect(loadRecentJobs().map((e) => e.raw)).toEqual(["job-two"]);
  });
});
