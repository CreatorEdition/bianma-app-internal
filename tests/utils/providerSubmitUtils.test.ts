import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import {
  isGithubCopilotProvider,
  resolveSubmitSettingsConfig,
  validateNonOfficialCredentials,
  validateProviderSpecificFields,
} from "@/components/providers/forms/providerSubmitUtils";

const t = ((key: string, options?: Record<string, unknown>) => {
  if (
    options &&
    "defaultValue" in options &&
    typeof options.defaultValue === "string"
  ) {
    return options.defaultValue;
  }
  return key;
}) as unknown as TFunction;

const baseProviderValidation = {
  appId: "opencode" as const,
  isAnyOmoCategory: false,
  isProviderKeyLockStateLoading: false,
  isProviderKeyLocked: false,
  opencodeProviderKey: "valid-key",
  openclawProviderKey: "valid-key",
  additiveExistingProviderKeys: [],
  opencodeModels: { "model-a": {} },
  t,
};

describe("providerSubmitUtils", () => {
  it("从预设、元数据或 baseUrl 判断 GitHub Copilot", () => {
    expect(
      isGithubCopilotProvider({
        templateProviderType: "github_copilot",
        initialProviderType: undefined,
        baseUrl: "",
      }),
    ).toBe(true);

    expect(
      isGithubCopilotProvider({
        templateProviderType: undefined,
        initialProviderType: "github_copilot",
        baseUrl: "",
      }),
    ).toBe(true);

    expect(
      isGithubCopilotProvider({
        templateProviderType: undefined,
        initialProviderType: undefined,
        baseUrl: "https://api.githubcopilot.com/v1",
      }),
    ).toBe(true);

    expect(
      isGithubCopilotProvider({
        templateProviderType: undefined,
        initialProviderType: undefined,
        baseUrl: "https://example.invalid/v1",
      }),
    ).toBe(false);
  });

  it("校验 OpenCode provider key 必填", () => {
    expect(
      validateProviderSpecificFields({
        ...baseProviderValidation,
        opencodeProviderKey: "",
      }),
    ).toBe("opencode.providerKeyRequired");
  });

  it("校验 OpenCode provider key 格式", () => {
    expect(
      validateProviderSpecificFields({
        ...baseProviderValidation,
        opencodeProviderKey: "bad--key",
      }),
    ).toBe("opencode.providerKeyInvalid");
  });

  it("校验 OpenCode provider key 状态加载中", () => {
    expect(
      validateProviderSpecificFields({
        ...baseProviderValidation,
        isProviderKeyLockStateLoading: true,
      }),
    ).toBe("正在加载供应商标识状态，请稍后再试");
  });

  it("校验 OpenCode provider key 重复", () => {
    expect(
      validateProviderSpecificFields({
        ...baseProviderValidation,
        opencodeProviderKey: "existing-key",
        additiveExistingProviderKeys: ["existing-key"],
      }),
    ).toBe("opencode.providerKeyDuplicate");
  });

  it("校验 OpenCode models 必填", () => {
    expect(
      validateProviderSpecificFields({
        ...baseProviderValidation,
        opencodeModels: {},
      }),
    ).toBe("opencode.modelsRequired");
  });

  it("OpenClaw locked 状态不把当前 key 判为重复", () => {
    expect(
      validateProviderSpecificFields({
        ...baseProviderValidation,
        appId: "openclaw",
        isProviderKeyLocked: true,
        openclawProviderKey: "existing-key",
        additiveExistingProviderKeys: ["existing-key"],
      }),
    ).toBeNull();
  });

  it("校验非官方供应商端点与凭据", () => {
    expect(
      validateNonOfficialCredentials({
        appId: "claude",
        category: "third_party",
        isCopilotProvider: false,
        isCopilotAuthenticated: false,
        baseUrl: "",
        apiKey: "",
        codexBaseUrl: "",
        codexApiKey: "",
        geminiBaseUrl: "",
        geminiApiKey: "",
        t,
      }),
    ).toBe("非官方供应商请填写 API 端点");

    expect(
      validateNonOfficialCredentials({
        appId: "claude",
        category: "third_party",
        isCopilotProvider: true,
        isCopilotAuthenticated: false,
        baseUrl: "https://api.githubcopilot.com",
        apiKey: "",
        codexBaseUrl: "",
        codexApiKey: "",
        geminiBaseUrl: "",
        geminiApiKey: "",
        t,
      }),
    ).toBe("请先登录 GitHub Copilot");
  });

  it("解析 Codex 提交配置", () => {
    const result = resolveSubmitSettingsConfig({
      appId: "codex",
      category: "official",
      rawSettingsConfig: "{}",
      codexAuth: JSON.stringify({ mode: "local" }),
      codexConfig: 'model = "gpt-5"',
      geminiEnv: "",
      geminiConfig: "",
      envStringToObj: () => ({}),
      omoAgents: {},
      omoCategories: {},
      omoOtherFieldsStr: "",
      t,
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(JSON.parse(result.settingsConfig)).toEqual({
        auth: { mode: "local" },
        config: 'model = "gpt-5"',
      });
    }
  });

  it("解析 Gemini 提交配置", () => {
    const result = resolveSubmitSettingsConfig({
      appId: "gemini",
      category: "third_party",
      rawSettingsConfig: "{}",
      codexAuth: "{}",
      codexConfig: "",
      geminiEnv: "GEMINI_REGION=global",
      geminiConfig: '{"retries":3}',
      envStringToObj: (value) =>
        Object.fromEntries(value.split("\n").map((line) => line.split("="))),
      omoAgents: {},
      omoCategories: {},
      omoOtherFieldsStr: "",
      t,
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(JSON.parse(result.settingsConfig)).toEqual({
        env: { GEMINI_REGION: "global" },
        config: { retries: 3 },
      });
    }
  });

  it("解析 OMO 提交配置并拒绝非对象 otherFields", () => {
    const okResult = resolveSubmitSettingsConfig({
      appId: "opencode",
      category: "omo",
      rawSettingsConfig: "{}",
      codexAuth: "{}",
      codexConfig: "",
      geminiEnv: "",
      geminiConfig: "",
      envStringToObj: () => ({}),
      omoAgents: { coder: { model: "model-a" } },
      omoCategories: { quick: { model: "model-b" } },
      omoOtherFieldsStr: '{"experimental":true}',
      t,
    });

    expect(okResult.ok).toBe(true);
    if (okResult.ok) {
      expect(JSON.parse(okResult.settingsConfig)).toEqual({
        agents: { coder: { model: "model-a" } },
        categories: { quick: { model: "model-b" } },
        otherFields: { experimental: true },
      });
    }

    const invalidResult = resolveSubmitSettingsConfig({
      appId: "opencode",
      category: "omo",
      rawSettingsConfig: "{}",
      codexAuth: "{}",
      codexConfig: "",
      geminiEnv: "",
      geminiConfig: "",
      envStringToObj: () => ({}),
      omoAgents: {},
      omoCategories: {},
      omoOtherFieldsStr: "[]",
      t,
    });

    expect(invalidResult.ok).toBe(false);
    if (!invalidResult.ok) {
      expect(invalidResult.errorMessage).toContain("must be a JSON object");
    }
  });
});
