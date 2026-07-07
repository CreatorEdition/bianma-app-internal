import type { ReactNode } from "react";
import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProviderPresetSelector } from "@/components/providers/forms/ProviderPresetSelector";
import type { ProviderPreset } from "@/config/claudeProviderPresets";

vi.mock("@/components/ui/form", () => ({
  FormLabel: ({ children }: { children: ReactNode }) => (
    <label>{children}</label>
  ),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) =>
      options?.defaultValue ?? key,
  }),
}));

describe("ProviderPresetSelector", () => {
  it("不再基于 isPartner 渲染星标促销徽标", () => {
    const partnerPreset: ProviderPreset = {
      name: "Partner Provider",
      websiteUrl: "https://provider.example.invalid",
      settingsConfig: { env: {} },
      isPartner: true,
      partnerPromotionKey: "packycode",
      category: "third_party",
    };

    const { container, getByRole } = render(
      <ProviderPresetSelector
        selectedPresetId={null}
        groupedPresets={{
          third_party: [{ id: "partner-provider", preset: partnerPreset }],
        }}
        categoryKeys={["third_party"]}
        presetCategoryLabels={{ third_party: "第三方" }}
        onPresetChange={vi.fn()}
        category="third_party"
      />,
    );

    expect(
      getByRole("button", { name: "Partner Provider" }),
    ).toBeInTheDocument();
    expect(container.querySelector(".from-amber-500")).not.toBeInTheDocument();
    expect(container.querySelector(".to-yellow-500")).not.toBeInTheDocument();
  });
});
