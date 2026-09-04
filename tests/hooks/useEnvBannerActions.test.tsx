import { renderHook, act } from "@testing-library/react";
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";
import { useEnvBannerActions } from "@/hooks/useEnvBannerActions";

const checkAllEnvConflictsMock = vi.fn();

const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

const renderUseEnvBannerActions = (
  setEnvConflicts = vi.fn(),
  setShowEnvBanner = vi.fn(),
) => {
  const rendered = renderHook(() =>
    useEnvBannerActions({
      setEnvConflicts,
      setShowEnvBanner,
      checkAllEnvConflictsFn: checkAllEnvConflictsMock,
    }),
  );

  return {
    ...rendered,
    setEnvConflicts,
    setShowEnvBanner,
  };
};

beforeEach(() => {
  sessionStorage.clear();
  checkAllEnvConflictsMock.mockReset();
  consoleErrorSpy.mockClear();
});

afterAll(() => {
  consoleErrorSpy.mockRestore();
});

describe("useEnvBannerActions", () => {
  it("dismisses banner and records session flag", () => {
    const { result, setShowEnvBanner } = renderUseEnvBannerActions();

    act(() => {
      result.current.handleEnvBannerDismiss();
    });

    expect(setShowEnvBanner).toHaveBeenCalledWith(false);
    expect(sessionStorage.getItem("env_banner_dismissed")).toBe("true");
  });

  it("refreshes conflicts and keeps banner visible when conflicts remain", async () => {
    const conflict = {
      varName: "OPENAI_API_KEY",
      varValue: "test",
      sourceType: "file" as const,
      sourcePath: "/tmp/.env:1",
    };
    checkAllEnvConflictsMock.mockResolvedValueOnce({
      claude: [conflict],
      codex: [],
    });

    const { result, setEnvConflicts, setShowEnvBanner } =
      renderUseEnvBannerActions();

    await act(async () => {
      await result.current.handleEnvBannerDeleted();
    });

    expect(setEnvConflicts).toHaveBeenCalledWith([conflict]);
    expect(setShowEnvBanner).not.toHaveBeenCalled();
  });

  it("hides banner when no conflicts remain after deletion", async () => {
    checkAllEnvConflictsMock.mockResolvedValueOnce({
      claude: [],
      codex: [],
    });

    const { result, setEnvConflicts, setShowEnvBanner } =
      renderUseEnvBannerActions();

    await act(async () => {
      await result.current.handleEnvBannerDeleted();
    });

    expect(setEnvConflicts).toHaveBeenCalledWith([]);
    expect(setShowEnvBanner).toHaveBeenCalledWith(false);
  });

  it("logs error when refreshing conflicts fails", async () => {
    checkAllEnvConflictsMock.mockRejectedValueOnce(new Error("network failed"));

    const { result, setEnvConflicts, setShowEnvBanner } =
      renderUseEnvBannerActions();

    await act(async () => {
      await result.current.handleEnvBannerDeleted();
    });

    expect(setEnvConflicts).not.toHaveBeenCalled();
    expect(setShowEnvBanner).not.toHaveBeenCalled();
    expect(consoleErrorSpy).toHaveBeenCalledTimes(1);
  });
});
