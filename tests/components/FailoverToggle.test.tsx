import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FailoverToggle } from "@/components/proxy/FailoverToggle";

const { useAutoFailoverEnabledMock, useSetAutoFailoverEnabledMock } =
  vi.hoisted(() => ({
    useAutoFailoverEnabledMock: vi.fn(),
    useSetAutoFailoverEnabledMock: vi.fn(),
  }));

const { translations } = vi.hoisted(() => ({
  translations: {
    "failover.tooltip.enabled":
      "{{app}} 故障转移已启用\n按队列优先级（P1→P2→...）选择供应商",
    "failover.tooltip.disabled":
      "启用 {{app}} 故障转移\n将立即切换到队列 P1，并在失败时自动切换到下一个",
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

vi.mock("@/lib/query/failover", () => ({
  useAutoFailoverEnabled: (...args: unknown[]) =>
    useAutoFailoverEnabledMock(...args),
  useSetAutoFailoverEnabled: () => useSetAutoFailoverEnabledMock(),
}));

describe("FailoverToggle", () => {
  beforeEach(() => {
    useAutoFailoverEnabledMock.mockReturnValue({
      data: false,
      isLoading: false,
    });
    useSetAutoFailoverEnabledMock.mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    });
  });

  it("shows the disabled tooltip from translation resources", () => {
    render(<FailoverToggle activeApp="claude" />);

    expect(screen.getByRole("switch").parentElement).toHaveAttribute(
      "title",
      formatTranslation("failover.tooltip.disabled", {
        app: "Claude",
      }),
    );
  });

  it("shows the enabled tooltip from translation resources", () => {
    useAutoFailoverEnabledMock.mockReturnValue({
      data: true,
      isLoading: false,
    });

    render(<FailoverToggle activeApp="claude" />);

    expect(screen.getByRole("switch").parentElement).toHaveAttribute(
      "title",
      formatTranslation("failover.tooltip.enabled", {
        app: "Claude",
      }),
    );
  });

  it("passes the app and next state to the failover action", async () => {
    const mutate = vi.fn();
    useSetAutoFailoverEnabledMock.mockReturnValue({
      mutate,
      isPending: false,
    });

    render(<FailoverToggle activeApp="claude" />);

    const user = userEvent.setup();
    await user.click(screen.getByRole("switch"));

    expect(mutate).toHaveBeenCalledWith({
      appType: "claude",
      enabled: true,
    });
  });
});
