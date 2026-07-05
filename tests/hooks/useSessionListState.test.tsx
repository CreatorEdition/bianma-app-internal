import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SessionMeta } from "@/types";
import { useSessionListState } from "@/components/sessions/hooks/useSessionListState";

const sessions: SessionMeta[] = [
  {
    providerId: "codex",
    sessionId: "codex-session-old",
    title: "Codex Old",
    summary: "old",
    projectDir: "/mock/codex",
    createdAt: 1,
    lastActiveAt: 10,
    sourcePath: "/mock/codex/old.jsonl",
    resumeCommand: "codex resume codex-session-old",
  },
  {
    providerId: "claude",
    sessionId: "claude-session-mid",
    title: "Claude Mid",
    summary: "mid",
    projectDir: "/mock/claude",
    createdAt: 2,
    lastActiveAt: 20,
    sourcePath: "/mock/claude/mid.jsonl",
    resumeCommand: "claude resume claude-session-mid",
  },
  {
    providerId: "codex",
    sessionId: "codex-session-new",
    title: "Codex New",
    summary: "new",
    projectDir: "/mock/codex",
    createdAt: 3,
    lastActiveAt: 30,
    sourcePath: "/mock/codex/new.jsonl",
    resumeCommand: "codex resume codex-session-new",
  },
];

describe("useSessionListState", () => {
  it("initializes provider filter from app id and selects the newest visible session", async () => {
    const { result } = renderHook(() =>
      useSessionListState({
        sessions,
        appId: "codex",
      }),
    );

    expect(result.current.providerFilter).toBe("codex");
    expect(
      result.current.filteredSessions.map((session) => session.title),
    ).toEqual(["Codex New", "Codex Old"]);

    await waitFor(() => {
      expect(result.current.selectedSession?.title).toBe("Codex New");
    });
  });

  it("falls back provider filter to all for unknown app id", async () => {
    const { result } = renderHook(() =>
      useSessionListState({
        sessions,
        appId: "unknown-app",
      }),
    );

    expect(result.current.providerFilter).toBe("all");
    expect(
      result.current.filteredSessions.map((session) => session.title),
    ).toEqual(["Codex New", "Claude Mid", "Codex Old"]);

    await waitFor(() => {
      expect(result.current.selectedSession?.title).toBe("Codex New");
    });
  });

  it("updates selected session when search and provider filter narrow results", async () => {
    const { result } = renderHook(() =>
      useSessionListState({
        sessions,
        appId: "codex",
      }),
    );

    await waitFor(() => {
      expect(result.current.selectedSession?.title).toBe("Codex New");
    });

    act(() => {
      result.current.setProviderFilter("all");
      result.current.setSearch("Claude");
    });

    await waitFor(() => {
      expect(result.current.filteredSessions).toHaveLength(1);
      expect(result.current.filteredSessions[0].title).toBe("Claude Mid");
      expect(result.current.selectedSession?.title).toBe("Claude Mid");
    });
  });

  it("clears selected key and selected session when filtered results are empty", async () => {
    const { result } = renderHook(() =>
      useSessionListState({
        sessions,
        appId: "codex",
      }),
    );

    await waitFor(() => {
      expect(result.current.selectedKey).not.toBeNull();
    });

    act(() => {
      result.current.setSearch("NoSuchSession");
    });

    await waitFor(() => {
      expect(result.current.filteredSessions).toHaveLength(0);
      expect(result.current.selectedKey).toBeNull();
      expect(result.current.selectedSession).toBeNull();
    });
  });
});
