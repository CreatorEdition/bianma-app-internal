import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { TFunction } from "i18next";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  getDeleteDialogContent,
  useSessionDeleteActions,
} from "@/components/sessions/hooks/useSessionDeleteActions";
import { getSessionKey } from "@/components/sessions/utils";
import { sessionsApi } from "@/lib/api";
import type { SessionMeta } from "@/types";

const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));

const t = ((key: string, options?: Record<string, unknown>) => {
  if (!options) {
    return key;
  }

  const defaultValue =
    typeof options.defaultValue === "string" ? options.defaultValue : key;

  return defaultValue.replace(/\{\{(\w+)\}\}/g, (_, token: string) =>
    String(options[token] ?? ""),
  );
}) as unknown as TFunction;

const createSession = (overrides?: Partial<SessionMeta>): SessionMeta => ({
  providerId: "codex",
  sessionId: "session-1",
  title: "Session One",
  summary: "summary",
  projectDir: "/mock/project-1",
  createdAt: 1,
  lastActiveAt: 2,
  sourcePath: "/mock/project-1/session-1.jsonl",
  resumeCommand: "codex resume session-1",
  ...overrides,
});

const createWrapper = (client: QueryClient) => {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
};

describe("getDeleteDialogContent", () => {
  it("returns single delete copy for one target", () => {
    const result = getDeleteDialogContent([createSession()], t);

    expect(result.title).toBe("删除会话");
    expect(result.confirmText).toBe("删除会话");
    expect(result.message).toContain("Session ID: session-1");
    expect(result.message).toContain("Session One");
  });

  it("returns batch delete copy for multiple targets", () => {
    const result = getDeleteDialogContent(
      [createSession(), createSession({ sessionId: "session-2" })],
      t,
    );

    expect(result.title).toBe("批量删除会话");
    expect(result.confirmText).toBe("删除所选会话");
    expect(result.message).toContain("2");
  });

  it("returns empty message when there are no targets", () => {
    const result = getDeleteDialogContent(null, t);

    expect(result.title).toBe("删除会话");
    expect(result.confirmText).toBe("删除会话");
    expect(result.message).toBe("");
  });
});

describe("useSessionDeleteActions", () => {
  beforeEach(() => {
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
  });

  it("deletes one session through the delete mutation and removes its selected key", async () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    const session = createSession();
    const removeSelectedKeys = vi.fn();
    client.setQueryData(["sessions"], [session]);

    const { result } = renderHook(
      () =>
        useSessionDeleteActions({
          t,
          selectedSession: session,
          selectedDeletableSessions: [],
          removeSelectedKeys,
        }),
      {
        wrapper: createWrapper(client),
      },
    );

    act(() => {
      result.current.openSingleDeleteDialog();
    });

    await act(async () => {
      await result.current.handleDeleteConfirm();
    });

    await waitFor(() =>
      expect(removeSelectedKeys).toHaveBeenCalledWith([getSessionKey(session)]),
    );
    expect(client.getQueryData<SessionMeta[]>(["sessions"])).toEqual([]);
    expect(toastErrorMock).not.toHaveBeenCalled();
  });

  it("batch deletes selected sessions and removes successful items from cache", async () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    const sessions = [
      createSession(),
      createSession({
        providerId: "claude",
        sessionId: "session-2",
        sourcePath: "/mock/project-1/session-2.jsonl",
      }),
    ];
    const removeSelectedKeys = vi.fn();
    client.setQueryData(["sessions"], sessions);

    const { result } = renderHook(
      () =>
        useSessionDeleteActions({
          t,
          selectedSession: null,
          selectedDeletableSessions: sessions,
          removeSelectedKeys,
        }),
      {
        wrapper: createWrapper(client),
      },
    );

    act(() => {
      result.current.openBatchDeleteDialog();
    });

    await act(async () => {
      await result.current.handleDeleteConfirm();
    });

    const deletedKeys = sessions.map((session) => getSessionKey(session));
    expect(removeSelectedKeys).toHaveBeenCalledWith(deletedKeys);
    expect(client.getQueryData<SessionMeta[]>(["sessions"])).toEqual([]);
    expect(result.current.isBatchDeleting).toBe(false);
    expect(toastSuccessMock).toHaveBeenCalledWith("已删除 2 个会话");
  });

  it("restores batch deleting state and reports request failures", async () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    const sessions = [
      createSession(),
      createSession({
        sessionId: "session-2",
        sourcePath: "/mock/project-1/session-2.jsonl",
      }),
    ];
    const deleteManySpy = vi
      .spyOn(sessionsApi, "deleteMany")
      .mockRejectedValueOnce(new Error("network error"));

    const { result } = renderHook(
      () =>
        useSessionDeleteActions({
          t,
          selectedSession: null,
          selectedDeletableSessions: sessions,
          removeSelectedKeys: vi.fn(),
        }),
      {
        wrapper: createWrapper(client),
      },
    );

    act(() => {
      result.current.openBatchDeleteDialog();
    });

    await act(async () => {
      await result.current.handleDeleteConfirm();
    });

    expect(result.current.isBatchDeleting).toBe(false);
    expect(toastErrorMock).toHaveBeenCalledWith("network error");

    deleteManySpy.mockRestore();
  });
});
