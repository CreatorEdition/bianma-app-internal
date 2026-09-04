import { useEffect, useState } from "react";
import { Boxes, Play, Settings2, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { universalProvidersApi } from "@/lib/api";
import { isUniversalRouteProvider } from "@/lib/universalProvider";
import { getActiveUniversalProviderId } from "@/lib/universalProviderSelection";
import type {
  ProxyServerInfo,
  ProxyStatus,
  ProxyTakeoverStatus,
} from "@/types/proxy";

const CLIENTS = [
  ["claude", "Claude Code"],
  ["codex", "Codex"],
  ["gemini", "Gemini CLI"],
] as const;

type UniversalProviderFailure = { id: string; name: string };

interface RouteCenterPanelProps {
  onOpenAdvanced: () => void;
  onOpenUpstreams: () => void;
  status?: Pick<ProxyStatus, "address" | "port" | "active_targets">;
  isRunning: boolean;
  takeoverStatus?: ProxyTakeoverStatus;
  startProxyServer: () => Promise<ProxyServerInfo>;
  stopWithRestore: () => Promise<unknown>;
  setTakeoverForApp: (variables: {
    appType: string;
    enabled: boolean;
    silent?: boolean;
  }) => Promise<unknown>;
  switchProxyProvider: (variables: {
    appType: string;
    providerId: string;
  }) => Promise<unknown>;
  isPending: boolean;
}

/** 默认路由页只暴露统一运行状态；客户端差异配置必须显式进入高级入口。 */
export function RouteCenterPanel({
  onOpenAdvanced,
  onOpenUpstreams,
  status,
  isRunning,
  takeoverStatus,
  startProxyServer,
  stopWithRestore,
  setTakeoverForApp,
  switchProxyProvider,
  isPending,
}: RouteCenterPanelProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionResult, setConnectionResult] = useState<string | null>(null);
  const [upstreamCount, setUpstreamCount] = useState<number | null>(null);
  const [activeProviderId, setActiveProviderId] = useState<string | null>(null);
  useEffect(() => {
    void universalProvidersApi
      .getAll()
      .then((providers) => {
        setUpstreamCount(
          Object.values(providers).filter(isUniversalRouteProvider).length,
        );
        setActiveProviderId(getActiveUniversalProviderId(providers));
      })
      .catch(() => {
        setUpstreamCount(0);
        setActiveProviderId(null);
      });
  }, []);

  const hasUpstream = (upstreamCount ?? 0) > 0;
  const endpoint = status
    ? `http://${status.address}:${status.port}`
    : "将使用本机默认监听地址";
  const takeoverClientCount = CLIENTS.filter(
    ([appType]) => takeoverStatus?.[appType],
  ).length;
  const connectedClientCount = activeProviderId
    ? CLIENTS.filter(
        ([appType]) =>
          takeoverStatus?.[appType] &&
          status?.active_targets?.some(
            (target) =>
              target.app_type === appType &&
              target.provider_id === `universal-${appType}-${activeProviderId}`,
          ),
      ).length
    : 0;
  const connectionSummary =
    upstreamCount === null
      ? "正在读取上游"
      : !hasUpstream
        ? "先添加上游"
        : !isRunning
          ? "尚未启动"
          : connectedClientCount === CLIENTS.length
            ? "统一路由已接入"
            : takeoverClientCount > 0
              ? "需要重新统一接入"
              : "等待接入";
  const connectionDetail =
    upstreamCount === null
      ? "确认本机是否已有可用的统一上游配置。"
      : !hasUpstream
        ? "填写 API 地址、Key 和默认模型后，再一键接入支持的客户端。"
        : !isRunning
          ? "点击一次即可启动本地入口并接入支持的客户端。"
          : connectedClientCount === CLIENTS.length
            ? "所有支持自动接入的客户端均已配置指向这个本地入口。"
            : takeoverClientCount > 0
              ? `已确认 ${connectedClientCount}/${CLIENTS.length} 个客户端使用当前上游；再次接入会统一修正。`
              : "本地入口已启动，点击接入即可完成客户端配置。";

  const startAndConnect = async () => {
    if (!hasUpstream) {
      onOpenUpstreams();
      return;
    }

    setIsConnecting(true);
    setConnectionResult(null);
    try {
      const providerMap = await universalProvidersApi.getAll();
      const providers = Object.values(providerMap).filter(
        isUniversalRouteProvider,
      );
      if (providers.length === 0) {
        setUpstreamCount(0);
        setActiveProviderId(null);
        onOpenUpstreams();
        return;
      }

      const primaryProviderId = getActiveUniversalProviderId(providerMap);
      const primaryProvider = providers.find(
        (provider) => provider.id === primaryProviderId,
      );
      if (!primaryProvider) {
        setConnectionResult("没有可用的当前上游，请先检查上游配置");
        onOpenUpstreams();
        return;
      }
      setActiveProviderId(primaryProvider.id);

      const failedUpstreams: UniversalProviderFailure[] = [];
      for (const provider of providers) {
        try {
          const result = await universalProvidersApi.sync(provider.id);
          if (!result.success) {
            failedUpstreams.push({ id: provider.id, name: provider.name });
          }
        } catch {
          failedUpstreams.push({ id: provider.id, name: provider.name });
        }
      }
      const primaryFailure = failedUpstreams.find(
        (provider) => provider.id === primaryProvider.id,
      );
      if (primaryFailure) {
        setConnectionResult(
          `当前上游准备失败：${primaryFailure.name}。修正后再次启动即可重试`,
        );
        return;
      }
      const failedCandidates = failedUpstreams.filter(
        (provider) => provider.id !== primaryProvider.id,
      );

      if (!isRunning) await startProxyServer();

      const failedClients: string[] = [];
      let connectedCount = 0;
      for (const [appType, name] of CLIENTS) {
        if (!takeoverStatus?.[appType]) {
          try {
            await setTakeoverForApp({ appType, enabled: true, silent: true });
          } catch {
            failedClients.push(name);
            continue;
          }
        }

        try {
          await switchProxyProvider({
            appType,
            providerId: `universal-${appType}-${primaryProvider.id}`,
          });
        } catch {
          failedClients.push(`${name}（当前上游切换失败）`);
          continue;
        }

        connectedCount += 1;
      }

      const clientResult = failedClients.length
        ? `已接入 ${connectedCount}/${CLIENTS.length}；失败：${failedClients.join("、")}`
        : `已接入 ${CLIENTS.length}/${CLIENTS.length} 个支持的客户端`;
      const candidateWarning = failedCandidates.length
        ? `；备用上游未准备：${failedCandidates.map((provider) => provider.name).join("、")}`
        : "";
      setConnectionResult(`${clientResult}${candidateWarning}`);
    } catch {
      setConnectionResult("本地入口启动失败，请查看错误提示后重试");
    } finally {
      setIsConnecting(false);
    }
  };

  return (
    <section className="mx-auto w-full max-w-4xl px-8 py-8">
      <div className="flex flex-wrap items-start justify-between gap-5 border-b border-border pb-6">
        <div>
          <h1 className="text-2xl font-semibold">路由中心</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            所有支持的客户端共用同一个本地入口和上游配置。
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="ghost"
            size="icon"
            onClick={onOpenAdvanced}
            aria-label="高级配置"
            title="高级配置"
          >
            <Settings2 className="h-4 w-4" />
          </Button>
          <Button
            onClick={() =>
              hasUpstream ? void startAndConnect() : onOpenUpstreams()
            }
            disabled={isPending || isConnecting || upstreamCount === null}
            className="gap-2"
          >
            {hasUpstream ? (
              <Play className="h-4 w-4" />
            ) : (
              <Boxes className="h-4 w-4" />
            )}
            {upstreamCount === null
              ? "正在读取上游"
              : hasUpstream
                ? isRunning
                  ? "统一接入客户端"
                  : "一键启动并接入"
                : "先添加上游"}
          </Button>
          {isRunning ? (
            <Button
              variant="outline"
              onClick={() => void stopWithRestore()}
              disabled={isPending || isConnecting}
              className="gap-2"
            >
              <Square className="h-4 w-4" />
              停止并恢复客户端
            </Button>
          ) : null}
        </div>
      </div>
      <div className="grid gap-8 py-8 md:grid-cols-[minmax(0,1fr)_220px]">
        <div>
          <h2 className="text-base font-semibold">统一接入状态</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            正常情况下不需要逐个查看或配置客户端。
          </p>
          <div
            className="mt-3 flex items-start gap-3 border-y border-border py-5"
            data-testid="aggregate-takeover-status"
          >
            <span
              className={`mt-1 h-2.5 w-2.5 shrink-0 rounded-full ${
                isRunning && connectedClientCount === CLIENTS.length
                  ? "bg-emerald-500"
                  : isRunning
                    ? "bg-amber-500"
                    : "bg-muted-foreground/40"
              }`}
              aria-hidden="true"
            />
            <div>
              <p className="text-sm font-medium">{connectionSummary}</p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {connectionDetail}
              </p>
            </div>
          </div>
          {connectionResult ? (
            <p
              className="mt-3 text-sm font-medium"
              role="status"
              data-testid="connection-result"
            >
              {connectionResult}
            </p>
          ) : null}
        </div>
        <div>
          <p className="text-xs text-muted-foreground">本地入口</p>
          <code className="mt-2 block border-y border-border py-3 text-sm">
            {endpoint}
          </code>
          <button
            type="button"
            onClick={onOpenUpstreams}
            className="mt-5 flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
          >
            <Boxes className="h-4 w-4" />
            {upstreamCount === null
              ? "读取上游"
              : `已配置 ${upstreamCount} 个上游`}
          </button>
        </div>
      </div>
    </section>
  );
}
