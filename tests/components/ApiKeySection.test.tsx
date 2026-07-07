import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiKeySection } from "@/components/providers/forms/shared/ApiKeySection";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => {
      if (key === "providerForm.getApiKey") {
        return "获取 API Key";
      }
      if (key === "providerForm.partnerPromotion.packycode") {
        return "合作方促销文案";
      }
      return options?.defaultValue ?? key;
    },
  }),
}));

describe("ApiKeySection", () => {
  it("保留 API Key 获取链接但不展示合作方促销文案", () => {
    render(
      <ApiKeySection
        value=""
        onChange={vi.fn()}
        category="third_party"
        shouldShowLink
        websiteUrl="https://provider.example.invalid/register"
        isPartner
        partnerPromotionKey="packycode"
      />,
    );

    const link = screen.getByRole("link", { name: "获取 API Key" });
    expect(link).toHaveAttribute(
      "href",
      "https://provider.example.invalid/register",
    );
    expect(screen.queryByText("合作方促销文案")).not.toBeInTheDocument();
  });
});
