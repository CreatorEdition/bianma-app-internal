import { useCallback } from "react";
import type { QueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { toast } from "sonner";
import { providersApi, settingsApi, type AppId } from "@/lib/api";
import { openclawKeys } from "@/hooks/useOpenClaw";
import type { Provider } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";

export interface ProviderWorkspaceConfirmAction {
  provider: Provider;
  action: "remove" | "delete";
}

interface UseProviderWorkspaceActionsOptions {
  activeApp: AppId;
  providers: Record<string, Provider>;
  addProvider: (
    provider: Omit<Provider, "id" | "createdAt"> & {
      providerKey?: string;
      addToLive?: boolean;
    },
  ) => Promise<void>;
  deleteProvider: (id: string) => Promise<void>;
  refetchProviders: () => Promise<unknown>;
  queryClient: QueryClient;
  t: TFunction;
  confirmAction: ProviderWorkspaceConfirmAction | null;
  clearConfirmAction: () => void;
}

const generateUniqueProviderCopyKey = (
  originalKey: string,
  existingKeys: string[],
): string => {
  const baseKey = `${originalKey}-copy`;

  if (!existingKeys.includes(baseKey)) {
    return baseKey;
  }

  let counter = 2;
  while (existingKeys.includes(`${baseKey}-${counter}`)) {
    counter++;
  }
  return `${baseKey}-${counter}`;
};

const cloneJsonValue = <T>(value: T): T => JSON.parse(JSON.stringify(value));

/**
 * 管理 Provider 工作台在 App 层已有的业务动作。
 *
 * 该 hook 只抽取公开仓现有逻辑，不承载 product 私有规则中心、云同步或商业策略能力。
 */
export function useProviderWorkspaceActions({
  activeApp,
  providers,
  addProvider,
  deleteProvider,
  refetchProviders,
  queryClient,
  t,
  confirmAction,
  clearConfirmAction,
}: UseProviderWorkspaceActionsOptions) {
  const handleOpenWebsite = useCallback(
    async (url: string) => {
      try {
        await settingsApi.openExternal(url);
      } catch (error) {
        const detail =
          extractErrorMessage(error) ||
          t("notifications.openLinkFailed", {
            defaultValue: "链接打开失败",
          });
        toast.error(detail);
      }
    },
    [t],
  );

  const handleConfirmAction = useCallback(async () => {
    if (!confirmAction) return;
    const { provider, action } = confirmAction;

    try {
      if (action === "remove") {
        await providersApi.removeFromLiveConfig(provider.id, activeApp);
        if (activeApp === "opencode") {
          await queryClient.invalidateQueries({
            queryKey: ["opencodeLiveProviderIds"],
          });
        } else if (activeApp === "openclaw") {
          await queryClient.invalidateQueries({
            queryKey: openclawKeys.liveProviderIds,
          });
          await queryClient.invalidateQueries({
            queryKey: openclawKeys.health,
          });
        }
        toast.success(
          t("notifications.removeFromConfigSuccess", {
            defaultValue: "已从配置移除",
          }),
          { closeButton: true },
        );
      } else {
        await deleteProvider(provider.id);
      }
    } catch (error) {
      console.error("[App] Failed to confirm provider action", error);
      const detail = extractErrorMessage(error);
      const message = t(
        action === "remove"
          ? "notifications.removeFromConfigFailed"
          : "notifications.deleteFailed",
        {
          defaultValue:
            action === "remove" ? "从配置移除失败" : "删除供应商失败",
          error: detail,
        },
      );
      toast.error(
        detail && !message.includes(detail) ? `${message}: ${detail}` : message,
      );
    } finally {
      clearConfirmAction();
    }
  }, [
    activeApp,
    clearConfirmAction,
    confirmAction,
    deleteProvider,
    queryClient,
    t,
  ]);

  const handleDuplicateProvider = useCallback(
    async (provider: Provider) => {
      const newSortIndex =
        provider.sortIndex !== undefined ? provider.sortIndex + 1 : undefined;

      const duplicatedProvider: Omit<Provider, "id" | "createdAt"> & {
        providerKey?: string;
        addToLive?: boolean;
      } = {
        name: `${provider.name} copy`,
        settingsConfig: cloneJsonValue(provider.settingsConfig),
        websiteUrl: provider.websiteUrl,
        category: provider.category,
        sortIndex: newSortIndex,
        meta: provider.meta ? cloneJsonValue(provider.meta) : undefined,
        icon: provider.icon,
        iconColor: provider.iconColor,
      };

      if (activeApp === "opencode" || activeApp === "openclaw") {
        let liveProviderIds: string[] = [];
        try {
          liveProviderIds =
            activeApp === "opencode"
              ? await queryClient.ensureQueryData({
                  queryKey: ["opencodeLiveProviderIds"],
                  queryFn: () => providersApi.getOpenCodeLiveProviderIds(),
                })
              : await queryClient.ensureQueryData({
                  queryKey: openclawKeys.liveProviderIds,
                  queryFn: () => providersApi.getOpenClawLiveProviderIds(),
                });
        } catch (error) {
          console.error(
            "[App] Failed to load live provider IDs for duplication",
            error,
          );
          const errorMessage = extractErrorMessage(error);
          toast.error(
            t("provider.duplicateLiveIdsLoadFailed", {
              defaultValue: "读取配置中的供应商标识失败，请先修复配置后再试",
            }) + (errorMessage ? `: ${errorMessage}` : ""),
          );
          return;
        }
        const existingKeys = Array.from(
          new Set([...Object.keys(providers), ...liveProviderIds]),
        );
        duplicatedProvider.providerKey = generateUniqueProviderCopyKey(
          provider.id,
          existingKeys,
        );
        duplicatedProvider.addToLive = false;
      }

      if (provider.sortIndex !== undefined) {
        const updates = Object.values(providers)
          .filter(
            (p) =>
              p.sortIndex !== undefined &&
              p.sortIndex >= newSortIndex! &&
              p.id !== provider.id,
          )
          .map((p) => ({
            id: p.id,
            sortIndex: p.sortIndex! + 1,
          }));

        if (updates.length > 0) {
          try {
            await providersApi.updateSortOrder(updates, activeApp);
          } catch (error) {
            console.error("[App] Failed to update sort order", error);
            toast.error(
              t("provider.sortUpdateFailed", {
                defaultValue: "排序更新失败",
              }),
            );
            return;
          }
        }
      }

      await addProvider(duplicatedProvider);
    },
    [activeApp, addProvider, providers, queryClient, t],
  );

  const handleOpenTerminal = useCallback(
    async (provider: Provider) => {
      try {
        const selectedDir = await settingsApi.pickDirectory();
        if (!selectedDir) {
          return;
        }

        await providersApi.openTerminal(provider.id, activeApp, {
          cwd: selectedDir,
        });
        toast.success(
          t("provider.terminalOpened", {
            defaultValue: "终端已打开",
          }),
        );
      } catch (error) {
        console.error("[App] Failed to open terminal", error);
        const errorMessage = extractErrorMessage(error);
        toast.error(
          t("provider.terminalOpenFailed", {
            defaultValue: "打开终端失败",
          }) + (errorMessage ? `: ${errorMessage}` : ""),
        );
      }
    },
    [activeApp, t],
  );

  const handleImportSuccess = useCallback(async () => {
    try {
      await queryClient.invalidateQueries({
        queryKey: ["providers"],
        refetchType: "all",
      });
      await queryClient.refetchQueries({
        queryKey: ["providers"],
        type: "all",
      });
    } catch (error) {
      console.error("[App] Failed to refresh providers after import", error);
      await refetchProviders();
    }
    try {
      await providersApi.updateTrayMenu();
    } catch (error) {
      console.error("[App] Failed to refresh tray menu", error);
    }
  }, [queryClient, refetchProviders]);

  return {
    handleOpenWebsite,
    handleConfirmAction,
    handleDuplicateProvider,
    handleOpenTerminal,
    handleImportSuccess,
  };
}
