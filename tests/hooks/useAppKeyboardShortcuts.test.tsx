import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  useAppKeyboardShortcuts,
  type AppKeyboardShortcutView,
} from "@/hooks/useAppKeyboardShortcuts";

function dispatchKeyDown(
  target: Window | HTMLElement,
  init: KeyboardEventInit,
) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  target.dispatchEvent(event);
  return event;
}

afterEach(() => {
  document.body.style.overflow = "";
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("useAppKeyboardShortcuts", () => {
  it("opens settings for Ctrl/Cmd comma and prevents the browser shortcut", () => {
    const setCurrentView = vi.fn();
    renderHook(() =>
      useAppKeyboardShortcuts({
        currentView: "home",
        setCurrentView,
      }),
    );

    const ctrlEvent = dispatchKeyDown(window, { key: ",", ctrlKey: true });
    const metaEvent = dispatchKeyDown(window, { key: ",", metaKey: true });

    expect(ctrlEvent.defaultPrevented).toBe(true);
    expect(metaEvent.defaultPrevented).toBe(true);
    expect(setCurrentView).toHaveBeenNthCalledWith(1, "settings");
    expect(setCurrentView).toHaveBeenNthCalledWith(2, "settings");
  });

  it("ignores Escape when already handled or modal body lock is active", () => {
    const setCurrentView = vi.fn();
    renderHook(() =>
      useAppKeyboardShortcuts({
        currentView: "settings",
        setCurrentView,
      }),
    );
    const handledEvent = new KeyboardEvent("keydown", {
      key: "Escape",
      bubbles: true,
      cancelable: true,
    });
    handledEvent.preventDefault();

    window.dispatchEvent(handledEvent);
    document.body.style.overflow = "hidden";
    dispatchKeyDown(window, { key: "Escape" });

    expect(setCurrentView).not.toHaveBeenCalled();
  });

  it("ignores Escape on primary views and editable text targets", () => {
    const setCurrentView = vi.fn();
    const { rerender } = renderHook(
      ({ currentView }: { currentView: AppKeyboardShortcutView }) =>
        useAppKeyboardShortcuts({
          currentView,
          setCurrentView,
        }),
      {
        initialProps: { currentView: "home" },
      },
    );

    dispatchKeyDown(window, { key: "Escape" });
    rerender({ currentView: "settings" });

    const input = document.createElement("input");
    document.body.append(input);
    const inputEvent = dispatchKeyDown(input, { key: "Escape" });

    expect(inputEvent.defaultPrevented).toBe(false);
    expect(setCurrentView).not.toHaveBeenCalled();
  });

  it("returns from skillsDiscovery to skills, Settings to Home, and other views to Services", () => {
    const setCurrentView = vi.fn();
    const { rerender } = renderHook(
      ({ currentView }: { currentView: AppKeyboardShortcutView }) =>
        useAppKeyboardShortcuts({
          currentView,
          setCurrentView,
        }),
      {
        initialProps: { currentView: "skillsDiscovery" },
      },
    );

    const skillsEvent = dispatchKeyDown(window, { key: "Escape" });
    rerender({ currentView: "settings" });
    const homeEvent = dispatchKeyDown(window, { key: "Escape" });
    rerender({ currentView: "prompts" });
    const servicesEvent = dispatchKeyDown(window, { key: "Escape" });

    expect(skillsEvent.defaultPrevented).toBe(true);
    expect(homeEvent.defaultPrevented).toBe(true);
    expect(servicesEvent.defaultPrevented).toBe(true);
    expect(setCurrentView).toHaveBeenNthCalledWith(1, "skills");
    expect(setCurrentView).toHaveBeenNthCalledWith(2, "home");
    expect(setCurrentView).toHaveBeenNthCalledWith(3, "services");
  });

  it("keeps one keydown listener while currentView changes through the ref", () => {
    const addEventListenerSpy = vi.spyOn(window, "addEventListener");
    const removeEventListenerSpy = vi.spyOn(window, "removeEventListener");
    const setCurrentView = vi.fn();
    const { rerender, unmount } = renderHook(
      ({ currentView }: { currentView: AppKeyboardShortcutView }) =>
        useAppKeyboardShortcuts({
          currentView,
          setCurrentView,
        }),
      {
        initialProps: { currentView: "settings" },
      },
    );

    rerender({ currentView: "skillsDiscovery" });
    dispatchKeyDown(window, { key: "Escape" });
    unmount();

    const addKeydownCalls = addEventListenerSpy.mock.calls.filter(
      ([type]) => type === "keydown",
    );
    const removeKeydownCalls = removeEventListenerSpy.mock.calls.filter(
      ([type]) => type === "keydown",
    );

    expect(addKeydownCalls).toHaveLength(1);
    expect(removeKeydownCalls).toHaveLength(1);
    expect(setCurrentView).toHaveBeenCalledWith("skills");
  });
});
