import { act, renderHook } from "@testing-library/react";
import type { TFunction } from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSessionActions } from "@/components/sessions/hooks/useSessionActions";
import { sessionsApi } from "@/lib/api";
import { isMac } from "@/lib/platform";
import type { SessionMeta } from "@/types";

const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));

vi.mock("@/lib/platform", () => ({
  isMac: vi.fn(() => false),
}));

const isMacMock = vi.mocked(isMac);

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

const t = ((key: string, options?: Record<string, unknown>) => {
  if (
    options &&
    "defaultValue" in options &&
    typeof options.defaultValue === "string"
  ) {
    return options.defaultValue;
  }

  return key;
}) as unknown as TFunction;

describe("useSessionActions", () => {
  beforeEach(() => {
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
    isMacMock.mockReturnValue(false);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    });
  });

  it("copies text and shows success toast", async () => {
    const { result } = renderHook(() =>
      useSessionActions({
        t,
        selectedSession: null,
      }),
    );

    await act(async () => {
      await result.current.handleCopy("hello", "copied");
    });

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("hello");
    expect(toastSuccessMock).toHaveBeenCalledWith("copied");
  });

  it("shows error toast when copy fails", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn().mockRejectedValue(new Error("copy failed")),
      },
    });

    const { result } = renderHook(() =>
      useSessionActions({
        t,
        selectedSession: null,
      }),
    );

    await act(async () => {
      await result.current.handleCopy("hello", "copied");
    });

    expect(toastErrorMock).toHaveBeenCalledWith("copy failed");
    expect(toastSuccessMock).not.toHaveBeenCalled();
  });

  it("copies resume command on non-mac", async () => {
    const { result } = renderHook(() =>
      useSessionActions({
        t,
        selectedSession: createSession(),
      }),
    );

    await act(async () => {
      await result.current.handleResume();
    });

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      "codex resume session-1",
    );
    expect(toastSuccessMock).toHaveBeenCalledWith(
      "sessionManager.resumeCommandCopied",
    );
  });

  it("launches terminal on mac", async () => {
    isMacMock.mockReturnValue(true);
    const launchSpy = vi
      .spyOn(sessionsApi, "launchTerminal")
      .mockResolvedValue(true);
    const { result } = renderHook(() =>
      useSessionActions({
        t,
        selectedSession: createSession(),
      }),
    );

    await act(async () => {
      await result.current.handleResume();
    });

    expect(launchSpy).toHaveBeenCalledWith({
      command: "codex resume session-1",
      cwd: "/mock/project-1",
    });
    expect(toastSuccessMock).toHaveBeenCalledWith(
      "sessionManager.terminalLaunched",
    );
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();

    launchSpy.mockRestore();
  });

  it("copies fallback command and shows error toast when mac launch fails", async () => {
    isMacMock.mockReturnValue(true);
    const launchSpy = vi
      .spyOn(sessionsApi, "launchTerminal")
      .mockRejectedValue(new Error("launch failed"));
    const { result } = renderHook(() =>
      useSessionActions({
        t,
        selectedSession: createSession(),
      }),
    );

    await act(async () => {
      await result.current.handleResume();
    });

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      "codex resume session-1",
    );
    expect(toastSuccessMock).toHaveBeenCalledWith(
      "sessionManager.resumeFallbackCopied",
    );
    expect(toastErrorMock).toHaveBeenCalledWith("launch failed");

    launchSpy.mockRestore();
  });

  it("does nothing when resume command is missing", async () => {
    const launchSpy = vi.spyOn(sessionsApi, "launchTerminal");
    const { result } = renderHook(() =>
      useSessionActions({
        t,
        selectedSession: createSession({ resumeCommand: "" }),
      }),
    );

    await act(async () => {
      await result.current.handleResume();
    });

    expect(launchSpy).not.toHaveBeenCalled();
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
    expect(toastSuccessMock).not.toHaveBeenCalled();
    expect(toastErrorMock).not.toHaveBeenCalled();

    launchSpy.mockRestore();
  });
});
