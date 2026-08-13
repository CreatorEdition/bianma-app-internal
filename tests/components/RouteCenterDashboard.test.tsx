import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RouteCenterDashboard } from "@/components/control-plane/RouteCenterDashboard";
import type { ProxyStatus } from "@/types/proxy";

const getAllMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  universalProvidersApi: {
    getAll: (...args: unknown[]) => getAllMock(...args),
  },
}));

const runningStatus: ProxyStatus = {
  running: true,
  address: "127.0.0.1",
  port: 15721,
  active_connections: 1,
  total_requests: 12,
  success_requests: 11,
  failed_requests: 1,
  success_rate: 91.7,
  uptime_seconds: 60,
  current_provider: "主线路",
  current_provider_id: "primary",
  last_request_at: "2026-08-12T00:00:00Z",
  last_error: null,
  failover_count: 0,
  active_targets: [
    {
      app_type: "codex",
      provider_name: "主线路",
      provider_id: "primary",
    },
  ],
};

describe("RouteCenterDashboard", () => {
  beforeEach(() => {
    getAllMock.mockReset();
  });

  it("只展示可证明的配置和运行事实", async () => {
    getAllMock.mockResolvedValue({
      primary: {
        id: "primary",
        name: "主线路",
        providerType: "custom_gateway",
        apps: { claude: true, codex: true, gemini: true },
        baseUrl: "https://api.example.com",
        apiKey: "secret",
        models: {
          claude: { model: "gpt-5.4" },
          codex: { model: "gpt-5.4" },
          gemini: { model: "gpt-5.4" },
        },
      },
    });

    render(
      <RouteCenterDashboard
        status={runningStatus}
        isProxyRunning
        takeoverCount={2}
        onOpenUpstreams={vi.fn()}
        onOpenRoutes={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("主线路")).toBeInTheDocument();
    });
    expect(screen.getByText("正在运行")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:15721")).toBeInTheDocument();
    expect(screen.getByText("12 次请求 · 91.7% 成功")).toBeInTheDocument();
    expect(screen.getByText("codex")).toBeInTheDocument();
    expect(screen.queryByText("默认可用")).not.toBeInTheDocument();
  });

  it("没有上游时主按钮直接进入上游配置", async () => {
    const onOpenUpstreams = vi.fn();
    const onOpenRoutes = vi.fn();
    getAllMock.mockResolvedValue({});

    render(
      <RouteCenterDashboard
        isProxyRunning={false}
        takeoverCount={0}
        onOpenUpstreams={onOpenUpstreams}
        onOpenRoutes={onOpenRoutes}
      />,
    );

    const addUpstream = await screen.findByText("先添加上游");
    fireEvent.click(addUpstream);

    expect(onOpenUpstreams).toHaveBeenCalledTimes(1);
    expect(onOpenRoutes).not.toHaveBeenCalled();
  });

  it("空壳上游不会解锁路由中心", async () => {
    const onOpenUpstreams = vi.fn();
    const onOpenRoutes = vi.fn();
    getAllMock.mockResolvedValue({
      broken: {
        id: "broken",
        name: "损坏配置",
        providerType: "custom_gateway",
        apps: { claude: true, codex: true, gemini: true },
        baseUrl: " ",
        apiKey: " ",
        models: {},
      },
    });

    render(
      <RouteCenterDashboard
        isProxyRunning={false}
        takeoverCount={0}
        onOpenUpstreams={onOpenUpstreams}
        onOpenRoutes={onOpenRoutes}
      />,
    );

    fireEvent.click(await screen.findByText("先添加上游"));

    expect(onOpenUpstreams).toHaveBeenCalledTimes(1);
    expect(onOpenRoutes).not.toHaveBeenCalled();
  });
});
