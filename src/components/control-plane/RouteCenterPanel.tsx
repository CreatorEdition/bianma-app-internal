import { useState } from "react";
import { Network, Play, Settings2, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useProxyStatus } from "@/hooks/useProxyStatus";

const CLIENTS = [
  ["claude", "Claude Code"],
  ["codex", "Codex"],
  ["gemini", "Gemini CLI"],
] as const;

interface RouteCenterPanelProps {
  onOpenAdvanced: () => void;
}

/** 默认路由页只暴露统一运行状态；客户端差异配置必须显式进入高级入口。 */
export function RouteCenterPanel({ onOpenAdvanced }: RouteCenterPanelProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionResult, setConnectionResult] = useState<string | null>(null);
  const {
    status,
    isRunning,
    takeoverStatus,
    startProxyServer,
    stopWithRestore,
    setTakeoverForApp,
    isPending,
  } = useProxyStatus();
  const endpoint = status
    ? `http://${status.address}:${status.port}`
    : "将使用本机默认监听地址";

  const startAndConnect = async () => {
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
            onClick={() => void startAndConnect()}
            disabled={isPending || isConnecting}
            className="gap-2"
          >
            <Play className="h-4 w-4" />
            {isRunning ? "统一接入客户端" : "一键启动并接入"}
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
      <div className="grid gap-8 py-8 lg:grid-cols-[1fr_0.8fr]">
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
              正常情况下不需要逐个配置客户端；点击统一接入即可。
            </p>
            <div className="mt-3 divide-y divide-border border-y border-border">
              {CLIENTS.map(([appType, name]) => (
                <div
                  key={appType}
                  className="flex items-center justify-between py-3"
                >
                  <div>
                    <p className="text-sm font-medium">{name}</p>
                    <p className="text-xs text-muted-foreground">
                      {takeoverStatus?.[appType] ? "已接入统一路由" : "未接入"}
                    </p>
                  </div>
                  <span
                    className={
                      takeoverStatus?.[appType]
                        ? "text-xs font-medium text-emerald-600 dark:text-emerald-400"
                        : "text-xs text-muted-foreground"
                    }
                  >
                    {takeoverStatus?.[appType] ? "已接入" : "待接入"}
                  </span>
                </div>
              ))}
            </div>
            <p className="mt-3 text-xs text-muted-foreground">
              OpenCode 与 OpenClaw 当前不支持自动接入，可在高级配置中手动兼容。
            </p>
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
            仅当你需要为某个客户端改模型映射、Header 或手动兼容
            OpenCode、OpenClaw 时，才进入这里。
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
