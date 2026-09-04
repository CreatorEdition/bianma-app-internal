import type { TFunction } from "i18next";
import type { AppId } from "@/lib/api";
import {
  providerPresets,
  type ProviderPreset,
} from "@/config/claudeProviderPresets";
import {
  codexProviderPresets,
  type CodexProviderPreset,
} from "@/config/codexProviderPresets";
import {
  geminiProviderPresets,
  type GeminiProviderPreset,
} from "@/config/geminiProviderPresets";
import {
  opencodeProviderPresets,
  type OpenCodeProviderPreset,
} from "@/config/opencodeProviderPresets";
import {
  openclawProviderPresets,
  type OpenClawProviderPreset,
} from "@/config/openclawProviderPresets";

type ProviderPresetUnion =
  | ProviderPreset
  | CodexProviderPreset
  | GeminiProviderPreset
  | OpenCodeProviderPreset
  | OpenClawProviderPreset;

export interface ProviderPresetEntry {
  id: string;
  preset: ProviderPresetUnion;
}

function toPresetEntries(
  prefix: "claude" | "codex" | "gemini" | "opencode" | "openclaw",
  presets: ProviderPresetUnion[],
): ProviderPresetEntry[] {
  return presets.map((preset, index) => ({
    id: `${prefix}-${index}`,
    preset,
  }));
}

function isVisiblePreset(preset: ProviderPresetUnion): boolean {
  return !("hidden" in preset && preset.hidden === true);
}

/**
 * 根据应用类型生成稳定的预设条目列表。
 */
export function getPresetEntriesByApp(appId: AppId): ProviderPresetEntry[] {
  if (appId === "codex") {
    return toPresetEntries("codex", codexProviderPresets);
  }
  if (appId === "gemini") {
    return toPresetEntries("gemini", geminiProviderPresets);
  }
  if (appId === "opencode") {
    return toPresetEntries("opencode", opencodeProviderPresets);
  }
  if (appId === "openclaw") {
    return toPresetEntries("openclaw", openclawProviderPresets);
  }

  return toPresetEntries("claude", providerPresets.filter(isVisiblePreset));
}

/**
 * 按预设分类聚合条目，缺省分类归入 others。
 */
export function groupPresetEntries(
  presetEntries: ProviderPresetEntry[],
): Record<string, ProviderPresetEntry[]> {
  return presetEntries.reduce<Record<string, ProviderPresetEntry[]>>(
    (acc, entry) => {
      const category = entry.preset.category ?? "others";
      if (!acc[category]) {
        acc[category] = [];
      }
      acc[category].push(entry);
      return acc;
    },
    {},
  );
}

/**
 * 返回需要在预设选择器中渲染的分类 key。
 */
export function getPresetCategoryKeys(
  groupedPresets: Record<string, ProviderPresetEntry[]>,
): string[] {
  return Object.keys(groupedPresets).filter(
    (key) => key !== "custom" && groupedPresets[key]?.length,
  );
}

/**
 * 构建预设分类的本地化标签。
 */
export function getPresetCategoryLabels(t: TFunction): Record<string, string> {
  return {
    official: t("providerForm.categoryOfficial", {
      defaultValue: "官方",
    }),
    cn_official: t("providerForm.categoryCnOfficial", {
      defaultValue: "国内官方",
    }),
    aggregator: t("providerForm.categoryAggregation", {
      defaultValue: "聚合服务",
    }),
    third_party: t("providerForm.categoryThirdParty", {
      defaultValue: "第三方",
    }),
    omo: "OMO",
  };
}
