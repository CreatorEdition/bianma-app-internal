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

  it("逐项接入支持的客户端并报告部分失败", async () => {
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
    expect(
      screen.getByText(/OpenCode 与 OpenClaw 当前不支持自动接入/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });
});
