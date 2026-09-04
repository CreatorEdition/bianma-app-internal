import { useTranslation } from "react-i18next";
import { Edit2, Trash2, RefreshCw, Globe } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProviderIcon } from "@/components/ProviderIcon";
import type { UniversalProvider } from "@/types";

export interface UniversalProviderSyncStatus {
  status: "success" | "error";
  lastSyncedAt: number;
  errorMessage?: string;
}

interface UniversalProviderCardProps {
  provider: UniversalProvider;
  onEdit: (provider: UniversalProvider) => void;
  onDelete: (id: string) => void;
  onSync: (id: string) => void;
  selected?: boolean;
  onSelectChange?: (id: string, selected: boolean) => void;
  syncStatus?: UniversalProviderSyncStatus;
  selectionDisabled?: boolean;
}

export function UniversalProviderCard({
  provider,
  onEdit,
  onDelete,
  onSync,
  selected = false,
  onSelectChange,
  syncStatus,
  selectionDisabled = false,
}: UniversalProviderCardProps) {
  const { t } = useTranslation();

  // 获取启用的应用列表
  const enabledApps: string[] = [
    provider.apps.claude ? "Claude" : null,
    provider.apps.codex ? "Codex" : null,
    provider.apps.gemini ? "Gemini" : null,
  ].filter((app): app is string => app !== null);

  return (
    <div className="group relative rounded-xl border border-border/50 bg-card p-4 transition-all hover:border-border hover:shadow-md">
      {onSelectChange ? (
        <div className="absolute right-3 top-3 z-10">
          <input
            type="checkbox"
            checked={selected}
            disabled={selectionDisabled}
            data-testid={`select-provider-${provider.id}`}
            aria-label={t("common.select", { defaultValue: "选择" })}
            onChange={(event) =>
              onSelectChange(provider.id, event.target.checked)
            }
            className="h-4 w-4 cursor-pointer rounded border-border text-primary focus:ring-primary"
          />
        </div>
      ) : null}

      {/* 头部：图标和名称 */}
      <div className="flex items-start justify-between">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent">
            <ProviderIcon icon={provider.icon} name={provider.name} size={24} />
          </div>
          <div>
            <h3 className="font-semibold text-foreground">{provider.name}</h3>
            <p className="text-xs text-muted-foreground">
              {provider.providerType}
            </p>
          </div>
        </div>

        {/* 操作按钮 */}
        <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => onSync(provider.id)}
            data-testid={`sync-provider-${provider.id}`}
            title={t("universalProvider.sync", { defaultValue: "同步到应用" })}
          >
            <RefreshCw className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => onEdit(provider)}
            title={t("common.edit", { defaultValue: "编辑" })}
          >
            <Edit2 className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-destructive hover:text-destructive"
            onClick={() => onDelete(provider.id)}
            data-testid={`delete-provider-${provider.id}`}
            title={t("common.delete", { defaultValue: "删除" })}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {/* 配置信息 */}
      <div className="mt-4 space-y-2">
        {/* Base URL */}
        <div className="flex items-center gap-2 text-sm">
          <Globe className="h-3.5 w-3.5 text-muted-foreground" />
          <span className="truncate text-muted-foreground">
            {provider.baseUrl || "-"}
          </span>
        </div>

        <div
          className="rounded-md border border-border/60 bg-muted/20 px-2.5 py-2"
          data-testid={`sync-status-${provider.id}`}
        >
          {syncStatus ? (
            <>
              <p
                className={`text-xs font-medium ${
                  syncStatus.status === "success"
                    ? "text-emerald-500"
                    : "text-destructive"
                }`}
              >
                {syncStatus.status === "success"
                  ? t("universalProvider.syncStatusSuccess", {
                      defaultValue: "最近同步成功",
                    })
                  : t("universalProvider.syncStatusError", {
                      defaultValue: "最近同步失败",
                    })}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {new Date(syncStatus.lastSyncedAt).toLocaleString()}
              </p>
              {syncStatus.status === "error" && syncStatus.errorMessage ? (
                <p className="mt-1 line-clamp-2 text-xs text-destructive/90">
                  {syncStatus.errorMessage}
                </p>
              ) : null}
            </>
          ) : (
            <p className="text-xs text-muted-foreground">
              {t("universalProvider.syncStatusEmpty", {
                defaultValue: "暂无同步记录",
              })}
            </p>
          )}
        </div>

        {/* 启用的应用 */}
        <div className="flex flex-wrap gap-1.5">
          {enabledApps.map((app) => (
            <span
              key={app}
              className="inline-flex items-center rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary"
            >
              {app}
            </span>
          ))}
          {enabledApps.length === 0 && (
            <span className="text-xs text-muted-foreground">
              {t("universalProvider.noAppsEnabled", {
                defaultValue: "未启用任何应用",
              })}
            </span>
          )}
        </div>
      </div>

      {/* 备注 */}
      {provider.notes && (
        <p className="mt-3 text-xs text-muted-foreground line-clamp-2">
          {provider.notes}
        </p>
      )}
    </div>
  );
}
