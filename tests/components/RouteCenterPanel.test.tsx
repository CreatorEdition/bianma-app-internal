import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RouteCenterPanel } from "@/components/control-plane/RouteCenterPanel";

const getAllMock = vi.hoisted(() => vi.fn());
const syncMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  universalProvidersApi: {
    getAll: (...args: unknown[]) => getAllMock(...args),
    sync: (...args: unknown[]) => syncMock(...args),
  },
}));

describe("RouteCenterPanel", () => {
  beforeEach(() => {
    getAllMock.mockReset();
    syncMock.mockReset();
    localStorage.clear();
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
    syncMock.mockResolvedValue({
      providerId: "primary",
      success: true,
      apps: [],
    });
  });

  it("默认只展示聚合状态，并在接入失败时给出可操作结果", async () => {
    const setTakeoverForApp = vi.fn(({ appType }: { appType: string }) =>
      appType === "codex"
        ? Promise.reject(new Error("broken"))
        : Promise.resolve(),
    );
    const switchProxyProvider = vi.fn().mockResolvedValue(undefined);

    const startProxyServer = vi.fn().mockResolvedValue(undefined);

    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={startProxyServer}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={setTakeoverForApp}
        switchProxyProvider={switchProxyProvider}
        isPending={false}
      />,
    );

    await waitFor(() =>
      expect(screen.getByText("一键启动并接入")).toBeEnabled(),
    );
    expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
      "尚未启动",
    );
    expect(screen.queryByText("Claude Code")).not.toBeInTheDocument();
    expect(screen.queryByText("Gemini CLI")).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("一键启动并接入"));

    await waitFor(() => {
      expect(screen.getByTestId("connection-result")).toHaveTextContent(
        "已接入 2/3；失败：Codex",
      );
    });
    expect(setTakeoverForApp).toHaveBeenCalledTimes(3);
    expect(syncMock).toHaveBeenCalledWith("primary");
    expect(setTakeoverForApp).toHaveBeenNthCalledWith(1, {
      appType: "claude",
      enabled: true,
      silent: true,
    });
    expect(switchProxyProvider).toHaveBeenCalledWith({
      appType: "claude",
      providerId: "universal-claude-primary",
    });
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });

  it("统一上游准备失败时不会启动本地入口", async () => {
    const startProxyServer = vi.fn();
    const setTakeoverForApp = vi.fn();
    const switchProxyProvider = vi.fn();
    syncMock.mockResolvedValueOnce({
      providerId: "primary",
      success: false,
      apps: [],
    });
    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={startProxyServer}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={setTakeoverForApp}
        switchProxyProvider={switchProxyProvider}
        isPending={false}
      />,
    );
    fireEvent.click(await screen.findByText("一键启动并接入"));

    await waitFor(() =>
      expect(screen.getByTestId("connection-result")).toHaveTextContent(
        "当前上游准备失败：主线路",
      ),
    );
    expect(startProxyServer).not.toHaveBeenCalled();
    expect(setTakeoverForApp).not.toHaveBeenCalled();
  });

  it("选择非默认当前上游后，三个客户端都切换到该上游", async () => {
    localStorage.setItem("bianma-universal-active-provider", "secondary");
    getAllMock.mockResolvedValue({
      primary: {
        id: "primary",
        name: "主线路",
        providerType: "custom_gateway",
        sortIndex: 0,
        apps: { claude: true, codex: true, gemini: true },
        baseUrl: "https://primary.example.com",
        apiKey: "primary-key",
        models: {
          claude: { model: "gpt-5.4" },
          codex: { model: "gpt-5.4" },
          gemini: { model: "gpt-5.4" },
        },
      },
      secondary: {
        id: "secondary",
        name: "备用线路",
        providerType: "custom_gateway",
        sortIndex: 1,
        apps: { claude: true, codex: true, gemini: true },
        baseUrl: "https://secondary.example.com",
        apiKey: "secondary-key",
        models: {
          claude: { model: "gpt-5.4" },
          codex: { model: "gpt-5.4" },
          gemini: { model: "gpt-5.4" },
        },
      },
    });
    const switchProxyProvider = vi.fn().mockResolvedValue(undefined);

    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={vi.fn().mockResolvedValue(undefined)}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={vi.fn().mockResolvedValue(undefined)}
        switchProxyProvider={switchProxyProvider}
        isPending={false}
      />,
    );

    fireEvent.click(await screen.findByText("一键启动并接入"));

    await waitFor(() =>
      expect(screen.getByTestId("connection-result")).toHaveTextContent(
        "已接入 3/3 个支持的客户端",
      ),
    );
    expect(switchProxyProvider).toHaveBeenCalledWith({
      appType: "claude",
      providerId: "universal-claude-secondary",
    });
    expect(switchProxyProvider).toHaveBeenCalledWith({
      appType: "codex",
      providerId: "universal-codex-secondary",
    });
    expect(switchProxyProvider).toHaveBeenCalledWith({
      appType: "gemini",
      providerId: "universal-gemini-secondary",
    });
  });

  it("当前上游热切换失败时不报告三端成功", async () => {
    const setTakeoverForApp = vi.fn().mockResolvedValue(undefined);
    const switchProxyProvider = vi.fn(({ appType }: { appType: string }) =>
      appType === "codex"
        ? Promise.reject(new Error("switch failed"))
        : Promise.resolve(),
    );

    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={vi.fn().mockResolvedValue(undefined)}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={setTakeoverForApp}
        switchProxyProvider={switchProxyProvider}
        isPending={false}
      />,
    );

    fireEvent.click(await screen.findByText("一键启动并接入"));

    await waitFor(() =>
      expect(screen.getByTestId("connection-result")).toHaveTextContent(
        "已接入 2/3；失败：Codex（当前上游切换失败）",
      ),
    );
    expect(setTakeoverForApp).toHaveBeenCalledTimes(3);
    expect(setTakeoverForApp).toHaveBeenCalledWith({
      appType: "codex",
      enabled: true,
      silent: true,
    });
    expect(setTakeoverForApp.mock.invocationCallOrder[1]).toBeLessThan(
      switchProxyProvider.mock.invocationCallOrder[1],
    );
  });

  it("全部接入后只呈现统一路由已接入", async () => {
    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        status={{
          address: "127.0.0.1",
          port: 15721,
          active_targets: [
            {
              app_type: "claude",
              provider_id: "universal-claude-primary",
              provider_name: "主线路",
            },
            {
              app_type: "codex",
              provider_id: "universal-codex-primary",
              provider_name: "主线路",
            },
            {
              app_type: "gemini",
              provider_id: "universal-gemini-primary",
              provider_name: "主线路",
            },
          ],
        }}
        isRunning
        takeoverStatus={{
          claude: true,
          codex: true,
          gemini: true,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={vi.fn()}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={vi.fn()}
        switchProxyProvider={vi.fn()}
        isPending={false}
      />,
    );

    await waitFor(() =>
      expect(screen.getByText("统一接入客户端")).toBeEnabled(),
    );
    expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
      "统一路由已接入",
    );
    expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
      "所有支持自动接入的客户端均已配置指向这个本地入口",
    );
    expect(screen.queryByText("Claude Code")).not.toBeInTheDocument();
    expect(screen.queryByText("Codex")).not.toBeInTheDocument();
    expect(screen.queryByText("Gemini CLI")).not.toBeInTheDocument();
  });

  it("三个客户端虽已接管但目标不一致时提示重新统一接入", async () => {
    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        status={{
          address: "127.0.0.1",
          port: 15721,
          active_targets: [
            {
              app_type: "claude",
              provider_id: "legacy-claude",
              provider_name: "旧线路 A",
            },
            {
              app_type: "codex",
              provider_id: "legacy-codex",
              provider_name: "旧线路 B",
            },
            {
              app_type: "gemini",
              provider_id: "legacy-gemini",
              provider_name: "旧线路 C",
            },
          ],
        }}
        isRunning
        takeoverStatus={{
          claude: true,
          codex: true,
          gemini: true,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={vi.fn()}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={vi.fn()}
        switchProxyProvider={vi.fn()}
        isPending={false}
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
        "需要重新统一接入",
      ),
    );
    expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
      "已确认 0/3 个客户端使用当前上游",
    );
    expect(screen.queryByText("统一路由已接入")).not.toBeInTheDocument();
  });

  it("备用上游准备失败时仍使用可用的当前上游启动", async () => {
    getAllMock.mockResolvedValue({
      primary: {
        id: "primary",
        name: "主线路",
        providerType: "custom_gateway",
        sortIndex: 0,
        apps: { claude: true, codex: true, gemini: true },
        baseUrl: "https://primary.example.com",
        apiKey: "primary-key",
        models: {
          claude: { model: "gpt-5.4" },
          codex: { model: "gpt-5.4" },
          gemini: { model: "gpt-5.4" },
        },
      },
      secondary: {
        id: "secondary",
        name: "备用线路",
        providerType: "custom_gateway",
        sortIndex: 1,
        apps: { claude: true, codex: true, gemini: true },
        baseUrl: "https://secondary.example.com",
        apiKey: "secondary-key",
        models: {
          claude: { model: "gpt-5.4" },
          codex: { model: "gpt-5.4" },
          gemini: { model: "gpt-5.4" },
        },
      },
    });
    syncMock.mockImplementation(async (id: string) => ({
      providerId: id,
      success: id === "primary",
      apps: [],
    }));
    const startProxyServer = vi.fn().mockResolvedValue(undefined);

    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={vi.fn()}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={startProxyServer}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={vi.fn().mockResolvedValue(undefined)}
        switchProxyProvider={vi.fn().mockResolvedValue(undefined)}
        isPending={false}
      />,
    );

    fireEvent.click(await screen.findByText("一键启动并接入"));

    await waitFor(() =>
      expect(screen.getByTestId("connection-result")).toHaveTextContent(
        "已接入 3/3 个支持的客户端；备用上游未准备：备用线路",
      ),
    );
    expect(startProxyServer).toHaveBeenCalledTimes(1);
  });

  it("没有上游时只引导添加上游，不启动代理或接管客户端", async () => {
    const onOpenUpstreams = vi.fn();
    const startProxyServer = vi.fn();
    const setTakeoverForApp = vi.fn();
    const switchProxyProvider = vi.fn();
    getAllMock.mockResolvedValue({});
    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={onOpenUpstreams}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={startProxyServer}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={setTakeoverForApp}
        switchProxyProvider={switchProxyProvider}
        isPending={false}
      />,
    );

    const addUpstream = await screen.findByRole("button", {
      name: "先添加上游",
    });
    fireEvent.click(addUpstream);

    expect(onOpenUpstreams).toHaveBeenCalledTimes(1);
    expect(startProxyServer).not.toHaveBeenCalled();
    expect(setTakeoverForApp).not.toHaveBeenCalled();
  });

  it("只覆盖部分客户端的上游不会解锁统一启动", async () => {
    const onOpenUpstreams = vi.fn();
    const startProxyServer = vi.fn();
    const setTakeoverForApp = vi.fn();
    const switchProxyProvider = vi.fn();
    getAllMock.mockResolvedValue({
      broken: {
        id: "broken",
        name: "损坏配置",
        providerType: "custom_gateway",
        apps: { claude: true, codex: false, gemini: false },
        baseUrl: "https://api.example.com",
        apiKey: "secret",
        models: { claude: { model: "claude-sonnet-4" } },
      },
    });
    render(
      <RouteCenterPanel
        onOpenAdvanced={vi.fn()}
        onOpenUpstreams={onOpenUpstreams}
        isRunning={false}
        takeoverStatus={{
          claude: false,
          codex: false,
          gemini: false,
          opencode: false,
          openclaw: false,
        }}
        startProxyServer={startProxyServer}
        stopWithRestore={vi.fn()}
        setTakeoverForApp={setTakeoverForApp}
        switchProxyProvider={switchProxyProvider}
        isPending={false}
      />,
    );

    fireEvent.click(await screen.findByRole("button", { name: "先添加上游" }));

    expect(onOpenUpstreams).toHaveBeenCalledTimes(1);
    expect(startProxyServer).not.toHaveBeenCalled();
    expect(setTakeoverForApp).not.toHaveBeenCalled();
  });
});
