import { useState, useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Layers, Plus, ServerCog, Settings2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  UniversalProviderCard,
  type UniversalProviderSyncStatus,
} from "./UniversalProviderCard";
import { UniversalProviderFormModal } from "./UniversalProviderFormModal";
import { universalProvidersApi } from "@/lib/api";
import type { UniversalProvider, UniversalProvidersMap } from "@/types";

const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "Unknown error";
};

interface UniversalProviderPanelProps {
  showAddButton?: boolean;
  onOpenAdvanced?: () => void;
  simpleMode?: boolean;
}

export function UniversalProviderPanel({
  showAddButton = true,
  onOpenAdvanced,
  simpleMode = false,
}: UniversalProviderPanelProps = {}) {
  const { t } = useTranslation();

  // 状态
  const [providers, setProviders] = useState<UniversalProvidersMap>({});
  const [loading, setLoading] = useState(true);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingProvider, setEditingProvider] =
    useState<UniversalProvider | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{
    open: boolean;
    id: string;
    name: string;
  }>({ open: false, id: "", name: "" });
  const [syncConfirm, setSyncConfirm] = useState<{
    open: boolean;
    id: string;
    name: string;
  }>({ open: false, id: "", name: "" });
  const [selectedProviderIds, setSelectedProviderIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [isBatchSyncing, setIsBatchSyncing] = useState(false);
  const [syncStatusById, setSyncStatusById] = useState<
    Record<string, UniversalProviderSyncStatus>
  >({});

  // 加载数据
  const loadProviders = useCallback(async () => {
    try {
      setLoading(true);
      const data = await universalProvidersApi.getAll();
      setProviders(data);
    } catch (error) {
      console.error("Failed to load universal providers:", error);
      toast.error(
        t("universalProvider.loadError", {
          defaultValue: "加载统一供应商失败",
        }),
      );
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadProviders();
  }, [loadProviders]);

  useEffect(() => {
    const activeIds = new Set(Object.keys(providers));
    setSelectedProviderIds((current) => {
      const next = new Set<string>();
      let changed = false;
      current.forEach((id) => {
        if (activeIds.has(id)) {
          next.add(id);
        } else {
          changed = true;
        }
      });
      return changed ? next : current;
    });
    setSyncStatusById((current) => {
      const next: Record<string, UniversalProviderSyncStatus> = {};
      let changed = false;
      Object.entries(current).forEach(([id, status]) => {
        if (activeIds.has(id)) {
          next[id] = status;
        } else {
          changed = true;
        }
      });
      return changed ? next : current;
    });
  }, [providers]);

  const updateSyncStatus = useCallback(
    (id: string, status: "success" | "error", errorMessage?: string) => {
      setSyncStatusById((current) => ({
        ...current,
        [id]: {
          status,
          lastSyncedAt: Date.now(),
          errorMessage,
        },
      }));
    },
    [],
  );

  const syncProviderById = useCallback(
    async (id: string): Promise<boolean> => {
      try {
        await universalProvidersApi.sync(id);
        updateSyncStatus(id, "success");
        return true;
      } catch (error) {
        console.error("Failed to sync universal provider:", error);
        updateSyncStatus(id, "error", getErrorMessage(error));
        return false;
      }
    },
    [updateSyncStatus],
  );

  // 添加/编辑供应商
  const handleSave = useCallback(
    async (provider: UniversalProvider) => {
      try {
        await universalProvidersApi.upsert(provider);

        // 新建模式下自动同步到各应用
        if (!editingProvider) {
          const syncOk = await syncProviderById(provider.id);
          if (!syncOk) {
            throw new Error("Sync failed");
          }
        }

        toast.success(
          editingProvider
            ? t("universalProvider.updated", {
                defaultValue: "统一供应商已更新",
              })
            : t("universalProvider.addedAndSynced", {
                defaultValue: "统一供应商已添加并同步",
              }),
        );
        loadProviders();
        setEditingProvider(null);
      } catch (error) {
        console.error("Failed to save universal provider:", error);
        if (!editingProvider) {
          updateSyncStatus(provider.id, "error", getErrorMessage(error));
        }
        toast.error(
          t("universalProvider.saveError", {
            defaultValue: "保存统一供应商失败",
          }),
        );
      }
    },
    [editingProvider, loadProviders, syncProviderById, t, updateSyncStatus],
  );

  // 保存并同步供应商
  const handleSaveAndSync = useCallback(
    async (provider: UniversalProvider) => {
      try {
        await universalProvidersApi.upsert(provider);
        const syncOk = await syncProviderById(provider.id);
        if (!syncOk) {
          throw new Error("Sync failed");
        }
        toast.success(
          t("universalProvider.savedAndSynced", {
            defaultValue: "已保存并同步到所有应用",
          }),
        );
        loadProviders();
        setEditingProvider(null);
      } catch (error) {
        console.error("Failed to save and sync universal provider:", error);
        toast.error(
          t("universalProvider.saveAndSyncError", {
            defaultValue: "保存并同步失败",
          }),
        );
      }
    },
    [loadProviders, syncProviderById, t],
  );

  // 删除供应商
  const handleDelete = useCallback(async () => {
    if (!deleteConfirm.id) return;

    try {
      await universalProvidersApi.delete(deleteConfirm.id);
      toast.success(
        t("universalProvider.deleted", { defaultValue: "统一供应商已删除" }),
      );
      loadProviders();
    } catch (error) {
      console.error("Failed to delete universal provider:", error);
      toast.error(
        t("universalProvider.deleteError", {
          defaultValue: "删除统一供应商失败",
        }),
      );
    } finally {
      setDeleteConfirm({ open: false, id: "", name: "" });
    }
  }, [deleteConfirm.id, loadProviders, t]);

  // 同步供应商
  const handleSync = useCallback(async () => {
    if (!syncConfirm.id) return;

    const syncOk = await syncProviderById(syncConfirm.id);
    if (syncOk) {
      toast.success(
        t("universalProvider.synced", { defaultValue: "已同步到所有应用" }),
      );
    } else {
      toast.error(
        t("universalProvider.syncError", {
          defaultValue: "同步统一供应商失败",
        }),
      );
    }

    setSyncConfirm({ open: false, id: "", name: "" });
  }, [syncConfirm.id, syncProviderById, t]);

  const handleProviderSelectionChange = useCallback(
    (id: string, selected: boolean) => {
      setSelectedProviderIds((current) => {
        const next = new Set(current);
        if (selected) {
          next.add(id);
        } else {
          next.delete(id);
        }
        return next;
      });
    },
    [],
  );

  const providerIds = Object.keys(providers);
  const selectedCount = selectedProviderIds.size;
  const allSelected =
    providerIds.length > 0 && selectedCount === providerIds.length;

  const handleToggleSelectAll = useCallback(() => {
    setSelectedProviderIds((current) => {
      if (current.size === providerIds.length) {
        return new Set();
      }
      return new Set(providerIds);
    });
  }, [providerIds]);

  const handleBatchSync = useCallback(async () => {
    const ids = Array.from(selectedProviderIds);
    if (ids.length === 0) {
      toast.error(
        t("universalProvider.batchSyncSelectRequired", {
          defaultValue: "请先选择至少一个统一供应商",
        }),
      );
      return;
    }

    setIsBatchSyncing(true);
    try {
      const results = await Promise.all(ids.map((id) => syncProviderById(id)));
      const successCount = results.filter(Boolean).length;
      const failedCount = ids.length - successCount;

      if (failedCount === 0) {
        toast.success(
          t("universalProvider.batchSyncSuccess", {
            defaultValue: `批量同步完成（${successCount}/${ids.length}）`,
            successCount,
            total: ids.length,
          }),
        );
      } else {
        toast.error(
          t("universalProvider.batchSyncPartial", {
            defaultValue: `批量同步完成，成功 ${successCount}，失败 ${failedCount}`,
            successCount,
            failedCount,
          }),
        );
      }
    } finally {
      setIsBatchSyncing(false);
    }
  }, [selectedProviderIds, syncProviderById, t]);

  // 打开同步确认
  const handleSyncClick = useCallback(
    (id: string) => {
      const provider = providers[id];
      setSyncConfirm({
        open: true,
        id,
        name: provider?.name || id,
      });
    },
    [providers],
  );

  // 打开编辑
  const handleEdit = useCallback((provider: UniversalProvider) => {
    setEditingProvider(provider);
    setIsFormOpen(true);
  }, []);

  // 打开删除确认
  const handleDeleteClick = useCallback(
    (id: string) => {
      const provider = providers[id];
      setDeleteConfirm({
        open: true,
        id,
        name: provider?.name || id,
      });
    },
    [providers],
  );

  const providerList = Object.values(providers);

  return (
    <div className="space-y-5" data-testid="universal-provider-panel">
      <div className="flex flex-wrap items-start justify-between gap-4 border-b border-border pb-6">
        <div>
          <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
            Upstreams
          </p>
          <div className="mt-2 flex items-center gap-2">
            <ServerCog className="h-5 w-5 text-primary" />
            <h1 className="text-2xl font-semibold">上游渠道</h1>
            <span className="text-sm text-muted-foreground">
              {providerList.length}
            </span>
          </div>
          <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
            默认只配置一次 API 地址、Key
            和模型。当前支持的已接入客户端继承同一套路由。
          </p>
        </div>
        <div className="flex items-center gap-2">
          {onOpenAdvanced ? (
            <Button
              size="sm"
              variant="ghost"
              onClick={onOpenAdvanced}
              className="gap-2"
            >
              <Settings2 className="h-4 w-4" />
              高级配置
            </Button>
          ) : null}
          {showAddButton ? (
            <Button
              size="sm"
              onClick={() => {
                setEditingProvider(null);
                setIsFormOpen(true);
              }}
              className="gap-2 rounded-md"
            >
              <Plus className="h-4 w-4" />
              添加上游
            </Button>
          ) : null}
        </div>
      </div>

      {!simpleMode && providerList.length > 0 ? (
        <div
          className="rounded-xl border border-border/60 bg-muted/20 p-3"
          data-testid="batch-actions-bar"
        >
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleToggleSelectAll}
              disabled={isBatchSyncing}
              data-testid="toggle-select-all"
            >
              {allSelected
                ? t("universalProvider.clearSelection", {
                    defaultValue: "清空选择",
                  })
                : t("universalProvider.selectAll", {
                    defaultValue: "全选",
                  })}
            </Button>
            <Button
              size="sm"
              onClick={handleBatchSync}
              disabled={isBatchSyncing || selectedCount === 0}
              data-testid="batch-sync-button"
            >
              {isBatchSyncing
                ? t("universalProvider.batchSyncing", {
                    defaultValue: "批量同步中...",
                  })
                : t("universalProvider.batchSync", {
                    defaultValue: "批量同步",
                  })}
            </Button>
            <span
              className="text-xs text-muted-foreground"
              data-testid="selected-count"
            >
              {t("universalProvider.selectedCount", {
                defaultValue: `已选择 ${selectedCount} 项`,
                count: selectedCount,
              })}
            </span>
          </div>
        </div>
      ) : null}

      {/* 供应商列表 */}
      {loading ? (
        <div className="flex items-center justify-center py-12">
          <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
        </div>
      ) : providerList.length === 0 ? (
        <div className="border-y border-border py-12 text-left">
          <Layers className="mb-3 h-8 w-8 text-muted-foreground/50" />
          <p className="text-sm text-muted-foreground">还没有上游渠道</p>
          <p className="mt-1 text-xs text-muted-foreground/70">
            填写一个 API 地址、Key 和默认模型，再到路由页一键接入。
          </p>
        </div>
      ) : (
        <div className="divide-y divide-border border-y border-border">
          {providerList.map((provider) => (
            <UniversalProviderCard
              key={provider.id}
              provider={provider}
              onEdit={handleEdit}
              onDelete={handleDeleteClick}
              onSync={handleSyncClick}
              selected={
                simpleMode ? undefined : selectedProviderIds.has(provider.id)
              }
              onSelectChange={
                simpleMode ? undefined : handleProviderSelectionChange
              }
              syncStatus={simpleMode ? undefined : syncStatusById[provider.id]}
              selectionDisabled={isBatchSyncing}
              simpleMode={simpleMode}
            />
          ))}
        </div>
      )}

      {/* 表单模态框 */}
      <UniversalProviderFormModal
        isOpen={isFormOpen}
        onClose={() => {
          setIsFormOpen(false);
          setEditingProvider(null);
        }}
        onSave={handleSave}
        onSaveAndSync={handleSaveAndSync}
        editingProvider={editingProvider}
      />

      {/* 删除确认对话框 */}
      <ConfirmDialog
        isOpen={deleteConfirm.open}
        title={t("universalProvider.deleteConfirmTitle", {
          defaultValue: "删除统一供应商",
        })}
        message={t("universalProvider.deleteConfirmDescription", {
          defaultValue: `确定要删除 "${deleteConfirm.name}" 吗？这将同时删除它在各应用中生成的供应商配置。`,
          name: deleteConfirm.name,
        })}
        confirmText={t("common.delete", { defaultValue: "删除" })}
        onConfirm={handleDelete}
        onCancel={() => setDeleteConfirm({ open: false, id: "", name: "" })}
      />

      {/* 同步确认对话框 */}
      <ConfirmDialog
        isOpen={syncConfirm.open}
        title={t("universalProvider.syncConfirmTitle", {
          defaultValue: "同步统一供应商",
        })}
        message={t("universalProvider.syncConfirmDescription", {
          defaultValue: `同步 "${syncConfirm.name}" 将会覆盖 Claude、Codex 和 Gemini 中关联的供应商配置。确定要继续吗？`,
          name: syncConfirm.name,
        })}
        confirmText={t("universalProvider.syncConfirm", {
          defaultValue: "同步",
        })}
        onConfirm={handleSync}
        onCancel={() => setSyncConfirm({ open: false, id: "", name: "" })}
      />
    </div>
  );
}
