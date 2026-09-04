import type { TFunction } from "i18next";
import type { AppId } from "@/lib/api";
import type { ProviderCategory } from "@/types";
import { parseOmoOtherFieldsObject } from "@/types/omo";
import { isProviderKeyValid } from "./providerKeyUtils";

interface ValidateProviderSpecificFieldsParams {
  appId: AppId;
  isAnyOmoCategory: boolean;
  isProviderKeyLockStateLoading: boolean;
  isProviderKeyLocked: boolean;
  opencodeProviderKey: string;
  openclawProviderKey: string;
  additiveExistingProviderKeys: string[];
  opencodeModels: Record<string, unknown>;
  t: TFunction;
}

interface IsGithubCopilotProviderParams {
  templateProviderType?: string;
  initialProviderType?: string;
  baseUrl: string;
}

export function isGithubCopilotProvider({
  templateProviderType,
  initialProviderType,
  baseUrl,
}: IsGithubCopilotProviderParams): boolean {
  let isCopilotHost = false;
  try {
    isCopilotHost = new URL(baseUrl).hostname === "api.githubcopilot.com";
  } catch {
    // Invalid or empty endpoints cannot identify a Copilot provider.
  }

  return (
    templateProviderType === "github_copilot" ||
    initialProviderType === "github_copilot" ||
    isCopilotHost
  );
}

/**
 * 校验 OpenCode/OpenClaw 提交前的供应商标识与模型配置。
 */
export function validateProviderSpecificFields({
  appId,
  isAnyOmoCategory,
  isProviderKeyLockStateLoading,
  isProviderKeyLocked,
  opencodeProviderKey,
  openclawProviderKey,
  additiveExistingProviderKeys,
  opencodeModels,
  t,
}: ValidateProviderSpecificFieldsParams): string | null {
  if (appId === "opencode" && !isAnyOmoCategory) {
    if (!opencodeProviderKey.trim()) {
      return t("opencode.providerKeyRequired");
    }
    if (!isProviderKeyValid(opencodeProviderKey)) {
      return t("opencode.providerKeyInvalid");
    }
    if (isProviderKeyLockStateLoading) {
      return t("providerForm.providerKeyStatusLoading", {
        defaultValue: "正在加载供应商标识状态，请稍后再试",
      });
    }
    if (
      !isProviderKeyLocked &&
      additiveExistingProviderKeys.includes(opencodeProviderKey)
    ) {
      return t("opencode.providerKeyDuplicate");
    }
    if (Object.keys(opencodeModels).length === 0) {
      return t("opencode.modelsRequired");
    }
  }

  if (appId === "openclaw") {
    if (!openclawProviderKey.trim()) {
      return t("openclaw.providerKeyRequired");
    }
    if (!isProviderKeyValid(openclawProviderKey)) {
      return t("openclaw.providerKeyInvalid");
    }
    if (isProviderKeyLockStateLoading) {
      return t("providerForm.providerKeyStatusLoading", {
        defaultValue: "正在加载供应商标识状态，请稍后再试",
      });
    }
    if (
      !isProviderKeyLocked &&
      additiveExistingProviderKeys.includes(openclawProviderKey)
    ) {
      return t("openclaw.providerKeyDuplicate");
    }
  }

  return null;
}

interface ValidateNonOfficialCredentialsParams {
  appId: AppId;
  category: ProviderCategory | undefined;
  isCopilotProvider: boolean;
  isCopilotAuthenticated: boolean;
  baseUrl: string;
  apiKey: string;
  codexBaseUrl: string;
  codexApiKey: string;
  geminiBaseUrl: string;
  geminiApiKey: string;
  t: TFunction;
}

export function validateNonOfficialCredentials({
  appId,
  category,
  isCopilotProvider,
  isCopilotAuthenticated,
  baseUrl,
  apiKey,
  codexBaseUrl,
  codexApiKey,
  geminiBaseUrl,
  geminiApiKey,
  t,
}: ValidateNonOfficialCredentialsParams): string | null {
  if (isCopilotProvider && !isCopilotAuthenticated) {
    return t("copilot.loginRequired", {
      defaultValue: "请先登录 GitHub Copilot",
    });
  }

  if (category === "official" || category === "cloud_provider") {
    return null;
  }

  if (appId === "claude") {
    if (!baseUrl.trim()) {
      return t("providerForm.endpointRequired", {
        defaultValue: "非官方供应商请填写 API 端点",
      });
    }
    if (!isCopilotProvider && !apiKey.trim()) {
      return t("providerForm.apiKeyRequired", {
        defaultValue: "非官方供应商请填写 API Key",
      });
    }
  } else if (appId === "codex") {
    if (!codexBaseUrl.trim()) {
      return t("providerForm.endpointRequired", {
        defaultValue: "非官方供应商请填写 API 端点",
      });
    }
    if (!codexApiKey.trim()) {
      return t("providerForm.apiKeyRequired", {
        defaultValue: "非官方供应商请填写 API Key",
      });
    }
  } else if (appId === "gemini") {
    if (!geminiBaseUrl.trim()) {
      return t("providerForm.endpointRequired", {
        defaultValue: "非官方供应商请填写 API 端点",
      });
    }
    if (!geminiApiKey.trim()) {
      return t("providerForm.apiKeyRequired", {
        defaultValue: "非官方供应商请填写 API Key",
      });
    }
  }

  return null;
}

interface ResolveSubmitSettingsConfigParams {
  appId: AppId;
  category: ProviderCategory | undefined;
  rawSettingsConfig: string;
  codexAuth: string;
  codexConfig: string;
  geminiEnv: string;
  geminiConfig: string;
  envStringToObj: (value: string) => Record<string, unknown>;
  omoAgents: Record<string, unknown>;
  omoCategories: Record<string, unknown>;
  omoOtherFieldsStr: string;
  t: TFunction;
}

export type ResolveSubmitSettingsConfigResult =
  | {
      ok: true;
      settingsConfig: string;
    }
  | {
      ok: false;
      errorMessage: string;
    };

export function resolveSubmitSettingsConfig({
  appId,
  category,
  rawSettingsConfig,
  codexAuth,
  codexConfig,
  geminiEnv,
  geminiConfig,
  envStringToObj,
  omoAgents,
  omoCategories,
  omoOtherFieldsStr,
  t,
}: ResolveSubmitSettingsConfigParams): ResolveSubmitSettingsConfigResult {
  if (appId === "codex") {
    try {
      const authJson = JSON.parse(codexAuth);
      return {
        ok: true,
        settingsConfig: JSON.stringify({
          auth: authJson,
          config: codexConfig ?? "",
        }),
      };
    } catch {
      return {
        ok: true,
        settingsConfig: rawSettingsConfig.trim(),
      };
    }
  }

  if (appId === "gemini") {
    try {
      const envObj = envStringToObj(geminiEnv);
      const configObj = geminiConfig.trim() ? JSON.parse(geminiConfig) : {};
      return {
        ok: true,
        settingsConfig: JSON.stringify({
          env: envObj,
          config: configObj,
        }),
      };
    } catch {
      return {
        ok: true,
        settingsConfig: rawSettingsConfig.trim(),
      };
    }
  }

  if (appId === "opencode" && (category === "omo" || category === "omo-slim")) {
    const omoConfig: Record<string, unknown> = {};
    if (Object.keys(omoAgents).length > 0) {
      omoConfig.agents = omoAgents;
    }
    if (category === "omo" && Object.keys(omoCategories).length > 0) {
      omoConfig.categories = omoCategories;
    }
    if (omoOtherFieldsStr.trim()) {
      try {
        const otherFields = parseOmoOtherFieldsObject(omoOtherFieldsStr);
        if (!otherFields) {
          return {
            ok: false,
            errorMessage: t("omo.jsonMustBeObject", {
              field: t("omo.otherFields", {
                defaultValue: "Other Config",
              }),
              defaultValue: "{{field}} must be a JSON object",
            }),
          };
        }
        omoConfig.otherFields = otherFields;
      } catch {
        return {
          ok: false,
          errorMessage: t("omo.invalidJson", {
            defaultValue: "Other Fields contains invalid JSON",
          }),
        };
      }
    }
    return {
      ok: true,
      settingsConfig: JSON.stringify(omoConfig),
    };
  }

  return {
    ok: true,
    settingsConfig: rawSettingsConfig.trim(),
  };
}
