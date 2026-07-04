import { describe, expect, it } from "vitest";
import {
  isProviderKeyValid,
  normalizeProviderKeyInput,
  PROVIDER_KEY_PATTERN,
} from "@/components/providers/forms/providerKeyUtils";

describe("providerKeyUtils", () => {
  it("将供应商标识输入归一化为小写字母、数字与连字符", () => {
    expect(normalizeProviderKeyInput("AbC_12-xx !@#")).toBe("abc12-xx");
    expect(normalizeProviderKeyInput("OPEN.Claw/Provider")).toBe(
      "openclawprovider",
    );
  });

  it("接受合法供应商标识", () => {
    expect(isProviderKeyValid("provider-1")).toBe(true);
    expect(isProviderKeyValid("abc")).toBe(true);
    expect(PROVIDER_KEY_PATTERN.test("openclaw-provider-2")).toBe(true);
  });

  it("拒绝空值、连续连字符或首尾连字符", () => {
    expect(isProviderKeyValid("")).toBe(false);
    expect(isProviderKeyValid("provider--1")).toBe(false);
    expect(isProviderKeyValid("-provider")).toBe(false);
    expect(isProviderKeyValid("provider-")).toBe(false);
  });
});
