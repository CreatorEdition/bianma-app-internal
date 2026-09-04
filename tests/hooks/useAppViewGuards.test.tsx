import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppId } from "@/lib/api";
import type { VisibleApps } from "@/types";
import {
  getFirstVisibleApp,
  hasSessionSupportForApp,
  useAppViewGuards,
  type AppViewGuardView,
} from "@/hooks/useAppViewGuards";

const allVisible: VisibleApps = {
  claude: true,
  codex: true,
  gemini: true,
  opencode: true,
  openclaw: true,
};

function renderGuards({
  activeApp = "claude",
  currentView = "home",
  visibleApps = allVisible,
}: {
  activeApp?: AppId;
  currentView?: AppViewGuardView;
  visibleApps?: VisibleApps;
} = {}) {
  const setActiveApp = vi.fn();
  const setCurrentView = vi.fn();

  renderHook(() =>
    useAppViewGuards({
      activeApp,
      currentView,
      visibleApps,
      setActiveApp,
      setCurrentView,
    }),
  );

  return { setActiveApp, setCurrentView };
}

describe("useAppViewGuards", () => {
  it("keeps the current app when it remains visible", () => {
    const { setActiveApp } = renderGuards({
      activeApp: "codex",
      visibleApps: {
        ...allVisible,
        claude: false,
      },
    });

    expect(setActiveApp).not.toHaveBeenCalled();
  });

  it("switches to the first visible app when the current app is hidden", () => {
    const { setActiveApp } = renderGuards({
      activeApp: "claude",
      visibleApps: {
        claude: false,
        codex: false,
        gemini: true,
        opencode: true,
        openclaw: true,
      },
    });

    expect(setActiveApp).toHaveBeenCalledWith("gemini");
  });

  it("falls back to claude when no app is visible", () => {
    expect(
      getFirstVisibleApp({
        claude: false,
        codex: false,
        gemini: false,
        opencode: false,
        openclaw: false,
      }),
    ).toBe("claude");
  });

  it("keeps session view for supported apps", () => {
    expect(hasSessionSupportForApp("gemini")).toBe(true);

    const { setCurrentView } = renderGuards({
      activeApp: "gemini",
      currentView: "sessions",
    });

    expect(setCurrentView).not.toHaveBeenCalled();
  });

  it("returns unsupported future apps from sessions to Services", () => {
    const unsupportedApp = "future" as AppId;
    expect(hasSessionSupportForApp(unsupportedApp)).toBe(false);

    const { setCurrentView } = renderGuards({
      activeApp: unsupportedApp,
      currentView: "sessions",
    });

    expect(setCurrentView).toHaveBeenCalledWith("services");
  });
});
