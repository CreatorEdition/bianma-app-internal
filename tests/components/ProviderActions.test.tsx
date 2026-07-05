import { render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProviderActions } from "@/components/providers/ProviderActions";

const { tMock, translations } = vi.hoisted(() => {
  const translations = {
    "failover.inQueue": "资源文案：故障队列中",
    "failover.addQueue": "资源文案：加入故障队列",
    "provider.addToConfig": "资源文案：添加到配置",
    "provider.removeFromConfig": "资源文案：从配置移除",
    "provider.setAsDefault": "资源文案：设为默认模型",
    "provider.isDefault": "资源文案：当前默认模型",
    "common.edit": "编辑",
    "provider.duplicate": "复制",
    "common.delete": "删除",
  } as Record<string, string>;

  return {
    translations,
    tMock: vi.fn((key: string, _options?: unknown) => translations[key] ?? key),
  };
});

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: tMock,
  }),
}));

const noop = () => {};

const renderProviderActions = (
  overrides: Partial<ComponentProps<typeof ProviderActions>> = {},
) =>
  render(
    <ProviderActions
      isCurrent={false}
      onSwitch={noop}
      onEdit={noop}
      onDuplicate={noop}
      onDelete={noop}
      {...overrides}
    />,
  );

const expectResourceButton = (key: keyof typeof translations) => {
  expect(
    screen.getByRole("button", { name: translations[key] }),
  ).toBeInTheDocument();
  expect(
    tMock.mock.calls.some(
      ([calledKey, options]) =>
        calledKey === key &&
        typeof options === "object" &&
        options !== null &&
        Object.prototype.hasOwnProperty.call(options, "defaultValue"),
    ),
  ).toBe(false);
};

describe("ProviderActions", () => {
  beforeEach(() => {
    tMock.mockClear();
  });

  it("shows the add-to-queue label from translation resources", () => {
    renderProviderActions({
      appId: "claude",
      isAutoFailoverEnabled: true,
      isInFailoverQueue: false,
      onToggleFailover: noop,
    });

    expectResourceButton("failover.addQueue");
  });

  it("shows the in-queue label from translation resources", () => {
    renderProviderActions({
      appId: "claude",
      isAutoFailoverEnabled: true,
      isInFailoverQueue: true,
      onToggleFailover: noop,
    });

    expectResourceButton("failover.inQueue");
  });

  it("shows the add-to-config label from translation resources", () => {
    renderProviderActions({
      appId: "opencode",
      isInConfig: false,
    });

    expectResourceButton("provider.addToConfig");
  });

  it("shows the remove-from-config label from translation resources", () => {
    renderProviderActions({
      appId: "opencode",
      isInConfig: true,
    });

    expectResourceButton("provider.removeFromConfig");
  });

  it("shows the set-as-default label from translation resources", () => {
    renderProviderActions({
      appId: "openclaw",
      isInConfig: true,
      isDefaultModel: false,
      onSetAsDefault: noop,
    });

    expectResourceButton("provider.setAsDefault");
  });

  it("shows the current-default label from translation resources", () => {
    renderProviderActions({
      appId: "openclaw",
      isInConfig: true,
      isDefaultModel: true,
      onSetAsDefault: noop,
    });

    expectResourceButton("provider.isDefault");
  });
});
