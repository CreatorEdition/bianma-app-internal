import { useCallback, useState } from "react";
import type { Provider } from "@/types";
import { useLastValidValue } from "@/hooks/useLastValidValue";

export type SettingsTab =
  | "general"
  | "proxy"
  | "usage"
  | "about"
  | "advanced"
  | "auth";

export type AppConfirmAction = {
  provider: Provider;
  action: "remove" | "delete";
};

interface UseAppUiStateOptions {
  setCurrentView: (view: "settings" | "home" | "stats") => void;
}

/**
 * 管理 App 顶层已有的纯 UI 状态。
 *
 * 该 hook 只负责弹窗、设置页默认 tab 与确认动作，不承载 Provider 业务逻辑。
 */
export function useAppUiState({ setCurrentView }: UseAppUiStateOptions) {
  const [settingsDefaultTab, setSettingsDefaultTab] =
    useState<SettingsTab>("general");
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);
  const [usageProvider, setUsageProvider] = useState<Provider | null>(null);
  const [confirmAction, setConfirmAction] = useState<AppConfirmAction | null>(
    null,
  );

  const effectiveEditingProvider = useLastValidValue(editingProvider);
  const effectiveUsageProvider = useLastValidValue(usageProvider);

  const openSettingsTab = useCallback(
    (tab: SettingsTab, view: "settings" | "home" | "stats" = "settings") => {
      setSettingsDefaultTab(tab);
      setCurrentView(view);
    },
    [setCurrentView],
  );

  const openGeneralSettings = useCallback(() => {
    openSettingsTab("general");
  }, [openSettingsTab]);

  const openProxySettings = useCallback(() => {
    openSettingsTab("proxy", "home");
  }, [openSettingsTab]);

  const openUsageSettings = useCallback(() => {
    openSettingsTab("usage", "stats");
  }, [openSettingsTab]);

  const openAboutSettings = useCallback(() => {
    openSettingsTab("about");
  }, [openSettingsTab]);

  const openAddDialog = useCallback(() => {
    setIsAddOpen(true);
  }, []);

  const handleAddDialogOpenChange = useCallback((open: boolean) => {
    setIsAddOpen(open);
  }, []);

  const openEditDialog = useCallback((provider: Provider) => {
    setEditingProvider(provider);
  }, []);

  const handleEditDialogOpenChange = useCallback((open: boolean) => {
    if (!open) {
      setEditingProvider(null);
    }
  }, []);

  const openUsageModal = useCallback((provider: Provider) => {
    setUsageProvider(provider);
  }, []);

  const closeUsageModal = useCallback(() => {
    setUsageProvider(null);
  }, []);

  const openDeleteConfirm = useCallback((provider: Provider) => {
    setConfirmAction({ provider, action: "delete" });
  }, []);

  const openRemoveConfirm = useCallback((provider: Provider) => {
    setConfirmAction({ provider, action: "remove" });
  }, []);

  const clearConfirmAction = useCallback(() => {
    setConfirmAction(null);
  }, []);

  return {
    settingsDefaultTab,
    isAddOpen,
    editingProvider,
    usageProvider,
    confirmAction,
    effectiveEditingProvider,
    effectiveUsageProvider,
    openGeneralSettings,
    openProxySettings,
    openUsageSettings,
    openAboutSettings,
    openAddDialog,
    handleAddDialogOpenChange,
    openEditDialog,
    handleEditDialogOpenChange,
    openUsageModal,
    closeUsageModal,
    openDeleteConfirm,
    openRemoveConfirm,
    clearConfirmAction,
  };
}
