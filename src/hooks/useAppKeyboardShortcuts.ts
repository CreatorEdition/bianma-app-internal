import { useEffect, useRef } from "react";
import { isTextEditableTarget } from "@/utils/domUtils";

export type AppKeyboardShortcutView =
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

type KeyboardShortcutTargetView = "providers" | "settings" | "skills";

interface UseAppKeyboardShortcutsParams {
  currentView: AppKeyboardShortcutView;
  setCurrentView: (view: KeyboardShortcutTargetView) => void;
}

function getEscapeTargetView(view: AppKeyboardShortcutView) {
  return view === "skillsDiscovery" ? "skills" : "providers";
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
      if (view === "providers") return;

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
