import { describe, expect, it } from "vitest";
import { settingsSchema } from "@/lib/schemas/settings";

describe("settingsSchema", () => {
  it("保留 OpenCode 与 OpenClaw 的目录和当前供应商字段", () => {
    const result = settingsSchema.parse({
      showInTray: true,
      minimizeToTrayOnClose: true,
      opencodeConfigDir: "C:\\Users\\demo\\.opencode",
      openclawConfigDir: "C:\\Users\\demo\\.openclaw",
      currentProviderOpencode: "opencode-provider",
      currentProviderOpenclaw: "openclaw-provider",
    });

    expect(result.opencodeConfigDir).toBe("C:\\Users\\demo\\.opencode");
    expect(result.openclawConfigDir).toBe("C:\\Users\\demo\\.openclaw");
    expect(result.currentProviderOpencode).toBe("opencode-provider");
    expect(result.currentProviderOpenclaw).toBe("openclaw-provider");
  });

  it("继续允许 OpenCode 与 OpenClaw 目录为空字符串", () => {
    const result = settingsSchema.parse({
      showInTray: false,
      minimizeToTrayOnClose: false,
      opencodeConfigDir: "",
      openclawConfigDir: "",
    });

    expect(result.opencodeConfigDir).toBe("");
    expect(result.openclawConfigDir).toBe("");
  });
});
