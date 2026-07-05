import type { TFunction } from "i18next";
import type { AppId } from "@/lib/api";
import type { ProviderFormData } from "@/lib/schemas/provider";
import type {
  ClaudeApiFormat,
  ClaudeApiKeyField,
  OpenClawProviderConfig,
  OpenCodeProviderConfig,
  ProviderCategory,
} from "@/types";
import type { ProviderPreset } from "@/config/claudeProviderPresets";
import type { CodexProviderPreset } from "@/config/codexProviderPresets";
import type { GeminiProviderPreset } from "@/config/geminiProviderPresets";
import type { OpenCodeProviderPreset } from "@/config/opencodeProviderPresets";
import type {
  OpenClawProviderPreset,
  OpenClawSuggestedDefaults,
} from "@/config/openclawProviderPresets";
import { getCodexCustomTemplate } from "@/config/codexTemplates";
import { applyTemplateValues } from "@/utils/providerConfigUtils";
import type { ProviderPresetEntry } from "./providerPresetUtils";

type PresetFormValues = Pick<
  ProviderFormData,
  "name" | "websiteUrl" | "settingsConfig" | "icon" | "iconColor"
>;

type PresetDisplayConfig = {
  name: string;
  nameKey?: string;
  websiteUrl?: string;
  icon?: string;
  iconColor?: string;
};

export interface ActivePresetState {
  id: string;
  category?: ProviderCategory;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  suggestedDefaults?: OpenClawSuggestedDefaults;
}

interface PresetSelectionBase {
  activePreset: ActivePresetState;
  formValues: PresetFormValues;
}

export type PresetSelectionResult =
  | (PresetSelectionBase & {
      appId: "codex";
      codexAuth: Record<string, unknown>;
      codexConfig: string;
    })
  | (PresetSelectionBase & {
      appId: "gemini";
      geminiEnv: Record<string, unknown>;
      geminiConfig: Record<string, unknown>;
    })
  | (PresetSelectionBase & {
      appId: "opencode";
      isOmoPreset: boolean;
      opencodeConfig?: OpenCodeProviderConfig;
    })
  | (PresetSelectionBase & {
      appId: "openclaw";
      openclawConfig: OpenClawProviderConfig;
    })
  | (PresetSelectionBase & {
      appId: "claude";
      apiFormat: ClaudeApiFormat;
      apiKeyField: ClaudeApiKeyField;
      isFullUrl: boolean;
    });

interface ResolvePresetSelectionParams {
  appId: AppId;
  presetId: string;
  entry: ProviderPresetEntry;
  t: TFunction;
}

export interface CustomPresetResetPlan {
  codex?: {
    auth: Record<string, unknown>;
    config: string;
  };
  shouldResetGemini: boolean;
  shouldResetOpencode: boolean;
  shouldResetOmoDraft: boolean;
  shouldResetOpenclaw: boolean;
}

function buildPresetFormValues(
  preset: PresetDisplayConfig,
  settingsConfig: unknown,
  t: TFunction,
  nameOverride?: string,
): PresetFormValues {
  return {
    name: nameOverride ?? (preset.nameKey ? t(preset.nameKey) : preset.name),
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify(settingsConfig, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  };
}

function buildActivePresetState(
  presetId: string,
  entry: ProviderPresetEntry,
): ActivePresetState {
  return {
    id: presetId,
    category: entry.preset.category,
    isPartner: entry.preset.isPartner,
    partnerPromotionKey: entry.preset.partnerPromotionKey,
  };
}

/**
 * 返回自定义预设分支需要重置的应用状态。
 */
export function getCustomPresetResetPlan(appId: AppId): CustomPresetResetPlan {
  if (appId === "codex") {
    const template = getCodexCustomTemplate();
    return {
      codex: {
        auth: template.auth,
        config: template.config,
      },
      shouldResetGemini: false,
      shouldResetOpencode: false,
      shouldResetOmoDraft: false,
      shouldResetOpenclaw: false,
    };
  }

  if (appId === "gemini") {
    return {
      shouldResetGemini: true,
      shouldResetOpencode: false,
      shouldResetOmoDraft: false,
      shouldResetOpenclaw: false,
    };
  }

  if (appId === "opencode") {
    return {
      shouldResetGemini: false,
      shouldResetOpencode: true,
      shouldResetOmoDraft: true,
      shouldResetOpenclaw: false,
    };
  }

  if (appId === "openclaw") {
    return {
      shouldResetGemini: false,
      shouldResetOpencode: false,
      shouldResetOmoDraft: false,
      shouldResetOpenclaw: true,
    };
  }

  return {
    shouldResetGemini: false,
    shouldResetOpencode: false,
    shouldResetOmoDraft: false,
    shouldResetOpenclaw: false,
  };
}

/**
 * 将选中的预设解析为 ProviderForm 可直接应用的状态变更描述。
 */
export function resolvePresetSelection({
  appId,
  presetId,
  entry,
  t,
}: ResolvePresetSelectionParams): PresetSelectionResult {
  const activePreset = buildActivePresetState(presetId, entry);

  if (appId === "codex") {
    const preset = entry.preset as CodexProviderPreset;
    const codexAuth = preset.auth ?? {};
    const codexConfig = preset.config ?? "";

    return {
      appId,
      activePreset,
      formValues: buildPresetFormValues(
        preset,
        { auth: codexAuth, config: codexConfig },
        t,
      ),
      codexAuth,
      codexConfig,
    };
  }

  if (appId === "gemini") {
    const preset = entry.preset as GeminiProviderPreset;
    const geminiEnv = (preset.settingsConfig as any)?.env ?? {};
    const geminiConfig = (preset.settingsConfig as any)?.config ?? {};

    return {
      appId,
      activePreset,
      formValues: buildPresetFormValues(preset, preset.settingsConfig, t),
      geminiEnv,
      geminiConfig,
    };
  }

  if (appId === "opencode") {
    const preset = entry.preset as OpenCodeProviderPreset;

    if (preset.category === "omo" || preset.category === "omo-slim") {
      return {
        appId,
        activePreset,
        formValues: buildPresetFormValues(
          preset,
          {},
          t,
          preset.category === "omo" ? "OMO" : "OMO Slim",
        ),
        isOmoPreset: true,
      };
    }

    return {
      appId,
      activePreset,
      formValues: buildPresetFormValues(preset, preset.settingsConfig, t),
      isOmoPreset: false,
      opencodeConfig: preset.settingsConfig,
    };
  }

  if (appId === "openclaw") {
    const preset = entry.preset as OpenClawProviderPreset;

    return {
      appId,
      activePreset: {
        ...activePreset,
        suggestedDefaults: preset.suggestedDefaults,
      },
      formValues: buildPresetFormValues(preset, preset.settingsConfig, t),
      openclawConfig: preset.settingsConfig,
    };
  }

  const preset = entry.preset as ProviderPreset;
  const settingsConfig = applyTemplateValues(
    preset.settingsConfig,
    preset.templateValues,
  );

  return {
    appId: "claude",
    activePreset,
    formValues: buildPresetFormValues(preset, settingsConfig, t),
    apiFormat: preset.apiFormat ?? "anthropic",
    apiKeyField: preset.apiKeyField ?? "ANTHROPIC_AUTH_TOKEN",
    isFullUrl: false,
  };
}
