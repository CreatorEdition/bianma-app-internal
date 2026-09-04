import { useEffect } from "react";
import type { AppId } from "@/lib/api";
import type { VisibleApps } from "@/types";
import type { AppKeyboardShortcutView } from "@/hooks/useAppKeyboardShortcuts";

export type AppViewGuardView = AppKeyboardShortcutView;

export const hasSessionSupportForApp = (appId: AppId): boolean =>
  appId === "claude" ||
  appId === "codex" ||
  appId === "opencode" ||
  appId === "openclaw" ||
  appId === "gemini";

export const getFirstVisibleApp = (visibleApps: VisibleApps): AppId => {
  if (visibleApps.claude) return "claude";
  if (visibleApps.codex) return "codex";
  if (visibleApps.gemini) return "gemini";
  if (visibleApps.opencode) return "opencode";
  if (visibleApps.openclaw) return "openclaw";
  return "claude";
};

interface UseAppViewGuardsOptions {
  activeApp: AppId;
  currentView: AppViewGuardView;
  visibleApps: VisibleApps;
  setActiveApp: (app: AppId) => void;
  setCurrentView: (view: AppKeyboardShortcutView) => void;
}

/**
 * 保护 App 顶层视图与可见应用状态。
 *
 * 这里只抽取公开仓既有兜底逻辑，不承载 product 新导航模型。
 */
export function useAppViewGuards({
  activeApp,
  currentView,
  visibleApps,
  setActiveApp,
  setCurrentView,
}: UseAppViewGuardsOptions) {
  useEffect(() => {
    if (!visibleApps[activeApp]) {
      setActiveApp(getFirstVisibleApp(visibleApps));
    }
  }, [activeApp, setActiveApp, visibleApps]);

  useEffect(() => {
    if (currentView === "sessions" && !hasSessionSupportForApp(activeApp)) {
      setCurrentView("services");
    }
  }, [activeApp, currentView, setCurrentView]);
}
