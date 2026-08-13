import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RouteCenterPanel } from "@/components/control-plane/RouteCenterPanel";

const useProxyStatusMock = vi.hoisted(() => vi.fn());

vi.mock("@/hooks/useProxyStatus", () => ({
  useProxyStatus: () => useProxyStatusMock(),
}));

describe("RouteCenterPanel", () => {
  beforeEach(() => {
    useProxyStatusMock.mockReset();
  });

  it("默认只展示聚合状态，并在接入失败时给出可操作结果", async () => {
    const setTakeoverForApp = vi.fn(({ appType }: { appType: string }) =>
      appType === "codex"
        ? Promise.reject(new Error("broken"))
        : Promise.resolve(),
    );

    useProxyStatusMock.mockReturnValue({
      status: undefined,
      isRunning: false,
      takeoverStatus: {
        claude: false,
        codex: false,
        gemini: false,
        opencode: false,
        openclaw: false,
      },
      startProxyServer: vi.fn().mockResolvedValue(undefined),
      stopWithRestore: vi.fn(),
      setTakeoverForApp,
      isPending: false,
    });

    render(<RouteCenterPanel onOpenAdvanced={vi.fn()} />);

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
    expect(setTakeoverForApp).toHaveBeenNthCalledWith(1, {
      appType: "claude",
      enabled: true,
      silent: true,
    });
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });

  it("全部接入后只呈现统一路由已接入", () => {
    useProxyStatusMock.mockReturnValue({
      status: { address: "127.0.0.1", port: 15721 },
      isRunning: true,
      takeoverStatus: {
        claude: true,
        codex: true,
        gemini: true,
        opencode: false,
        openclaw: false,
      },
      startProxyServer: vi.fn(),
      stopWithRestore: vi.fn(),
      setTakeoverForApp: vi.fn(),
      isPending: false,
    });

    render(<RouteCenterPanel onOpenAdvanced={vi.fn()} />);

    expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
      "统一路由已接入",
    );
    expect(screen.getByTestId("aggregate-takeover-status")).toHaveTextContent(
      "所有支持自动接入的客户端都在使用这个本地入口",
    );
    expect(screen.queryByText("Claude Code")).not.toBeInTheDocument();
    expect(screen.queryByText("Codex")).not.toBeInTheDocument();
    expect(screen.queryByText("Gemini CLI")).not.toBeInTheDocument();
  });
});
