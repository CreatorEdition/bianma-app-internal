import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SessionMeta } from "@/types";
import { getSessionKey } from "@/components/sessions/utils";
import { useSessionSelectionState } from "@/components/sessions/hooks/useSessionSelectionState";

const sessions: SessionMeta[] = [
  {
    providerId: "codex",
    sessionId: "codex-1",
    title: "Codex 1",
    summary: "summary",
    projectDir: "/mock/codex",
    createdAt: 1,
    lastActiveAt: 10,
    sourcePath: "/mock/codex/1.jsonl",
    resumeCommand: "codex resume codex-1",
  },
  {
    providerId: "claude",
    sessionId: "claude-1",
    title: "Claude 1",
    summary: "summary",
    projectDir: "/mock/claude",
    createdAt: 2,
    lastActiveAt: 20,
    sourcePath: "/mock/claude/1.jsonl",
    resumeCommand: "claude resume claude-1",
  },
  {
    providerId: "gemini",
    sessionId: "gemini-no-source",
    title: "Gemini no source",
    summary: "summary",
    projectDir: "/mock/gemini",
    createdAt: 3,
    lastActiveAt: 30,
    sourcePath: undefined,
    resumeCommand: "gemini resume gemini-no-source",
  },
];

describe("useSessionSelectionState", () => {
  it("toggles session checked state and ignores non-deletable sessions", () => {
    const { result } = renderHook(() =>
      useSessionSelectionState({
        sessions,
        filteredSessions: sessions,
        selectionMode: false,
      }),
    );

    act(() => {
      result.current.toggleSessionChecked(sessions[0], true);
      result.current.toggleSessionChecked(sessions[2], true);
    });

    expect(
      result.current.selectedSessionKeys.has(getSessionKey(sessions[0])),
    ).toBe(true);
    expect(
      result.current.selectedSessionKeys.has(getSessionKey(sessions[2])),
    ).toBe(false);
  });

  it("supports select all and clear selection", () => {
    const { result } = renderHook(() =>
      useSessionSelectionState({
        sessions,
        filteredSessions: sessions,
        selectionMode: true,
      }),
    );

    act(() => {
      result.current.toggleSelectAll();
    });

    expect(result.current.selectedDeletableSessions).toHaveLength(2);
    expect(result.current.allFilteredSelected).toBe(true);

    act(() => {
      result.current.clearSelection();
    });

    expect(result.current.selectedSessionKeys.size).toBe(0);
  });

  it("drops hidden selections when filtered list narrows in selection mode", async () => {
    const { result, rerender } = renderHook(
      ({
        filteredSessions,
        selectionMode,
      }: {
        filteredSessions: SessionMeta[];
        selectionMode: boolean;
      }) =>
        useSessionSelectionState({
          sessions,
          filteredSessions,
          selectionMode,
        }),
      {
        initialProps: {
          filteredSessions: sessions,
          selectionMode: true,
        },
      },
    );

    act(() => {
      result.current.toggleSelectAll();
    });
    expect(result.current.selectedDeletableSessions).toHaveLength(2);

    rerender({
      filteredSessions: [sessions[0]],
      selectionMode: true,
    });

    await waitFor(() => {
      expect(result.current.selectedDeletableSessions).toHaveLength(1);
      expect(
        result.current.selectedSessionKeys.has(getSessionKey(sessions[0])),
      ).toBe(true);
    });
  });

  it("removes selected keys after single or batch deletion succeeds", () => {
    const { result } = renderHook(() =>
      useSessionSelectionState({
        sessions,
        filteredSessions: sessions,
        selectionMode: true,
      }),
    );

    act(() => {
      result.current.toggleSelectAll();
      result.current.removeSelectedKeys([getSessionKey(sessions[0])]);
    });

    expect(
      result.current.selectedSessionKeys.has(getSessionKey(sessions[0])),
    ).toBe(false);
    expect(
      result.current.selectedSessionKeys.has(getSessionKey(sessions[1])),
    ).toBe(true);

    act(() => {
      result.current.removeSelectedKeys([getSessionKey(sessions[1])]);
    });

    expect(result.current.selectedSessionKeys.size).toBe(0);
  });
});
