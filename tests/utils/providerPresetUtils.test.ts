import { describe, expect, it } from "vitest";
import type { AppId } from "@/lib/api";
import {
  getPresetCategoryKeys,
  getPresetCategoryLabels,
  getPresetEntriesByApp,
  groupPresetEntries,
  type ProviderPresetEntry,
} from "@/components/providers/forms/providerPresetUtils";

describe("providerPresetUtils", () => {
  it("按应用生成带稳定前缀的预设条目", () => {
    const appPrefixes: Array<{ appId: AppId; prefix: string }> = [
      { appId: "claude", prefix: "claude-" },
      { appId: "codex", prefix: "codex-" },
      { appId: "gemini", prefix: "gemini-" },
      { appId: "opencode", prefix: "opencode-" },
      { appId: "openclaw", prefix: "openclaw-" },
    ];

    appPrefixes.forEach(({ appId, prefix }) => {
      const entries = getPresetEntriesByApp(appId);
      expect(entries.length).toBeGreaterThan(0);
      expect(entries.every((entry) => entry.id.startsWith(prefix))).toBe(true);
    });
  });

  it("不会返回显式隐藏的 Claude 预设", () => {
    const entries = getPresetEntriesByApp("claude");

    expect(
      entries.some(
        (entry) => "hidden" in entry.preset && entry.preset.hidden === true,
      ),
    ).toBe(false);
  });

  it("按分类聚合预设条目，缺省分类回退到 others", () => {
    const entries = [
      {
        id: "a",
        preset: { category: "official" },
      },
      {
        id: "b",
        preset: {},
      },
      {
        id: "c",
        preset: { category: "official" },
      },
    ] as ProviderPresetEntry[];

    const grouped = groupPresetEntries(entries);

    expect(grouped.official).toHaveLength(2);
    expect(grouped.others).toHaveLength(1);
  });

  it("过滤空分类并排除 custom 分类", () => {
    const categoryKeys = getPresetCategoryKeys({
      official: [{ id: "a", preset: {} as ProviderPresetEntry["preset"] }],
      custom: [{ id: "b", preset: {} as ProviderPresetEntry["preset"] }],
      empty: [],
    });

    expect(categoryKeys).toEqual(["official"]);
  });

  it("构建本地化预设分类标签", () => {
    const t = ((key: string, options?: Record<string, unknown>) => {
      if (options?.defaultValue && typeof options.defaultValue === "string") {
        return options.defaultValue;
      }
      return key;
    }) as Parameters<typeof getPresetCategoryLabels>[0];

    const labels = getPresetCategoryLabels(t);

    expect(labels.official).toBe("官方");
    expect(labels.cn_official).toBe("国内官方");
    expect(labels.aggregator).toBe("聚合服务");
    expect(labels.third_party).toBe("第三方");
    expect(labels.omo).toBe("OMO");
  });
});
