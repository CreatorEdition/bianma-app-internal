import { useEffect, useState } from "react";
import { Boxes, Network, Play, Settings2, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { universalProvidersApi } from "@/lib/api";
import { isUsableUniversalProvider } from "@/lib/universalProvider";

const CLIENTS = [
  ["claude", "Claude Code"],
  ["codex", "Codex"],
  ["gemini", "Gemini CLI"],
] as const;

interface RouteCenterPanelProps {
  onOpenAdvanced: () => void;
  onOpenUpstreams: () => void;
}

/** 默认路由页只暴露统一运行状态；客户端差异配置必须显式进入高级入口。 */
export function RouteCenterPanel({
  onOpenAdvanced,
  onOpenUpstreams,
}: RouteCenterPanelProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionResult, setConnectionResult] = useState<string | null>(null);
  const [upstreamCount, setUpstreamCount] = useState<number | null>(null);
  const {
    status,
    isRunning,
    takeoverStatus,
    startProxyServer,
    stopWithRestore,
    setTakeoverForApp,
    isPending,
  } = useProxyStatus();
  useEffect(() => {
    void universalProvidersApi
      .getAll()
      .then((providers) =>
        setUpstreamCount(
          Object.values(providers).filter(isUsableUniversalProvider).length,
        ),
      )
      .catch(() => setUpstreamCount(0));
  }, []);

  const hasUpstream = (upstreamCount ?? 0) > 0;
  const endpoint = status
    ? `http://${status.address}:${status.port}`
    : "将使用本机默认监听地址";
  const connectedClientCount = CLIENTS.filter(
    ([appType]) => takeoverStatus?.[appType],
  ).length;
  const connectionSummary =
    upstreamCount === null
      ? "正在读取上游"
      : !hasUpstream
        ? "先添加上游"
        : !isRunning
          ? "尚未启动"
          : connectedClientCount === CLIENTS.length
            ? "统一路由已接入"
            : connectedClientCount > 0
              ? `部分接入 · ${connectedClientCount}/${CLIENTS.length}`
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
            : connectedClientCount > 0
              ? "已有部分客户端接入，可再次点击完成其余接入。"
              : "本地入口已启动，点击接入即可完成客户端配置。";

  const startAndConnect = async () => {
    if (!hasUpstream) {
      onOpenUpstreams();
      return;
    }

    setIsConnecting(true);
    setConnectionResult(null);
    try {
      if (!isRunning) await startProxyServer();

      const failedClients: string[] = [];
      let connectedCount = 0;
      for (const [appType, name] of CLIENTS) {
        if (takeoverStatus?.[appType]) {
          connectedCount += 1;
          continue;
        }
        try {
          await setTakeoverForApp({ appType, enabled: true, silent: true });
          connectedCount += 1;
        } catch {
          failedClients.push(name);
        }
      }

      setConnectionResult(
        failedClients.length
          ? `已接入 ${connectedCount}/${CLIENTS.length}；失败：${failedClients.join("、")}`
          : `已接入 ${CLIENTS.length}/${CLIENTS.length} 个支持的客户端`,
      );
    } catch {
      setConnectionResult("本地入口启动失败，请查看错误提示后重试");
    } finally {
      setIsConnecting(false);
    }
  };

  return (
    <section className="mx-auto w-full max-w-5xl px-8 py-8">
      <div className="flex flex-wrap items-start justify-between gap-5 border-b border-border pb-6">
        <div>
          <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
            Routing
          </p>
          <h1 className="mt-2 text-2xl font-semibold">本地路由中心</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            一次启动统一接入当前支持的客户端；例外映射仅在高级配置中创建。
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
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
      <div className="grid gap-8 py-8 lg:grid-cols-[1.15fr_0.65fr]">
        <div className="space-y-6">
          <div>
            <p className="text-xs text-muted-foreground">本地入口</p>
            <code className="mt-2 block border-y border-border py-3 text-sm">
              {endpoint}
            </code>
          </div>
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
        </div>
        <div className="border-l border-border pl-0 lg:pl-8">
          <Network className="h-5 w-5 text-primary" />
          <h2 className="mt-4 text-base font-semibold">高级配置</h2>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            仅当你需要客户端专属模型映射、Header、User-Agent 或手动兼容时，
            才进入这里查看逐客户端设置。
          </p>
          <Button
            variant="ghost"
            onClick={onOpenAdvanced}
            className="mt-5 gap-2 px-0"
          >
            <Settings2 className="h-4 w-4" />
            打开高级配置
          </Button>
        </div>
      </div>
    </section>
  );
}
