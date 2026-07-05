import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { useSettings } from "@/hooks/useSettings";
import type { Settings } from "@/types";

const mutateAsyncMock = vi.fn();
const useSettingsQueryMock = vi.fn();
const refetchSettingsMock = vi.fn();
const setAppConfigDirOverrideMock = vi.fn();
const applyClaudePluginConfigMock = vi.fn();
const applyClaudeOnboardingSkipMock = vi.fn();
const clearClaudeOnboardingSkipMock = vi.fn();
const updateTrayMenuMock = vi.fn();
const toastErrorMock = vi.fn();
const toastSuccessMock = vi.fn();

let settingsFormMock: any;
let directorySettingsMock: any;
let metadataMock: any;
let serverSettings: Settings;

vi.mock("sonner", () => ({
  toast: {
    error: (...args: unknown[]) => toastErrorMock(...args),
    success: (...args: unknown[]) => toastSuccessMock(...args),
  },
}));

vi.mock("@/hooks/useSettingsForm", () => ({
  useSettingsForm: () => settingsFormMock,
}));

vi.mock("@/hooks/useDirectorySettings", () => ({
  useDirectorySettings: () => directorySettingsMock,
}));

vi.mock("@/hooks/useSettingsMetadata", () => ({
  useSettingsMetadata: () => metadataMock,
}));

vi.mock("@/lib/query", () => ({
  useSettingsQuery: (...args: unknown[]) => useSettingsQueryMock(...args),
  useSaveSettingsMutation: () => ({
    mutateAsync: mutateAsyncMock,
    isPending: false,
  }),
}));

vi.mock("@/lib/api", () => ({
  settingsApi: {
    setAppConfigDirOverride: (...args: unknown[]) =>
      setAppConfigDirOverrideMock(...args),
    applyClaudePluginConfig: (...args: unknown[]) =>
      applyClaudePluginConfigMock(...args),
    applyClaudeOnboardingSkip: (...args: unknown[]) =>
      applyClaudeOnboardingSkipMock(...args),
    clearClaudeOnboardingSkip: (...args: unknown[]) =>
      clearClaudeOnboardingSkipMock(...args),
  },
  providersApi: {
    updateTrayMenu: (...args: unknown[]) => updateTrayMenuMock(...args),
  },
}));

const createSettingsFormMock = (overrides: Record<string, unknown> = {}) => ({
  settings: {
    showInTray: true,
    minimizeToTrayOnClose: true,
    enableClaudePluginIntegration: false,
    skipClaudeOnboarding: true,
    claudeConfigDir: "/claude",
    codexConfigDir: "/codex",
    language: "zh",
  },
  isLoading: false,
  initialLanguage: "zh",
  updateSettings: vi.fn(),
  resetSettings: vi.fn(),
  syncLanguage: vi.fn(),
  ...overrides,
});

const createDirectorySettingsMock = (
  overrides: Record<string, unknown> = {},
) => ({
  appConfigDir: undefined,
  resolvedDirs: {
    appConfig: "/home/mock/.cc-switch",
    claude: "/default/claude",
    codex: "/default/codex",
  },
  isLoading: false,
  initialAppConfigDir: undefined,
  updateDirectory: vi.fn(),
  updateAppConfigDir: vi.fn(),
  browseDirectory: vi.fn(),
  browseAppConfigDir: vi.fn(),
  resetDirectory: vi.fn(),
  resetAppConfigDir: vi.fn(),
  resetAllDirectories: vi.fn(),
  ...overrides,
});

const createMetadataMock = (overrides: Record<string, unknown> = {}) => ({
  isPortable: false,
  requiresRestart: false,
  isLoading: false,
  acknowledgeRestart: vi.fn(),
  setRequiresRestart: vi.fn(),
  ...overrides,
});

describe("useSettings hook", () => {
  beforeEach(() => {
    mutateAsyncMock.mockReset();
    useSettingsQueryMock.mockReset();
    refetchSettingsMock.mockReset();
    setAppConfigDirOverrideMock.mockReset();
    applyClaudePluginConfigMock.mockReset();
    applyClaudeOnboardingSkipMock.mockReset();
    clearClaudeOnboardingSkipMock.mockReset();
    updateTrayMenuMock.mockReset();
    toastErrorMock.mockReset();
    toastSuccessMock.mockReset();
    window.localStorage.clear();

    serverSettings = {
      showInTray: true,
      minimizeToTrayOnClose: true,
      enableClaudePluginIntegration: false,
      skipClaudeOnboarding: true,
      claudeConfigDir: "/server/claude",
      codexConfigDir: "/server/codex",
      language: "zh",
    };

    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
      isError: false,
      error: null,
      refetch: refetchSettingsMock,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        language: "zh",
      },
    });
    directorySettingsMock = createDirectorySettingsMock();
    metadataMock = createMetadataMock();

    mutateAsyncMock.mockResolvedValue(true);
    setAppConfigDirOverrideMock.mockResolvedValue(true);
    applyClaudePluginConfigMock.mockResolvedValue(true);
    applyClaudeOnboardingSkipMock.mockResolvedValue(true);
    clearClaudeOnboardingSkipMock.mockResolvedValue(true);
    updateTrayMenuMock.mockResolvedValue(true);
    refetchSettingsMock.mockResolvedValue({});
  });

  it("exposes settings query error state and refetch handler", async () => {
    const queryError = new Error("无法连接桌面运行时");
    useSettingsQueryMock.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: queryError,
      refetch: refetchSettingsMock,
    });
    settingsFormMock = createSettingsFormMock({
      settings: null,
      isLoading: false,
    });

    const { result } = renderHook(() => useSettings());

    expect(result.current.settingsQueryHasData).toBe(false);
    expect(result.current.settingsQueryIsError).toBe(true);
    expect(result.current.settingsQueryError).toBe(queryError);

    await act(async () => {
      await result.current.refetchSettings();
    });

    expect(refetchSettingsMock).toHaveBeenCalledTimes(1);
  });

  it("auto-saves and applies Claude onboarding skip when toggled on", async () => {
    serverSettings = {
      ...serverSettings,
      skipClaudeOnboarding: false,
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        language: "zh",
        skipClaudeOnboarding: false,
      },
    });

    const { result } = renderHook(() => useSettings());

    await act(async () => {
      await result.current.autoSaveSettings({ skipClaudeOnboarding: true });
    });

    expect(applyClaudeOnboardingSkipMock).toHaveBeenCalledTimes(1);
    expect(toastErrorMock).not.toHaveBeenCalled();
  });

  it("auto-saves and clears Claude onboarding skip when toggled off", async () => {
    serverSettings = {
      ...serverSettings,
      skipClaudeOnboarding: true,
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        language: "zh",
        skipClaudeOnboarding: true,
      },
    });

    const { result } = renderHook(() => useSettings());

    await act(async () => {
      await result.current.autoSaveSettings({ skipClaudeOnboarding: false });
    });

    expect(clearClaudeOnboardingSkipMock).toHaveBeenCalledTimes(1);
    expect(toastErrorMock).not.toHaveBeenCalled();
  });

  it("saves settings and flags restart when app config directory changes", async () => {
    serverSettings = {
      ...serverSettings,
      enableClaudePluginIntegration: false,
      claudeConfigDir: "/server/claude",
      codexConfigDir: undefined,
      language: "en",
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        claudeConfigDir: "  /custom/claude  ",
        codexConfigDir: "   ",
        language: "en",
        enableClaudePluginIntegration: true, // 状态从 false 变为 true
      },
      initialLanguage: "en",
    });

    directorySettingsMock = createDirectorySettingsMock({
      appConfigDir: "  /override/app  ",
      initialAppConfigDir: "/previous/app",
    });

    const { result } = renderHook(() => useSettings());

    let saveResult: { requiresRestart: boolean } | null = null;
    await act(async () => {
      saveResult = await result.current.saveSettings();
    });

    expect(saveResult).toEqual({ requiresRestart: true });
    expect(mutateAsyncMock).toHaveBeenCalledTimes(1);
    const payload = mutateAsyncMock.mock.calls[0][0] as Settings;
    expect(payload.claudeConfigDir).toBe("/custom/claude");
    expect(payload.codexConfigDir).toBeUndefined();
    expect(payload.language).toBe("en");
    expect(setAppConfigDirOverrideMock).toHaveBeenCalledWith("/override/app");
    // 状态改变，应该调用 API
    expect(applyClaudePluginConfigMock).toHaveBeenCalledWith({
      official: false,
    });
    expect(metadataMock.setRequiresRestart).toHaveBeenCalledWith(true);
    expect(window.localStorage.getItem("language")).toBe("en");
    expect(toastErrorMock).not.toHaveBeenCalled();
    expect(updateTrayMenuMock).toHaveBeenCalledTimes(1);
  });

  it("saves settings without restart when directory unchanged", async () => {
    // 确保服务器和本地状态一致，不触发 API 调用
    serverSettings = {
      ...serverSettings,
      enableClaudePluginIntegration: false,
      launchOnStartup: false,
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        enableClaudePluginIntegration: false, // 状态未变
        launchOnStartup: false, // 状态未变
        language: "zh",
      },
      initialLanguage: "zh",
    });

    directorySettingsMock = createDirectorySettingsMock({
      appConfigDir: undefined,
      initialAppConfigDir: undefined,
    });

    const { result } = renderHook(() => useSettings());

    let saveResult: { requiresRestart: boolean } | null = null;
    await act(async () => {
      saveResult = await result.current.saveSettings();
    });

    expect(saveResult).toEqual({ requiresRestart: false });
    expect(setAppConfigDirOverrideMock).toHaveBeenCalledWith(null);
    // 状态未改变，不应调用 API
    expect(applyClaudePluginConfigMock).not.toHaveBeenCalled();
    expect(metadataMock.setRequiresRestart).toHaveBeenCalledWith(false);
  });

  it("saves provider directory overrides without unrelated write side effects", async () => {
    serverSettings = {
      ...serverSettings,
      enableClaudePluginIntegration: false,
      skipClaudeOnboarding: true,
      claudeConfigDir: "/server/claude",
      codexConfigDir: "/server/codex",
      geminiConfigDir: "/server/gemini",
      opencodeConfigDir: "/server/opencode",
      language: "zh",
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        claudeConfigDir: "  /custom/claude  ",
        codexConfigDir: "  /custom/codex  ",
        geminiConfigDir: "  /custom/gemini  ",
        opencodeConfigDir: "  /custom/opencode  ",
        enableClaudePluginIntegration: false,
        skipClaudeOnboarding: true,
        language: "zh",
      },
      initialLanguage: "zh",
    });

    directorySettingsMock = createDirectorySettingsMock({
      appConfigDir: undefined,
      initialAppConfigDir: undefined,
    });

    const { result } = renderHook(() => useSettings());

    let saveResult: { requiresRestart: boolean } | null = null;
    await act(async () => {
      saveResult = await result.current.saveSettings();
    });

    expect(saveResult).toEqual({ requiresRestart: false });
    expect(mutateAsyncMock).toHaveBeenCalledTimes(1);
    const payload = mutateAsyncMock.mock.calls[0][0] as Settings;
    expect(payload.claudeConfigDir).toBe("/custom/claude");
    expect(payload.codexConfigDir).toBe("/custom/codex");
    expect(payload.geminiConfigDir).toBe("/custom/gemini");
    expect(payload.opencodeConfigDir).toBe("/custom/opencode");
    expect(setAppConfigDirOverrideMock).toHaveBeenCalledWith(null);
    expect(updateTrayMenuMock).toHaveBeenCalledTimes(1);
    expect(applyClaudePluginConfigMock).not.toHaveBeenCalled();
    expect(applyClaudeOnboardingSkipMock).not.toHaveBeenCalled();
    expect(clearClaudeOnboardingSkipMock).not.toHaveBeenCalled();
    expect(toastErrorMock).not.toHaveBeenCalled();
  });

  it("shows toast when Claude plugin sync fails but continues flow", async () => {
    // 设置服务器状态为 false,本地状态为 true,触发状态变化
    serverSettings = {
      ...serverSettings,
      enableClaudePluginIntegration: false,
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        enableClaudePluginIntegration: true, // 状态改变
        language: "zh",
      },
    });
    directorySettingsMock = createDirectorySettingsMock({
      appConfigDir: "/override/app",
      initialAppConfigDir: "/prior/app",
    });

    applyClaudePluginConfigMock.mockRejectedValueOnce(new Error("sync failed"));

    const { result } = renderHook(() => useSettings());

    await act(async () => {
      await result.current.saveSettings();
    });

    expect(toastErrorMock).toHaveBeenCalled();
    const message = toastErrorMock.mock.calls.at(-1)?.[0] as string;
    expect(message).toContain("同步 Claude 插件失败");
    expect(metadataMock.setRequiresRestart).toHaveBeenCalledWith(true);
  });

  it("resets form, language and directories using server data", () => {
    serverSettings = {
      ...serverSettings,
      claudeConfigDir: "  /server/claude  ",
      codexConfigDir: "   ",
      language: "zh",
    };
    useSettingsQueryMock.mockReturnValue({
      data: serverSettings,
      isLoading: false,
    });

    settingsFormMock = createSettingsFormMock({
      settings: {
        ...serverSettings,
        language: "zh",
      },
      initialLanguage: "zh",
    });
    directorySettingsMock = createDirectorySettingsMock();

    const { result } = renderHook(() => useSettings());

    act(() => {
      result.current.resetSettings();
    });

    expect(settingsFormMock.resetSettings).toHaveBeenCalledWith(serverSettings);
    expect(settingsFormMock.syncLanguage).toHaveBeenCalledWith(
      settingsFormMock.initialLanguage,
    );
    expect(directorySettingsMock.resetAllDirectories).toHaveBeenCalledWith(
      "/server/claude",
      undefined,
      undefined, // geminiConfigDir
      undefined, // opencodeConfigDir
    );
    expect(metadataMock.setRequiresRestart).toHaveBeenCalledWith(false);
  });

  it("returns null immediately when settings state is missing", async () => {
    settingsFormMock = createSettingsFormMock({
      settings: null,
    });

    const { result } = renderHook(() => useSettings());

    let resultValue: { requiresRestart: boolean } | null = null;
    await act(async () => {
      resultValue = await result.current.saveSettings();
    });

    expect(resultValue).toBeNull();
    expect(mutateAsyncMock).not.toHaveBeenCalled();
    expect(setAppConfigDirOverrideMock).not.toHaveBeenCalled();
  });

  it("throws when save mutation rejects and keeps restart flag untouched", async () => {
    settingsFormMock = createSettingsFormMock();
    directorySettingsMock = createDirectorySettingsMock({
      appConfigDir: "/override/app",
      initialAppConfigDir: "/override/app",
    });
    const rejection = new Error("save failed");
    mutateAsyncMock.mockRejectedValueOnce(rejection);

    const { result } = renderHook(() => useSettings());

    await expect(
      act(async () => {
        await result.current.saveSettings();
      }),
    ).rejects.toThrow("save failed");

    expect(setAppConfigDirOverrideMock).not.toHaveBeenCalled();
    expect(metadataMock.setRequiresRestart).not.toHaveBeenCalledWith(true);
  });
});
