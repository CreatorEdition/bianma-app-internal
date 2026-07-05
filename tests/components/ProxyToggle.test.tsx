import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProxyToggle } from "@/components/proxy/ProxyToggle";

const useProxyStatusMock = vi.hoisted(() => vi.fn());

const { translations } = vi.hoisted(() => ({
  translations: {
    "proxy.takeover.tooltip.active":
      "{{appLabel}} 已接管 - {{address}}:{{port}}\n切换该应用供应商为热切换",
    "proxy.takeover.tooltip.broken": "{{appLabel}} 已接管，但代理服务未运行",
    "proxy.takeover.tooltip.inactive":
      "接管 {{appLabel}} 的 Live 配置，让该应用请求走本地代理",
  } as Record<string, string>,
}));

const formatTranslation = (
  key: string,
  options?: Record<string, unknown>,
): string => {
  const template = translations[key];
  if (!template) {
    return key;
  }

  return template.replace(/\{\{(\w+)\}\}/g, (_match, name: string) =>
    String(options?.[name] ?? ""),
  );
};

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      formatTranslation(key, options),
  }),
}));

vi.mock("@/hooks/useProxyStatus", () => ({
  useProxyStatus: () => useProxyStatusMock(),
}));

describe("ProxyToggle", () => {
  beforeEach(() => {
    useProxyStatusMock.mockReturnValue({
      isRunning: false,
      takeoverStatus: {
        claude: false,
        codex: false,
        gemini: false,
        opencode: false,
        openclaw: false,
      },
      setTakeoverForApp: vi.fn().mockResolvedValue(undefined),
      isPending: false,
      status: {
        address: "127.0.0.1",
        port: 15721,
      },
    });
  });

  it("shows the inactive tooltip from translation resources", () => {
    render(<ProxyToggle activeApp="claude" />);

    expect(screen.getByRole("switch").parentElement).toHaveAttribute(
      "title",
      formatTranslation("proxy.takeover.tooltip.inactive", {
        appLabel: "Claude",
      }),
    );
  });

  it("shows the active tooltip from translation resources", () => {
    useProxyStatusMock.mockReturnValue({
      isRunning: true,
      takeoverStatus: {
        claude: true,
        codex: false,
        gemini: false,
        opencode: false,
        openclaw: false,
      },
      setTakeoverForApp: vi.fn().mockResolvedValue(undefined),
      isPending: false,
      status: {
        address: "127.0.0.1",
        port: 15721,
      },
    });

    render(<ProxyToggle activeApp="claude" />);

    expect(screen.getByRole("switch").parentElement).toHaveAttribute(
      "title",
      formatTranslation("proxy.takeover.tooltip.active", {
        appLabel: "Claude",
        address: "127.0.0.1",
        port: 15721,
      }),
    );
  });

  it("shows the broken tooltip from translation resources", () => {
    useProxyStatusMock.mockReturnValue({
      isRunning: false,
      takeoverStatus: {
        claude: true,
        codex: false,
        gemini: false,
        opencode: false,
        openclaw: false,
      },
      setTakeoverForApp: vi.fn().mockResolvedValue(undefined),
      isPending: false,
      status: {
        address: "127.0.0.1",
        port: 15721,
      },
    });

    render(<ProxyToggle activeApp="claude" />);

    expect(screen.getByRole("switch").parentElement).toHaveAttribute(
      "title",
      formatTranslation("proxy.takeover.tooltip.broken", {
        appLabel: "Claude",
      }),
    );
  });

  it("passes the active app and next state to the takeover action", async () => {
    const setTakeoverForApp = vi.fn().mockResolvedValue(undefined);
    useProxyStatusMock.mockReturnValue({
      isRunning: false,
      takeoverStatus: {
        claude: false,
        codex: false,
        gemini: false,
        opencode: false,
        openclaw: false,
      },
      setTakeoverForApp,
      isPending: false,
      status: {
        address: "127.0.0.1",
        port: 15721,
      },
    });

    render(<ProxyToggle activeApp="claude" />);

    const user = userEvent.setup();
    await user.click(screen.getByRole("switch"));

    expect(setTakeoverForApp).toHaveBeenCalledWith({
      appType: "claude",
      enabled: true,
    });
  });
});
