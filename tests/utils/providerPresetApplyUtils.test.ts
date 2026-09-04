import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import type { ProviderPresetEntry } from "@/components/providers/forms/providerPresetUtils";
import {
  getCustomPresetResetPlan,
  resolvePresetSelection,
} from "@/components/providers/forms/providerPresetApplyUtils";

const t = ((key: string) => {
  if (key === "preset.localized") {
    return "本地化名称";
  }
  return key;
}) as unknown as TFunction;

describe("providerPresetApplyUtils", () => {
  it("按应用生成 custom 分支重置计划", () => {
    const codexPlan = getCustomPresetResetPlan("codex");
    expect(codexPlan.codex).toBeDefined();
    expect(codexPlan.shouldResetGemini).toBe(false);

    const geminiPlan = getCustomPresetResetPlan("gemini");
    expect(geminiPlan.shouldResetGemini).toBe(true);

    const opencodePlan = getCustomPresetResetPlan("opencode");
    expect(opencodePlan.shouldResetOpencode).toBe(true);
    expect(opencodePlan.shouldResetOmoDraft).toBe(true);

    const openclawPlan = getCustomPresetResetPlan("openclaw");
    expect(openclawPlan.shouldResetOpenclaw).toBe(true);

    const claudePlan = getCustomPresetResetPlan("claude");
    expect(claudePlan.codex).toBeUndefined();
    expect(claudePlan.shouldResetGemini).toBe(false);
    expect(claudePlan.shouldResetOpencode).toBe(false);
    expect(claudePlan.shouldResetOmoDraft).toBe(false);
    expect(claudePlan.shouldResetOpenclaw).toBe(false);
  });

  it("解析 Codex 预设选择结果", () => {
    const entry = {
      id: "codex-0",
      preset: {
        name: "Codex Provider",
        websiteUrl: "https://codex.example.invalid",
        auth: { mode: "local" },
        config: 'model = "gpt-5.4"',
      },
    } as ProviderPresetEntry;

    const result = resolvePresetSelection({
      appId: "codex",
      presetId: entry.id,
      entry,
      t,
    });

    expect(result.appId).toBe("codex");
    if (result.appId !== "codex") {
      throw new Error("unexpected app branch");
    }
    expect(result.activePreset.id).toBe("codex-0");
    expect(result.codexAuth).toEqual({ mode: "local" });
    expect(result.codexConfig).toBe('model = "gpt-5.4"');
    expect(result.formValues.settingsConfig).toContain('"auth"');
    expect(result.formValues.settingsConfig).toContain('"config"');
  });

  it("解析 Gemini 预设并使用本地化名称", () => {
    const entry = {
      id: "gemini-0",
      preset: {
        name: "Gemini Provider",
        nameKey: "preset.localized",
        websiteUrl: "https://gemini.example.invalid",
        settingsConfig: {
          env: { GEMINI_REGION: "global" },
          config: { retries: 3 },
        },
      },
    } as ProviderPresetEntry;

    const result = resolvePresetSelection({
      appId: "gemini",
      presetId: entry.id,
      entry,
      t,
    });

    expect(result.appId).toBe("gemini");
    if (result.appId !== "gemini") {
      throw new Error("unexpected app branch");
    }
    expect(result.formValues.name).toBe("本地化名称");
    expect(result.geminiEnv).toEqual({ GEMINI_REGION: "global" });
    expect(result.geminiConfig).toEqual({ retries: 3 });
  });

  it("解析 OpenCode OMO 预设为空配置表单值", () => {
    const entry = {
      id: "opencode-omo",
      preset: {
        name: "Omo Provider",
        websiteUrl: "https://omo.example.invalid",
        category: "omo",
        settingsConfig: {
          npm: "@ai-sdk/openai-compatible",
          options: {},
          models: {},
        },
      },
    } as ProviderPresetEntry;

    const result = resolvePresetSelection({
      appId: "opencode",
      presetId: entry.id,
      entry,
      t,
    });

    expect(result.appId).toBe("opencode");
    if (result.appId !== "opencode") {
      throw new Error("unexpected app branch");
    }
    expect(result.isOmoPreset).toBe(true);
    expect(result.formValues.name).toBe("OMO");
    expect(result.formValues.settingsConfig).toBe("{}");
  });

  it("保留 OpenClaw suggestedDefaults 到 activePreset", () => {
    const openclawConfig = {
      baseUrl: "https://api.openclaw.example.invalid",
    };
    const suggestedDefaults = {
      model: {
        primary: "openclaw/model-a",
      },
    };
    const entry = {
      id: "openclaw-0",
      preset: {
        name: "OpenClaw Provider",
        websiteUrl: "https://openclaw.example.invalid",
        settingsConfig: openclawConfig,
        suggestedDefaults,
      },
    } as ProviderPresetEntry;

    const result = resolvePresetSelection({
      appId: "openclaw",
      presetId: entry.id,
      entry,
      t,
    });

    expect(result.appId).toBe("openclaw");
    if (result.appId !== "openclaw") {
      throw new Error("unexpected app branch");
    }
    expect(result.activePreset.suggestedDefaults).toEqual(suggestedDefaults);
    expect(result.openclawConfig).toEqual(openclawConfig);
  });

  it("应用 Claude 模板变量并填充默认认证字段", () => {
    const entry = {
      id: "claude-0",
      preset: {
        name: "Claude Template",
        websiteUrl: "https://claude.example.invalid",
        settingsConfig: {
          env: {
            ANTHROPIC_BASE_URL: "${BASE_URL}",
          },
        },
        templateValues: {
          BASE_URL: {
            label: "Base URL",
            editorValue: "https://api.claude.example.invalid",
          },
        },
      },
    } as ProviderPresetEntry;

    const result = resolvePresetSelection({
      appId: "claude",
      presetId: entry.id,
      entry,
      t,
    });

    expect(result.appId).toBe("claude");
    if (result.appId !== "claude") {
      throw new Error("unexpected app branch");
    }
    expect(result.apiFormat).toBe("anthropic");
    expect(result.apiKeyField).toBe("ANTHROPIC_AUTH_TOKEN");
    expect(result.isFullUrl).toBe(false);
    expect(result.formValues.settingsConfig).toContain(
      "https://api.claude.example.invalid",
    );
  });
});
