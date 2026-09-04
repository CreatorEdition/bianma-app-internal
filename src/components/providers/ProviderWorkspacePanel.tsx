import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ExternalLink,
  FlaskConical,
  Loader2,
  Network,
  RefreshCcw,
  Search,
  Shield,
  Star,
  Zap,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type {
  DiscoveredModel,
  Provider,
  ProviderAppId,
  ProviderProtocolHint,
} from "@/types";
import type { AppId } from "@/lib/api";
import { providersApi } from "@/lib/api/providers";
import { useDragSort } from "@/hooks/useDragSort";
import { useOpenClawDefaultModel } from "@/hooks/useOpenClaw";
import { useStreamCheck } from "@/hooks/useStreamCheck";
import { useAutoFailoverEnabled } from "@/lib/query/failover";
import { ProviderList } from "@/components/providers/ProviderList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { isTextEditableTarget } from "@/utils/domUtils";
import {
  readCompatibleStorage,
  removeCompatibleStorage,
  writeCompatibleStorage,
} from "@/lib/storageCompat";
import {
  getProviderConnectionDetails,
  inferProviderProtocolHint,
} from "@/utils/providerConfigUtils";

interface ProviderWorkspacePanelProps {
  activeApp: AppId;
  providers: Record<string, Provider>;
  currentProviderId: string;
  isLoading: boolean;
  isProxyRunning: boolean;
  isCurrentAppTakeoverActive: boolean;
  activeProviderId?: string;
  onSwitch: (provider: Provider) => void;
  onEdit: (provider: Provider) => void;
  onDelete: (provider: Provider) => void;
  onRemoveFromConfig?: (provider: Provider) => void;
  onDisableOmo?: () => void;
  onDisableOmoSlim?: () => void;
  onDuplicate: (provider: Provider) => void;
  onConfigureUsage: (provider: Provider) => void;
  onOpenWebsite: (url: string) => void;
  onOpenTerminal?: (provider: Provider) => void;
  onCreate: () => void;
  onSetAsDefault?: (provider: Provider) => void;
  onOpenProxySettings: () => void;
}

type ProviderSortStrategy = "manual" | "activeFirst";
type ProviderScope = "all" | "favorites";
type ModelScope = "all" | "favorites";
type ModelDiscoveryStatus = "idle" | "loading" | "success" | "error";

interface ProviderDiscoveryState {
  status: ModelDiscoveryStatus;
  models: DiscoveredModel[];
  protocolHint?: ProviderProtocolHint;
  error?: string;
}

interface LatencySummary {
  latencyMs: number | null;
  status: number | null;
  error: string | null;
  testedAt: number;
}

const MODEL_WORKSPACE_STORAGE_PREFIX = "bianma-model-workspace";
const LEGACY_MODEL_WORKSPACE_STORAGE_PREFIX = "cc-switch-model-workspace";
const DISCOVERY_SUPPORTED_APPS: ProviderAppId[] = ["claude", "codex", "gemini"];

const isSortStrategy = (value: string | null): value is ProviderSortStrategy =>
  value === "manual" || value === "activeFirst";

const getWorkspaceStorageKey = (appId: AppId, suffix: string) =>
  `${MODEL_WORKSPACE_STORAGE_PREFIX}-${appId}-${suffix}`;

const getLegacyWorkspaceStorageKey = (appId: AppId, suffix: string) =>
  `${LEGACY_MODEL_WORKSPACE_STORAGE_PREFIX}-${appId}-${suffix}`;

function dedupeAndSortModels(models: DiscoveredModel[]): DiscoveredModel[] {
  const byId = new Map<string, DiscoveredModel>();

  models.forEach((model) => {
    if (!model.id) {
      return;
    }
    byId.set(model.id, {
      ...model,
      name: model.name?.trim() || model.id,
    });
  });

  return [...byId.values()].sort((left, right) =>
    left.name.localeCompare(right.name, "zh-CN", { sensitivity: "base" }),
  );
}

function getConfiguredModels(
  provider: Provider,
  appId: ProviderAppId,
): DiscoveredModel[] {
  const config = provider.settingsConfig ?? {};

  if (appId === "opencode") {
    const models = config.models;
    if (!models || typeof models !== "object" || Array.isArray(models)) {
      return [];
    }

    return dedupeAndSortModels(
      Object.entries(models as Record<string, any>).map(([id, value]) => ({
        id,
        name:
          typeof value?.name === "string" && value.name.trim().length > 0
            ? value.name
            : id,
        contextWindow:
          typeof value?.limit?.context === "number"
            ? value.limit.context
            : undefined,
      })),
    );
  }

  if (appId === "openclaw") {
    const models = Array.isArray(config.models) ? config.models : [];
    return dedupeAndSortModels(
      models.map((model: Record<string, any>) => ({
        id: typeof model.id === "string" ? model.id : "",
        name:
          typeof model.name === "string" && model.name.trim().length > 0
            ? model.name
            : typeof model.id === "string"
              ? model.id
              : "",
        contextWindow:
          typeof model.contextWindow === "number"
            ? model.contextWindow
            : undefined,
      })),
    );
  }

  return [];
}

function formatLatency(
  t: (key: string, options?: Record<string, unknown>) => string,
  result?: LatencySummary,
): string {
  if (!result) {
    return t("provider.latencyNotTested", { defaultValue: "未测速" });
  }
  if (result.error) {
    return t("provider.latencyFailed", { defaultValue: "失败" });
  }
  if (typeof result.latencyMs === "number") {
    return `${Math.round(result.latencyMs)} ms`;
  }
  return result.status
    ? `${result.status}`
    : t("provider.latencyNoResponse", { defaultValue: "无响应" });
}

export function ProviderWorkspacePanel({
  activeApp,
  providers,
  currentProviderId,
  isLoading,
  isProxyRunning,
  isCurrentAppTakeoverActive,
  activeProviderId,
  onSwitch,
  onEdit,
  onDelete,
  onRemoveFromConfig,
  onDisableOmo,
  onDisableOmoSlim,
  onDuplicate,
  onConfigureUsage,
  onOpenWebsite,
  onOpenTerminal,
  onCreate,
  onSetAsDefault,
  onOpenProxySettings,
}: ProviderWorkspacePanelProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const discoveryRequestSeqRef = useRef<Record<string, number>>({});
  const discoveryByProviderIdRef = useRef<
    Record<string, ProviderDiscoveryState>
  >({});
  const activeProviderApp = activeApp as ProviderAppId;
  const { sortedProviders } = useDragSort(providers, activeApp);
  const { data: openclawDefaultModel } = useOpenClawDefaultModel(
    activeApp === "openclaw",
  );
  const { data: isAutoFailoverEnabled } = useAutoFailoverEnabled(activeApp);
  const { checkProvider, isChecking } = useStreamCheck(activeApp);

  const storageKeys = useMemo(
    () => ({
      sort: getWorkspaceStorageKey(activeApp, "sort"),
      selectedProvider: getWorkspaceStorageKey(activeApp, "selected-provider"),
    }),
    [activeApp],
  );

  const [searchTerm, setSearchTerm] = useState("");
  const [providerScope, setProviderScope] = useState<ProviderScope>("all");
  const [modelScope, setModelScope] = useState<ModelScope>("all");
  const [sortStrategy, setSortStrategy] =
    useState<ProviderSortStrategy>("manual");
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    null,
  );
  const [discoveryByProviderId, setDiscoveryByProviderId] = useState<
    Record<string, ProviderDiscoveryState>
  >({});
  const [isTestingLatency, setIsTestingLatency] = useState(false);
  const [latencyResults, setLatencyResults] = useState<
    Record<string, LatencySummary>
  >({});

  const setProviderDiscoveryState = useCallback(
    (providerId: string, state: ProviderDiscoveryState) => {
      discoveryByProviderIdRef.current = {
        ...discoveryByProviderIdRef.current,
        [providerId]: state,
      };
      setDiscoveryByProviderId((previous) => ({
        ...previous,
        [providerId]: state,
      }));
    },
    [],
  );

  useEffect(() => {
    const storedSort = readCompatibleStorage(storageKeys.sort, [
      getLegacyWorkspaceStorageKey(activeApp, "sort"),
    ]);
    setSortStrategy(isSortStrategy(storedSort) ? storedSort : "manual");
    setSelectedProviderId(
      readCompatibleStorage(storageKeys.selectedProvider, [
        getLegacyWorkspaceStorageKey(activeApp, "selected-provider"),
      ]),
    );
  }, [activeApp, storageKeys]);

  useEffect(() => {
    writeCompatibleStorage(storageKeys.sort, sortStrategy, [
      getLegacyWorkspaceStorageKey(activeApp, "sort"),
    ]);
  }, [activeApp, sortStrategy, storageKeys.sort]);

  useEffect(() => {
    if (selectedProviderId) {
      writeCompatibleStorage(storageKeys.selectedProvider, selectedProviderId, [
        getLegacyWorkspaceStorageKey(activeApp, "selected-provider"),
      ]);
      return;
    }

    removeCompatibleStorage(storageKeys.selectedProvider, [
      getLegacyWorkspaceStorageKey(activeApp, "selected-provider"),
    ]);
  }, [activeApp, selectedProviderId, storageKeys.selectedProvider]);

  const orderedProviders = useMemo(() => {
    if (sortStrategy === "manual") {
      return sortedProviders;
    }

    const activeIds = new Set(
      [activeProviderId, currentProviderId].filter((id): id is string =>
        Boolean(id),
      ),
    );

    return [...sortedProviders]
      .map((provider, index) => ({ provider, index }))
      .sort((left, right) => {
        const leftActive = activeIds.has(left.provider.id) ? 1 : 0;
        const rightActive = activeIds.has(right.provider.id) ? 1 : 0;
        if (leftActive !== rightActive) {
          return rightActive - leftActive;
        }
        return left.index - right.index;
      })
      .map(({ provider }) => provider);
  }, [activeProviderId, currentProviderId, sortStrategy, sortedProviders]);

  const visibleProviders = useMemo(() => {
    const keyword = searchTerm.trim().toLowerCase();

    return orderedProviders.filter((provider) => {
      if (providerScope === "favorites" && !provider.meta?.favoriteProvider) {
        return false;
      }

      if (!keyword) {
        return true;
      }

      const fields = [
        provider.name,
        provider.id,
        provider.notes,
        provider.websiteUrl,
      ];
      return fields.some((field) =>
        field?.toString().toLowerCase().includes(keyword),
      );
    });
  }, [orderedProviders, providerScope, searchTerm]);

  useEffect(() => {
    if (visibleProviders.length === 0) {
      setSelectedProviderId(null);
      return;
    }

    const visibleIds = new Set(visibleProviders.map((provider) => provider.id));
    setSelectedProviderId((previous) => {
      if (previous && visibleIds.has(previous)) {
        return previous;
      }

      return (
        [activeProviderId, currentProviderId].find((id): id is string => {
          if (!id) {
            return false;
          }
          return visibleIds.has(id);
        }) ?? visibleProviders[0].id
      );
    });
  }, [activeProviderId, currentProviderId, visibleProviders]);

  const selectedProvider = useMemo(() => {
    if (!selectedProviderId) {
      return null;
    }
    return providers[selectedProviderId] ?? null;
  }, [providers, selectedProviderId]);

  const selectedProviderMap = useMemo(() => {
    if (!selectedProvider) {
      return {};
    }
    return { [selectedProvider.id]: selectedProvider };
  }, [selectedProvider]);

  const selectedConnection = useMemo(() => {
    if (!selectedProvider) {
      return null;
    }
    return getProviderConnectionDetails(selectedProvider, activeProviderApp);
  }, [activeProviderApp, selectedProvider]);

  const selectedDiscoveryState = selectedProvider
    ? discoveryByProviderId[selectedProvider.id]
    : undefined;
  const selectedDiscoveryProtocol = useMemo(() => {
    if (!selectedProvider) {
      return undefined;
    }
    return (
      selectedDiscoveryState?.protocolHint ??
      inferProviderProtocolHint(selectedProvider, activeProviderApp)
    );
  }, [
    activeProviderApp,
    selectedDiscoveryState?.protocolHint,
    selectedProvider,
  ]);

  const canDiscoverModels =
    DISCOVERY_SUPPORTED_APPS.includes(activeProviderApp);
  const configuredModels = useMemo(
    () =>
      selectedProvider
        ? getConfiguredModels(selectedProvider, activeProviderApp)
        : [],
    [activeProviderApp, selectedProvider],
  );
  const discoveredModels = useMemo(() => {
    if (!selectedProvider) {
      return [];
    }
    if (canDiscoverModels) {
      return selectedDiscoveryState?.models ?? [];
    }
    return configuredModels;
  }, [
    canDiscoverModels,
    configuredModels,
    selectedDiscoveryState?.models,
    selectedProvider,
  ]);

  const favoriteModelIds = useMemo(() => {
    if (!selectedProvider) {
      return [];
    }
    return (
      selectedProvider.meta?.favoriteModelsByApp?.[activeProviderApp] ?? []
    );
  }, [activeProviderApp, selectedProvider]);

  const visibleModels = useMemo(() => {
    if (modelScope === "favorites") {
      const favorites = new Set(favoriteModelIds);
      return discoveredModels.filter((model) => favorites.has(model.id));
    }
    return discoveredModels;
  }, [discoveredModels, favoriteModelIds, modelScope]);

  useEffect(() => {
    if (favoriteModelIds.length === 0 && modelScope === "favorites") {
      setModelScope("all");
    }
  }, [favoriteModelIds.length, modelScope]);

  const persistProviderUpdate = useCallback(
    async (provider: Provider) => {
      await providersApi.update(provider, activeApp);
      await queryClient.invalidateQueries({
        queryKey: ["providers", activeApp],
      });
    },
    [activeApp, queryClient],
  );

  const updateProviderMeta = useCallback(
    async (
      provider: Provider,
      transform: (
        currentMeta: NonNullable<Provider["meta"]>,
      ) => NonNullable<Provider["meta"]>,
    ) => {
      try {
        await persistProviderUpdate({
          ...provider,
          meta: transform(provider.meta ?? {}),
        });
      } catch (error) {
        console.error("Failed to update provider meta:", error);
        toast.error(
          t("provider.metaUpdateFailed", {
            defaultValue: "服务商偏好保存失败，请稍后重试",
          }),
        );
      }
    },
    [persistProviderUpdate, t],
  );

  const discoverModelsForProvider = useCallback(
    async (
      provider: Provider,
      protocolHint?: ProviderProtocolHint,
      force?: boolean,
    ) => {
      const connection = getProviderConnectionDetails(
        provider,
        activeProviderApp,
      );
      const resolvedProtocol = protocolHint ?? connection.protocolHint;

      if (!connection.baseUrl || !resolvedProtocol) {
        discoveryRequestSeqRef.current[provider.id] =
          (discoveryRequestSeqRef.current[provider.id] ?? 0) + 1;
        setProviderDiscoveryState(provider.id, {
          status: "error",
          models: discoveryByProviderIdRef.current[provider.id]?.models ?? [],
          protocolHint: resolvedProtocol,
          error: t("provider.discoveryMissingConfig", {
            defaultValue: "缺少可发现模型的接口地址或协议提示",
          }),
        });
        return;
      }

      if (!force) {
        const existing = discoveryByProviderIdRef.current[provider.id];
        if (
          existing &&
          existing.protocolHint === resolvedProtocol &&
          (existing.status === "loading" || existing.status === "success")
        ) {
          return;
        }
      }

      const requestSeq = (discoveryRequestSeqRef.current[provider.id] ?? 0) + 1;
      discoveryRequestSeqRef.current[provider.id] = requestSeq;

      setProviderDiscoveryState(provider.id, {
        status: "loading",
        models: discoveryByProviderIdRef.current[provider.id]?.models ?? [],
        protocolHint: resolvedProtocol,
      });

      try {
        const models = await providersApi.discoverModels({
          baseUrl: connection.baseUrl,
          apiKey: connection.apiKey,
          protocolHint: resolvedProtocol,
        });

        if (discoveryRequestSeqRef.current[provider.id] !== requestSeq) {
          return;
        }

        setProviderDiscoveryState(provider.id, {
          status: "success",
          models: dedupeAndSortModels(models),
          protocolHint: resolvedProtocol,
        });
      } catch (error) {
        if (discoveryRequestSeqRef.current[provider.id] !== requestSeq) {
          return;
        }

        const message = error instanceof Error ? error.message : String(error);
        setProviderDiscoveryState(provider.id, {
          status: "error",
          models: discoveryByProviderIdRef.current[provider.id]?.models ?? [],
          protocolHint: resolvedProtocol,
          error: message,
        });
      }
    },
    [activeProviderApp, setProviderDiscoveryState, t],
  );

  useEffect(() => {
    const loadCachedLatency = async () => {
      try {
        const cached = await providersApi.getCachedLatencyResults(activeApp);
        const results: Record<string, LatencySummary> = {};
        cached.forEach((result) => {
          results[result.providerId] = {
            latencyMs: result.latencyMs,
            status: result.status,
            error: result.error,
            testedAt: result.testedAt,
          };
        });
        setLatencyResults(results);
      } catch (error) {
        console.error("Failed to load cached latency results:", error);
      }
    };

    void loadCachedLatency();
  }, [activeApp]);

  useEffect(() => {
    if (!selectedProvider || !canDiscoverModels) {
      return;
    }

    void discoverModelsForProvider(selectedProvider, selectedDiscoveryProtocol);
  }, [
    canDiscoverModels,
    discoverModelsForProvider,
    selectedDiscoveryProtocol,
    selectedProvider,
  ]);

  const handleTestProvidersLatency = useCallback(async () => {
    setIsTestingLatency(true);
    setLatencyResults({});

    try {
      const response = await providersApi.testProvidersLatency(
        activeApp,
        undefined,
        10,
      );
      const results: Record<string, LatencySummary> = {};
      response.results.forEach((result) => {
        results[result.providerId] = {
          latencyMs: result.latencyMs,
          status: result.status,
          error: result.error,
          testedAt: result.testedAt,
        };
      });
      setLatencyResults(results);

      toast.success(
        t("provider.latencyTestSuccess", {
          defaultValue: "测速完成: {{success}} 成功, {{failed}} 失败",
          success: response.success,
          failed: response.failed,
        }),
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(
        `${t("provider.latencyTestFailed", {
          defaultValue: "延迟测试失败",
        })}: ${message}`,
      );
    } finally {
      setIsTestingLatency(false);
    }
  }, [activeApp, t]);

  const handleCardClick = useCallback((providerId: string) => {
    setSelectedProviderId(providerId);
  }, []);

  const handleCardDoubleClick = useCallback(
    (provider: Provider) => {
      setSelectedProviderId(provider.id);
      if (provider.id !== currentProviderId) {
        onSwitch(provider);
      }
    },
    [currentProviderId, onSwitch],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();

      if ((event.metaKey || event.ctrlKey) && key === "f") {
        event.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        return;
      }

      if (isTextEditableTarget(event.target)) {
        return;
      }

      if (visibleProviders.length === 0) {
        return;
      }

      const currentIndex = selectedProviderId
        ? visibleProviders.findIndex(
            (provider) => provider.id === selectedProviderId,
          )
        : -1;

      if (event.key === "ArrowDown" || event.key === "ArrowRight") {
        event.preventDefault();
        const nextIndex =
          currentIndex < 0
            ? 0
            : Math.min(currentIndex + 1, visibleProviders.length - 1);
        setSelectedProviderId(visibleProviders[nextIndex].id);
        return;
      }

      if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
        event.preventDefault();
        const previousIndex =
          currentIndex < 0 ? 0 : Math.max(currentIndex - 1, 0);
        setSelectedProviderId(visibleProviders[previousIndex].id);
        return;
      }

      if (event.key === "Enter" && currentIndex >= 0) {
        event.preventDefault();
        const provider = visibleProviders[currentIndex];
        if (provider.id !== currentProviderId) {
          onSwitch(provider);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [currentProviderId, onSwitch, selectedProviderId, visibleProviders]);

  const handleToggleProviderFavorite = useCallback(
    (provider: Provider) => {
      void updateProviderMeta(provider, (meta) => ({
        ...meta,
        favoriteProvider: !meta.favoriteProvider,
      }));
    },
    [updateProviderMeta],
  );

  const handleToggleModelFavorite = useCallback(
    (modelId: string) => {
      if (!selectedProvider) {
        return;
      }

      void updateProviderMeta(selectedProvider, (meta) => {
        const byApp = meta.favoriteModelsByApp ?? {};
        const currentFavorites = byApp[activeProviderApp] ?? [];
        const nextFavorites = currentFavorites.includes(modelId)
          ? currentFavorites.filter((id) => id !== modelId)
          : [...currentFavorites, modelId];

        return {
          ...meta,
          favoriteModelsByApp: {
            ...byApp,
            [activeProviderApp]: nextFavorites,
          },
        };
      });
    },
    [activeProviderApp, selectedProvider, updateProviderMeta],
  );

  const handleProtocolChange = useCallback(
    (protocolHint: ProviderProtocolHint) => {
      if (!selectedProvider) {
        return;
      }

      void updateProviderMeta(selectedProvider, (meta) => ({
        ...meta,
        modelDiscoveryProtocol: protocolHint,
      }));
      void discoverModelsForProvider(selectedProvider, protocolHint, true);
    },
    [discoverModelsForProvider, selectedProvider, updateProviderMeta],
  );

  const selectedBadges = useMemo(() => {
    if (!selectedProvider) {
      return [];
    }

    const badges = [
      selectedProvider.category,
      selectedConnection?.baseUrl ? "API" : undefined,
      selectedProvider.meta?.usage_script?.enabled
        ? t("provider.badgeUsageScript", { defaultValue: "用量脚本" })
        : undefined,
      selectedProvider.meta?.favoriteProvider
        ? t("provider.badgeFavorited", { defaultValue: "已收藏" })
        : undefined,
      activeApp === "openclaw" && openclawDefaultModel?.primary
        ? openclawDefaultModel.primary
        : undefined,
    ];

    return badges.filter((badge): badge is string => Boolean(badge));
  }, [
    activeApp,
    openclawDefaultModel?.primary,
    selectedConnection,
    selectedProvider,
    t,
  ]);

  const proxySummaryItems = useMemo(
    () => [
      {
        label: t("provider.proxyRunning", { defaultValue: "代理进程" }),
        value: isProxyRunning
          ? t("common.enabled", { defaultValue: "已启用" })
          : t("common.disabled", { defaultValue: "未启用" }),
      },
      {
        label: t("provider.proxyTakeover", { defaultValue: "当前应用接管" }),
        value: isCurrentAppTakeoverActive
          ? t("common.enabled", { defaultValue: "已启用" })
          : t("common.disabled", { defaultValue: "未启用" }),
      },
      {
        label: t("provider.failover", { defaultValue: "故障转移" }),
        value: isAutoFailoverEnabled
          ? t("common.enabled", { defaultValue: "已启用" })
          : t("common.disabled", { defaultValue: "未启用" }),
      },
    ],
    [isAutoFailoverEnabled, isCurrentAppTakeoverActive, isProxyRunning, t],
  );

  if (isLoading) {
    return (
      <div className="px-6 pb-12">
        <div className="grid gap-4 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(0,1.5fr)]">
          <div className="h-[32rem] rounded-2xl border border-dashed border-border bg-muted/30" />
          <div className="h-[32rem] rounded-2xl border border-dashed border-border bg-muted/30" />
        </div>
      </div>
    );
  }

  if (orderedProviders.length === 0) {
    return (
      <div className="px-6 pb-12">
        <ProviderList
          providers={providers}
          currentProviderId={currentProviderId}
          appId={activeApp}
          isLoading={isLoading}
          isProxyRunning={isProxyRunning}
          isProxyTakeover={isProxyRunning && isCurrentAppTakeoverActive}
          activeProviderId={activeProviderId}
          onSwitch={onSwitch}
          onEdit={onEdit}
          onDelete={onDelete}
          onRemoveFromConfig={onRemoveFromConfig}
          onDisableOmo={onDisableOmo}
          onDisableOmoSlim={onDisableOmoSlim}
          onDuplicate={onDuplicate}
          onConfigureUsage={onConfigureUsage}
          onOpenWebsite={onOpenWebsite}
          onOpenTerminal={onOpenTerminal}
          onCreate={onCreate}
          onSetAsDefault={onSetAsDefault}
        />
      </div>
    );
  }

  return (
    <div className="px-6 pb-12">
      <div className="grid min-h-0 gap-4 xl:grid-cols-[minmax(19rem,0.85fr)_minmax(0,1.55fr)]">
        <aside className="min-h-0 rounded-2xl border border-border bg-background">
          <div className="border-b border-border p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p className="text-xs text-muted-foreground">
                  {t("provider.workspaceTitle", {
                    defaultValue: "服务商工作台",
                  })}
                </p>
                <h2 className="mt-1 text-xl font-semibold">
                  {t("provider.workspaceProviders", {
                    defaultValue: "服务商",
                  })}
                </h2>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={isTestingLatency}
                onClick={() => void handleTestProvidersLatency()}
              >
                {isTestingLatency ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Zap className="mr-2 h-4 w-4" />
                )}
                {t("provider.batchLatency", { defaultValue: "批量测速" })}
              </Button>
            </div>

            <div className="mt-4 flex flex-col gap-3">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  ref={searchInputRef}
                  value={searchTerm}
                  onChange={(event) => setSearchTerm(event.target.value)}
                  placeholder={t("provider.searchPlaceholder", {
                    defaultValue: "搜索名称、备注或 URL...",
                  })}
                  className="pl-9"
                  data-testid="service-search-input"
                />
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <div className="inline-flex rounded-xl border border-border bg-muted/30 p-1">
                  {(
                    [
                      {
                        key: "all",
                        label: t("provider.allProviders", {
                          defaultValue: "全部",
                        }),
                      },
                      {
                        key: "favorites",
                        label: t("provider.favoriteProviders", {
                          defaultValue: "收藏",
                        }),
                      },
                    ] as Array<{ key: ProviderScope; label: string }>
                  ).map((item) => (
                    <Button
                      key={item.key}
                      type="button"
                      size="sm"
                      variant={providerScope === item.key ? "default" : "ghost"}
                      className="h-8 px-3"
                      onClick={() => setProviderScope(item.key)}
                      data-testid={`provider-scope-${item.key}`}
                    >
                      {item.label}
                    </Button>
                  ))}
                </div>

                <select
                  value={sortStrategy}
                  onChange={(event) =>
                    setSortStrategy(event.target.value as ProviderSortStrategy)
                  }
                  className="h-9 rounded-xl border border-border bg-background px-3 text-sm text-foreground"
                  data-testid="provider-sort-select"
                  aria-label={t("provider.sortStrategy", {
                    defaultValue: "服务商排序",
                  })}
                >
                  <option value="manual">
                    {t("provider.sortManual", { defaultValue: "手动排序" })}
                  </option>
                  <option value="activeFirst">
                    {t("provider.sortActiveFirst", {
                      defaultValue: "当前优先",
                    })}
                  </option>
                </select>
              </div>
            </div>
          </div>

          <div className="max-h-[calc(100vh-15rem)] space-y-2 overflow-y-auto p-3">
            {visibleProviders.length === 0 ? (
              <div className="rounded-xl border border-dashed border-border bg-muted/20 px-4 py-8 text-center text-sm text-muted-foreground">
                {t("provider.noSearchResults", {
                  defaultValue: "没有匹配的服务商。",
                })}
              </div>
            ) : (
              visibleProviders.map((provider) => {
                const isSelected = provider.id === selectedProviderId;
                const isActive =
                  provider.id === currentProviderId ||
                  provider.id === activeProviderId;
                const latency = latencyResults[provider.id];

                return (
                  <div
                    key={provider.id}
                    role="button"
                    tabIndex={0}
                    className={cn(
                      "w-full rounded-xl border bg-background p-3 text-left transition",
                      "hover:border-primary hover:bg-primary/10",
                      isSelected
                        ? "border-primary bg-primary/10"
                        : "border-border",
                    )}
                    onClick={() => handleCardClick(provider.id)}
                    onDoubleClick={() => handleCardDoubleClick(provider)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        event.stopPropagation();
                        setSelectedProviderId(provider.id);
                        if (
                          event.key === "Enter" &&
                          provider.id !== currentProviderId
                        ) {
                          onSwitch(provider);
                        }
                      }
                    }}
                    data-testid={`service-row-${provider.id}`}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="truncate text-sm font-semibold">
                            {provider.name}
                          </span>
                          {isActive && (
                            <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                              {t("provider.current", {
                                defaultValue: "当前",
                              })}
                            </span>
                          )}
                        </div>
                        <div className="mt-1 truncate text-xs text-muted-foreground">
                          {provider.notes || provider.id}
                        </div>
                      </div>
                      <span
                        className={cn(
                          "rounded-full border border-border bg-muted/40 px-2 py-0.5 text-[11px]",
                          latency?.error
                            ? "text-destructive"
                            : "text-muted-foreground",
                        )}
                      >
                        {formatLatency(t, latency)}
                      </span>
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-2">
                      <span className="truncate text-xs text-muted-foreground">
                        {provider.websiteUrl ||
                          getProviderConnectionDetails(
                            provider,
                            activeProviderApp,
                          ).baseUrl ||
                          t("common.notSet", { defaultValue: "未设置" })}
                      </span>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        className={cn(
                          "h-7 w-7",
                          provider.meta?.favoriteProvider &&
                            "text-amber-500 hover:text-amber-500",
                        )}
                        onClick={(event) => {
                          event.stopPropagation();
                          handleToggleProviderFavorite(provider);
                        }}
                        aria-label={
                          provider.meta?.favoriteProvider
                            ? t("provider.unfavoriteProvider", {
                                defaultValue: "取消收藏服务商",
                              })
                            : t("provider.favoriteProvider", {
                                defaultValue: "收藏服务商",
                              })
                        }
                      >
                        <Star
                          className={cn(
                            "h-4 w-4",
                            provider.meta?.favoriteProvider && "fill-current",
                          )}
                        />
                      </Button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </aside>

        <section className="min-w-0 space-y-4">
          {!selectedProvider ? (
            <div className="rounded-2xl border border-dashed border-border bg-muted/20 px-6 py-12 text-center text-sm text-muted-foreground">
              {t("provider.selectProviderHint", {
                defaultValue: "请选择一个服务商查看详情。",
              })}
            </div>
          ) : (
            <>
              <div
                className="rounded-2xl border border-border bg-background p-4"
                data-testid="service-detail-panel-detail"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p
                      className="text-xs text-muted-foreground"
                      data-testid="detail-provider-id"
                    >
                      {selectedProvider.id}
                    </p>
                    <h3 className="mt-1 truncate text-2xl font-semibold">
                      {selectedProvider.name}
                    </h3>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {selectedBadges.map((badge) => (
                      <span
                        key={`selected-${badge}`}
                        className="rounded-full border border-border bg-muted/50 px-2.5 py-1 text-xs text-muted-foreground"
                      >
                        {badge}
                      </span>
                    ))}
                  </div>
                </div>

                <div className="mt-4 grid gap-4 xl:grid-cols-[1.3fr_1fr]">
                  <div className="space-y-3">
                    <div className="rounded-xl border border-border bg-muted/20 p-3">
                      <div className="text-xs text-muted-foreground">
                        {t("provider.notes", { defaultValue: "备注" })}
                      </div>
                      <div className="mt-2 text-sm text-foreground">
                        {selectedProvider.notes?.trim() ||
                          t("common.notSet", { defaultValue: "未设置" })}
                      </div>
                    </div>
                    <div className="rounded-xl border border-border bg-muted/20 p-3">
                      <div className="text-xs text-muted-foreground">
                        {t("provider.discoveryProtocol", {
                          defaultValue: "发现协议",
                        })}
                      </div>
                      <div className="mt-2 text-sm uppercase text-foreground">
                        {selectedDiscoveryProtocol ||
                          t("common.notSet", { defaultValue: "未设置" })}
                      </div>
                    </div>
                    {selectedProvider.websiteUrl && (
                      <Button
                        type="button"
                        variant="ghost"
                        className="px-0 text-sm text-primary hover:bg-transparent"
                        onClick={() =>
                          onOpenWebsite(selectedProvider.websiteUrl!)
                        }
                      >
                        <ExternalLink className="mr-2 h-4 w-4" />
                        {t("provider.openWebsite", {
                          defaultValue: "打开服务商主页",
                        })}
                      </Button>
                    )}
                  </div>

                  <div className="rounded-xl border border-border bg-muted/10 p-3">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <Shield className="h-4 w-4 text-primary" />
                      {t("provider.proxyCardTitle", {
                        defaultValue: "代理状态",
                      })}
                    </div>
                    <div className="mt-3 space-y-2.5">
                      {proxySummaryItems.map((item) => (
                        <div
                          key={item.label}
                          className="flex items-center justify-between rounded-lg border border-border bg-background px-3 py-2"
                        >
                          <span className="text-sm text-muted-foreground">
                            {item.label}
                          </span>
                          <span className="text-sm font-medium">
                            {item.value}
                          </span>
                        </div>
                      ))}
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="mt-3 px-0 text-sm text-primary hover:bg-transparent"
                      onClick={onOpenProxySettings}
                    >
                      {t("provider.openProxySettings", {
                        defaultValue: "前往代理设置",
                      })}
                    </Button>
                  </div>
                </div>

                <div
                  className="mt-4 rounded-2xl border border-border bg-muted/10 p-3"
                  data-testid="service-detail-actions-inline"
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <Network className="h-4 w-4 text-primary" />
                      {t("provider.sectionActions", {
                        defaultValue: "操作",
                      })}
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={onCreate}
                    >
                      {t("provider.addProvider", {
                        defaultValue: "新增服务商",
                      })}
                    </Button>
                  </div>
                  <div
                    className="mt-3 rounded-2xl border border-border bg-background p-2"
                    data-testid="service-detail-actions-provider-list"
                  >
                    <ProviderList
                      providers={selectedProviderMap}
                      currentProviderId={currentProviderId}
                      appId={activeApp}
                      isLoading={isLoading}
                      isProxyRunning={isProxyRunning}
                      isProxyTakeover={
                        isProxyRunning && isCurrentAppTakeoverActive
                      }
                      activeProviderId={activeProviderId}
                      onSwitch={onSwitch}
                      onEdit={onEdit}
                      onDelete={onDelete}
                      onRemoveFromConfig={onRemoveFromConfig}
                      onDisableOmo={onDisableOmo}
                      onDisableOmoSlim={onDisableOmoSlim}
                      onDuplicate={onDuplicate}
                      onConfigureUsage={onConfigureUsage}
                      onOpenWebsite={onOpenWebsite}
                      onOpenTerminal={onOpenTerminal}
                      onCreate={onCreate}
                      onSetAsDefault={onSetAsDefault}
                      displayMode="single"
                    />
                  </div>
                </div>
              </div>

              <div
                className="rounded-2xl border border-border bg-background p-4"
                data-testid="service-detail-panel-models"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-xs text-muted-foreground">
                      {t("provider.sectionModels", { defaultValue: "模型" })}
                    </div>
                    <h3 className="mt-2 text-lg font-semibold">
                      {canDiscoverModels
                        ? t("provider.modelDiscoveryTitle", {
                            defaultValue: "自动发现模型",
                          })
                        : t("provider.modelCatalogTitle", {
                            defaultValue: "当前模型目录",
                          })}
                    </h3>
                  </div>

                  {canDiscoverModels && (
                    <div className="flex flex-wrap items-center gap-2">
                      <div className="inline-flex rounded-xl border border-border bg-muted/20 p-1">
                        {(
                          [
                            { key: "openai", label: "OpenAI" },
                            { key: "anthropic", label: "Anthropic" },
                          ] as Array<{
                            key: ProviderProtocolHint;
                            label: string;
                          }>
                        ).map((item) => (
                          <Button
                            key={item.key}
                            type="button"
                            size="sm"
                            variant={
                              selectedDiscoveryProtocol === item.key
                                ? "default"
                                : "ghost"
                            }
                            className="h-8 px-3"
                            onClick={() => handleProtocolChange(item.key)}
                          >
                            {item.label}
                          </Button>
                        ))}
                      </div>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={selectedDiscoveryState?.status === "loading"}
                        onClick={() =>
                          void discoverModelsForProvider(
                            selectedProvider,
                            selectedDiscoveryProtocol,
                            true,
                          )
                        }
                        data-testid="discover-models-button"
                      >
                        {selectedDiscoveryState?.status === "loading" ? (
                          <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                        ) : (
                          <RefreshCcw className="mr-2 h-4 w-4" />
                        )}
                        {t("provider.discoverModels", {
                          defaultValue: "重新发现",
                        })}
                      </Button>
                    </div>
                  )}
                </div>

                <div className="mt-4 space-y-3">
                  {canDiscoverModels &&
                    selectedDiscoveryState?.status === "error" && (
                      <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-300">
                        {selectedDiscoveryState.error}
                      </div>
                    )}

                  {!canDiscoverModels && (
                    <div className="rounded-xl border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
                      {t("provider.modelDiscoveryUnsupported", {
                        defaultValue:
                          "当前应用使用静态模型目录，不自动调用远端模型接口。",
                      })}
                    </div>
                  )}

                  {favoriteModelIds.length > 0 && (
                    <div className="inline-flex rounded-xl border border-border bg-muted/20 p-1">
                      {(
                        [
                          {
                            key: "all",
                            label: t("provider.allModels", {
                              defaultValue: "全部模型",
                            }),
                          },
                          {
                            key: "favorites",
                            label: t("provider.favoriteModels", {
                              defaultValue: "收藏模型",
                            }),
                          },
                        ] as Array<{ key: ModelScope; label: string }>
                      ).map((item) => (
                        <Button
                          key={item.key}
                          type="button"
                          size="sm"
                          variant={
                            modelScope === item.key ? "default" : "ghost"
                          }
                          className="h-8 px-3"
                          onClick={() => setModelScope(item.key)}
                        >
                          {item.label}
                        </Button>
                      ))}
                    </div>
                  )}

                  <div className="rounded-2xl border border-border bg-muted/10">
                    <div className="flex items-center justify-between border-b border-border px-4 py-3 text-sm text-muted-foreground">
                      <span>
                        {t("provider.modelCount", {
                          defaultValue: "模型数量",
                        })}
                        : {visibleModels.length}
                      </span>
                      {favoriteModelIds.length > 0 && (
                        <span>
                          {t("provider.favoriteCount", {
                            defaultValue: "收藏",
                          })}
                          : {favoriteModelIds.length}
                        </span>
                      )}
                    </div>

                    {visibleModels.length === 0 ? (
                      <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                        {t("provider.noModels", {
                          defaultValue: "当前没有可展示的模型。",
                        })}
                      </div>
                    ) : (
                      <div className="max-h-[320px] divide-y divide-border overflow-y-auto">
                        {visibleModels.map((model) => {
                          const isFavorite = favoriteModelIds.includes(
                            model.id,
                          );
                          return (
                            <div
                              key={model.id}
                              className="flex items-center justify-between gap-3 px-4 py-3"
                              data-testid={`model-row-${model.id}`}
                            >
                              <div className="min-w-0">
                                <div className="truncate text-sm font-medium">
                                  {model.name}
                                </div>
                                <div className="mt-1 truncate text-xs text-muted-foreground">
                                  {model.id}
                                </div>
                                {(model.provider ||
                                  model.ownedBy ||
                                  model.contextWindow) && (
                                  <div className="mt-1 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                                    {model.provider && (
                                      <span>{model.provider}</span>
                                    )}
                                    {model.ownedBy && (
                                      <span>{model.ownedBy}</span>
                                    )}
                                    {model.contextWindow && (
                                      <span>{model.contextWindow} ctx</span>
                                    )}
                                  </div>
                                )}
                              </div>
                              <Button
                                type="button"
                                size="icon"
                                variant="ghost"
                                className={cn(
                                  "h-8 w-8",
                                  isFavorite &&
                                    "text-amber-500 hover:text-amber-500",
                                )}
                                onClick={() =>
                                  handleToggleModelFavorite(model.id)
                                }
                                aria-label={
                                  isFavorite
                                    ? t("provider.unfavoriteModel", {
                                        defaultValue: "取消收藏模型",
                                      })
                                    : t("provider.favoriteModel", {
                                        defaultValue: "收藏模型",
                                      })
                                }
                              >
                                <Star
                                  className={cn(
                                    "h-4 w-4",
                                    isFavorite && "fill-current",
                                  )}
                                />
                              </Button>
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </div>
                </div>
              </div>

              <div
                className="rounded-2xl border border-border bg-background p-4"
                data-testid="service-detail-panel-test"
              >
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <div className="text-xs text-muted-foreground">
                      {t("provider.sectionTest", { defaultValue: "测试" })}
                    </div>
                    <h3 className="mt-2 text-lg font-semibold">
                      {t("provider.providerTestTitle", {
                        defaultValue: "服务商连通性测试",
                      })}
                    </h3>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isChecking(selectedProvider.id)}
                    onClick={() =>
                      void checkProvider(
                        selectedProvider.id,
                        selectedProvider.name,
                      )
                    }
                  >
                    {isChecking(selectedProvider.id) ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : (
                      <FlaskConical className="mr-2 h-4 w-4" />
                    )}
                    {t("provider.runProviderTest", {
                      defaultValue: "测试当前服务商",
                    })}
                  </Button>
                </div>

                <div className="mt-4 grid gap-3 sm:grid-cols-2">
                  <div className="rounded-xl border border-border bg-muted/20 p-3">
                    <div className="text-xs text-muted-foreground">
                      {t("provider.testModel", { defaultValue: "测试模型" })}
                    </div>
                    <div className="mt-2 text-sm font-medium">
                      {selectedProvider.meta?.testConfig?.testModel ||
                        favoriteModelIds[0] ||
                        discoveredModels[0]?.id ||
                        t("common.notSet", { defaultValue: "未设置" })}
                    </div>
                  </div>
                  <div className="rounded-xl border border-border bg-muted/20 p-3">
                    <div className="text-xs text-muted-foreground">
                      {t("provider.discoveryStatus", {
                        defaultValue: "发现状态",
                      })}
                    </div>
                    <div className="mt-2 text-sm font-medium">
                      {selectedDiscoveryState?.status === "loading"
                        ? t("common.loading", { defaultValue: "加载中" })
                        : selectedDiscoveryState?.status === "success"
                          ? t("provider.discoverySuccess", {
                              defaultValue: "已完成",
                            })
                          : selectedDiscoveryState?.status === "error"
                            ? t("provider.discoveryFailed", {
                                defaultValue: "失败",
                              })
                            : t("common.notSet", { defaultValue: "未设置" })}
                    </div>
                  </div>
                </div>
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
