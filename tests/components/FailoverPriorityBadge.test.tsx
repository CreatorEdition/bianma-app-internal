import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FailoverPriorityBadge } from "@/components/providers/FailoverPriorityBadge";

const { translations } = vi.hoisted(() => ({
  translations: {
    "failover.priority.tooltip": "故障转移优先级 {{priority}}",
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

describe("FailoverPriorityBadge", () => {
  it("shows the priority tooltip from translation resources", () => {
    render(<FailoverPriorityBadge priority={2} />);

    expect(
      screen.getByTitle(
        formatTranslation("failover.priority.tooltip", { priority: 2 }),
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("P2")).toBeInTheDocument();
  });
});
