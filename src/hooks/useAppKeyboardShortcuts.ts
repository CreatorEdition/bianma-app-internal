import { useEffect, useRef } from "react";
import { isTextEditableTarget } from "@/utils/domUtils";

export type AppKeyboardShortcutView =
  | "home"
  | "services"
  | "strategy"
  | "stats"
  | "providers"
  | "settings"
  | "prompts"
  | "skills"
  | "skillsDiscovery"
  | "mcp"
  | "agents"
  | "universal"
  | "sessions"
  | "workspace"
  | "openclawEnv"
  | "openclawTools"
  | "openclawAgents";

type KeyboardShortcutTargetView = AppKeyboardShortcutView;

interface UseAppKeyboardShortcutsParams {
  currentView: AppKeyboardShortcutView;
  setCurrentView: (view: KeyboardShortcutTargetView) => void;
}

function getEscapeTargetView(view: AppKeyboardShortcutView) {
  if (view === "skillsDiscovery") return "skills";
  if (
    view === "home" ||
    view === "services" ||
    view === "strategy" ||
    view === "stats"
  ) {
    return view;
  }
  return view === "settings" ? "home" : "services";
}

export function useAppKeyboardShortcuts({
  currentView,
  setCurrentView,
}: UseAppKeyboardShortcutsParams) {
  const currentViewRef = useRef(currentView);

  useEffect(() => {
    currentViewRef.current = currentView;
  }, [currentView]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "," && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setCurrentView("settings");
        return;
      }

      if (event.key !== "Escape" || event.defaultPrevented) return;

      if (document.body.style.overflow === "hidden") return;

      const view = currentViewRef.current;
      if (
        view === "home" ||
        view === "services" ||
        view === "strategy" ||
        view === "stats"
      ) {
        return;
      }

      if (isTextEditableTarget(event.target)) return;

      event.preventDefault();
      setCurrentView(getEscapeTargetView(view));
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [setCurrentView]);
}
