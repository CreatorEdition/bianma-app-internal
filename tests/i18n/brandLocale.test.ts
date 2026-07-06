import { describe, expect, it } from "vitest";

import zh from "../../src/i18n/locales/zh.json";
import en from "../../src/i18n/locales/en.json";
import ja from "../../src/i18n/locales/ja.json";

type BrandLocale = {
  confirm: {
    removeProviderMessage: string;
  };
  settings: {
    advanced: {
      globalProxy: {
        description: string;
      };
    };
    importExportHint: string;
    launchOnStartupDescription: string;
    appConfigDir: string;
    appConfigDirDescription: string;
    restartRequiredMessage: string;
  };
  mcp: {
    unifiedPanel: {
      noImportFound: string;
    };
  };
  skills: {
    importDescription: string;
    noUnmanagedFound: string;
  };
};

const locales: Array<{ name: string; messages: BrandLocale }> = [
  { name: "zh", messages: zh },
  { name: "en", messages: en },
  { name: "ja", messages: ja },
];

const targetMessages = [
  [
    "confirm.removeProviderMessage",
    (locale: BrandLocale) => locale.confirm.removeProviderMessage,
  ],
  [
    "settings.advanced.globalProxy.description",
    (locale: BrandLocale) => locale.settings.advanced.globalProxy.description,
  ],
  [
    "settings.launchOnStartupDescription",
    (locale: BrandLocale) => locale.settings.launchOnStartupDescription,
  ],
  [
    "settings.appConfigDir",
    (locale: BrandLocale) => locale.settings.appConfigDir,
  ],
  [
    "settings.appConfigDirDescription",
    (locale: BrandLocale) => locale.settings.appConfigDirDescription,
  ],
  [
    "settings.restartRequiredMessage",
    (locale: BrandLocale) => locale.settings.restartRequiredMessage,
  ],
  [
    "mcp.unifiedPanel.noImportFound",
    (locale: BrandLocale) => locale.mcp.unifiedPanel.noImportFound,
  ],
  [
    "skills.importDescription",
    (locale: BrandLocale) => locale.skills.importDescription,
  ],
  [
    "skills.noUnmanagedFound",
    (locale: BrandLocale) => locale.skills.noUnmanagedFound,
  ],
] as const;

const compatibilityMessages = [
  [
    "settings.importExportHint",
    (locale: BrandLocale) => locale.settings.importExportHint,
  ],
] as const;

describe("brand locale copy", () => {
  it.each(locales)(
    "uses bianma.ai in public UI brand targets for $name",
    ({ messages }) => {
      for (const [key, getMessage] of targetMessages) {
        const message = getMessage(messages);

        expect(message, key).toContain("bianma.ai");
        expect(message, key).not.toContain("CC Switch");
      }
    },
  );

  it.each(locales)(
    "keeps legacy compatibility wording for $name",
    ({ messages }) => {
      for (const [key, getMessage] of compatibilityMessages) {
        const message = getMessage(messages);

        expect(message, key).toContain("bianma.ai");
        expect(message, key).toContain("CC Switch");
        expect(message, key).toMatch(/compatible|legacy|兼容|互換/);
      }
    },
  );
});
