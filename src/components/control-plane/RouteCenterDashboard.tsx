import { useEffect, useMemo, useState } from "react";
import { ArrowRight, Boxes, Network, PlugZap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { universalProvidersApi } from "@/lib/api";
import type { ProxyStatus } from "@/types/proxy";
import type { UniversalProvidersMap } from "@/types";

interface RouteCenterDashboardProps {
  status?: ProxyStatus;
  isProxyRunning: boolean;
  takeoverCount: number;
  onOpenUpstreams: () => void;
  onOpenRoutes: () => void;
}

/** 默认首页只呈现一个本地路由中心，不要求用户先选择客户端。 */
export function RouteCenterDashboard({
  status,
  isProxyRunning,
  takeoverCount,
  onOpenUpstreams,
  onOpenRoutes,
}: RouteCenterDashboardProps) {
  const [upstreams, setUpstreams] = useState<UniversalProvidersMap>({});

  useEffect(() => {
    void universalProvidersApi
      .getAll()
      .then(setUpstreams)
      .catch(() => setUpstreams({}));
  }, []);

  const upstreamList = useMemo(() => Object.values(upstreams), [upstreams]);
  const endpoint = status
    ? `http://${status.address}:${status.port}`
    : "本地入口尚未启动";
  const health = !isProxyRunning
    ? "未启动"
    : status?.last_error
      ? "需要检查"
      : status?.total_requests
        ? "运行正常"
        : "等待请求";

  return (
    <section className="mx-auto w-full max-w-6xl px-8 py-8">
      <div className="border-b border-border pb-6">
        <div className="flex flex-wrap items-start justify-between gap-5">
          <div>
            <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
              Local routing center
            </p>
            <h1 className="mt-2 text-2xl font-semibold tracking-tight">
              统一路由中心
            </h1>
            <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
              配置一次上游，当前支持的客户端通过同一个本地入口请求模型。
            </p>
          </div>
          <Button onClick={onOpenRoutes} className="gap-2">
            <PlugZap className="h-4 w-4" />
            {isProxyRunning ? "查看路由中心" : "启动并接入客户端"}
          </Button>
        </div>
      </div>

      <dl className="grid border-b border-border sm:grid-cols-2 lg:grid-cols-4">
        <StatusItem
          label="路由中心"
          value={isProxyRunning ? "正在运行" : "已停止"}
          detail={endpoint}
          ok={isProxyRunning}
        />
        <StatusItem
          label="上游渠道"
          value={upstreamList.length ? `${upstreamList.length} 个` : "尚未配置"}
          detail={upstreamList.length ? "已保存到本机" : "先添加一个 API 渠道"}
        />
        <StatusItem
          label="客户端接入"
          value={`${takeoverCount} 个`}
          detail="Claude / Codex / Gemini 可自动接入"
        />
        <StatusItem
          label="请求健康"
          value={health}
          detail={
            status
              ? `${status.total_requests} 次请求 · ${status.success_rate.toFixed(1)}% 成功`
              : "启动后显示实时状态"
          }
          ok={health === "运行正常"}
        />
      </dl>

      <div className="grid gap-10 py-8 lg:grid-cols-[1.25fr_0.75fr]">
        <div>
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-base font-semibold">上游渠道</h2>
              <p className="mt-1 text-sm text-muted-foreground">
                这里是统一配置入口，不需要先选择 Claude、Codex 或 Gemini。
              </p>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={onOpenUpstreams}
              className="gap-1"
            >
              管理上游 <ArrowRight className="h-3.5 w-3.5" />
            </Button>
          </div>
          <div className="mt-4 divide-y divide-border border-y border-border">
            {upstreamList.length ? (
              upstreamList.slice(0, 3).map((upstream) => (
                <div
                  key={upstream.id}
                  className="flex items-center justify-between gap-4 py-3"
                >
                  <div className="min-w-0">
                    <p className="font-medium">{upstream.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {upstream.baseUrl}
                    </p>
                  </div>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    已配置
                  </span>
                </div>
              ))
            ) : (
              <button
                type="button"
                onClick={onOpenUpstreams}
                className="w-full py-7 text-left text-sm text-muted-foreground hover:text-foreground"
              >
                还没有上游。添加 API 地址、Key 和默认模型后即可接入。
              </button>
            )}
          </div>
        </div>
        <div className="border-l border-border pl-0 lg:pl-8">
          <h2 className="text-base font-semibold">当前流量目标</h2>
          {status?.active_targets?.length ? (
            <div className="mt-4 divide-y divide-border border-y border-border">
              {status.active_targets.map((target) => (
                <div
                  key={`${target.app_type}-${target.provider_id}`}
                  className="flex items-center justify-between gap-3 py-3 text-sm"
                >
                  <span className="font-medium">{target.app_type}</span>
                  <span className="truncate text-muted-foreground">
                    {target.provider_name}
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <p className="mt-4 border-y border-border py-6 text-sm text-muted-foreground">
              尚无活动目标。启动路由中心并接入客户端后，这里会显示真实转发目标。
            </p>
          )}
          <ol className="mt-5 space-y-3 text-sm">
            <Step
              icon={Boxes}
              title="添加上游"
              detail="填写 API 地址、Key 和默认模型。"
            />
            <Step
              icon={Network}
              title="启动本地入口"
              detail="当前支持 Claude、Codex 与 Gemini 自动接入。"
            />
          </ol>
        </div>
      </div>
    </section>
  );
}

function StatusItem({
  label,
  value,
  detail,
  ok,
}: {
  label: string;
  value: string;
  detail: string;
  ok?: boolean;
}) {
  return (
    <div className="border-b border-border px-0 py-4 sm:border-b-0 sm:border-r sm:px-5 sm:first:pl-0 lg:last:border-r-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd
        className={`mt-1 font-medium ${ok ? "text-emerald-600 dark:text-emerald-400" : ""}`}
      >
        {value}
      </dd>
      <p className="mt-1 truncate text-xs text-muted-foreground">{detail}</p>
    </div>
  );
}

function Step({
  icon: Icon,
  title,
  detail,
}: {
  icon: typeof Boxes;
  title: string;
  detail: string;
}) {
  return (
    <li className="flex gap-3">
      <Icon className="mt-0.5 h-4 w-4 text-primary" />
      <div>
        <p className="font-medium">{title}</p>
        <p className="mt-0.5 text-muted-foreground">{detail}</p>
      </div>
    </li>
  );
}
