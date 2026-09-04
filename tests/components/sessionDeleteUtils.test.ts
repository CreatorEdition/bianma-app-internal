import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import type { DeleteSessionResult } from "@/lib/api/sessions";
import type { SessionMeta } from "@/types";
import {
  getDeletableSessions,
  getDeleteResultSummary,
  toDeleteSessionOptions,
} from "@/components/sessions/deleteUtils";

const sessions: SessionMeta[] = [
  {
    providerId: "codex",
    sessionId: "session-1",
    title: "Session One",
    summary: "summary",
    projectDir: "/mock/project-1",
    createdAt: 1,
    lastActiveAt: 2,
    sourcePath: "/mock/project-1/session-1.jsonl",
    resumeCommand: "codex resume session-1",
  },
  {
    providerId: "claude",
    sessionId: "session-2",
    title: "Session Two",
    summary: "summary",
    projectDir: "/mock/project-2",
    createdAt: 3,
    lastActiveAt: 4,
    sourcePath: undefined,
    resumeCommand: "claude resume session-2",
  },
];

const t = ((key: string) => key) as unknown as TFunction;

describe("session delete utils", () => {
  it("filters non-deletable sessions", () => {
    expect(getDeletableSessions(null)).toEqual([]);
    expect(getDeletableSessions(sessions)).toEqual([sessions[0]]);
  });

  it("maps deletable sessions to delete options", () => {
    expect(toDeleteSessionOptions(sessions)).toEqual([
      {
        providerId: "codex",
        sessionId: "session-1",
        sourcePath: "/mock/project-1/session-1.jsonl",
      },
    ]);
  });

  it("builds deleted key list and failed error list from delete results", () => {
    const results: DeleteSessionResult[] = [
      {
        providerId: "codex",
        sessionId: "session-1",
        sourcePath: "/mock/project-1/session-1.jsonl",
        success: true,
      },
      {
        providerId: "claude",
        sessionId: "session-2",
        sourcePath: "/mock/project-2/session-2.jsonl",
        success: false,
      },
      {
        providerId: "gemini",
        sessionId: "session-3",
        sourcePath: "/mock/project-3/session-3.jsonl",
        success: false,
        error: "network error",
      },
    ];

    const summary = getDeleteResultSummary(results, t);

    expect(summary.deletedKeys).toEqual([
      "codex:session-1:/mock/project-1/session-1.jsonl",
    ]);
    expect(summary.failedErrors).toEqual(["common.unknown", "network error"]);
  });
});
