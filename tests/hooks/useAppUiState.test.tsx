import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Provider } from "@/types";
import { useAppUiState } from "@/hooks/useAppUiState";

const provider: Provider = {
  id: "provider-one",
  name: "Provider One",
  settingsConfig: {},
};

const anotherProvider: Provider = {
  id: "provider-two",
  name: "Provider Two",
  settingsConfig: {},
};

afterEach(() => {
  vi.clearAllMocks();
});

describe("useAppUiState", () => {
  it("opens settings tabs and switches current view to settings", () => {
    const setCurrentView = vi.fn();
    const { result } = renderHook(() => useAppUiState({ setCurrentView }));

    act(() => {
      result.current.openGeneralSettings();
    });
    expect(result.current.settingsDefaultTab).toBe("general");

    act(() => {
      result.current.openProxySettings();
    });
    expect(result.current.settingsDefaultTab).toBe("proxy");

    act(() => {
      result.current.openUsageSettings();
    });
    expect(result.current.settingsDefaultTab).toBe("usage");

    act(() => {
      result.current.openAboutSettings();
    });
    expect(result.current.settingsDefaultTab).toBe("about");
    expect(setCurrentView).toHaveBeenCalledTimes(4);
    expect(setCurrentView).toHaveBeenCalledWith("settings");
  });

  it("opens add, edit and usage UI state and closes them through handlers", () => {
    const setCurrentView = vi.fn();
    const { result } = renderHook(() => useAppUiState({ setCurrentView }));

    act(() => {
      result.current.openAddDialog();
    });
    expect(result.current.isAddOpen).toBe(true);

    act(() => {
      result.current.handleAddDialogOpenChange(false);
    });
    expect(result.current.isAddOpen).toBe(false);

    act(() => {
      result.current.openEditDialog(provider);
    });
    expect(result.current.editingProvider).toBe(provider);

    act(() => {
      result.current.handleEditDialogOpenChange(false);
    });
    expect(result.current.editingProvider).toBeNull();

    act(() => {
      result.current.openUsageModal(anotherProvider);
    });
    expect(result.current.usageProvider).toBe(anotherProvider);

    act(() => {
      result.current.closeUsageModal();
    });
    expect(result.current.usageProvider).toBeNull();
  });

  it("opens delete and remove confirm actions and clears them", () => {
    const setCurrentView = vi.fn();
    const { result } = renderHook(() => useAppUiState({ setCurrentView }));

    act(() => {
      result.current.openDeleteConfirm(provider);
    });
    expect(result.current.confirmAction).toEqual({
      provider,
      action: "delete",
    });

    act(() => {
      result.current.openRemoveConfirm(anotherProvider);
    });
    expect(result.current.confirmAction).toEqual({
      provider: anotherProvider,
      action: "remove",
    });

    act(() => {
      result.current.clearConfirmAction();
    });
    expect(result.current.confirmAction).toBeNull();
  });

  it("keeps effective providers during close animation after source state is cleared", () => {
    const setCurrentView = vi.fn();
    const { result } = renderHook(() => useAppUiState({ setCurrentView }));

    act(() => {
      result.current.openEditDialog(provider);
      result.current.openUsageModal(anotherProvider);
    });
    expect(result.current.effectiveEditingProvider).toBe(provider);
    expect(result.current.effectiveUsageProvider).toBe(anotherProvider);

    act(() => {
      result.current.handleEditDialogOpenChange(false);
      result.current.closeUsageModal();
    });

    expect(result.current.editingProvider).toBeNull();
    expect(result.current.usageProvider).toBeNull();
    expect(result.current.effectiveEditingProvider).toBe(provider);
    expect(result.current.effectiveUsageProvider).toBe(anotherProvider);
  });
});
