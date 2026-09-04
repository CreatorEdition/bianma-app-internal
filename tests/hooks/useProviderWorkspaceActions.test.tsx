import { renderHook, act } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { useProviderWorkspaceActions } from "@/hooks/useProviderWorkspaceActions";
import type { Provider } from "@/types";

const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));

const removeFromLiveConfigMock = vi.fn();
const updateSortOrderMock = vi.fn();
const updateTrayMenuMock = vi.fn();
const getOpenCodeLiveProviderIdsMock = vi.fn();
const getOpenClawLiveProviderIdsMock = vi.fn();
const openExternalMock = vi.fn();
const pickDirectoryMock = vi.fn();
const openTerminalMock = vi.fn();

vi.mock("@/lib/api", () => ({
  providersApi: {
    removeFromLiveConfig: (...args: unknown[]) =>
      removeFromLiveConfigMock(...args),
    updateSortOrder: (...args: unknown[]) => updateSortOrderMock(...args),
    updateTrayMenu: (...args: unknown[]) => updateTrayMenuMock(...args),
    getOpenCodeLiveProviderIds: (...args: unknown[]) =>
      getOpenCodeLiveProviderIdsMock(...args),
    getOpenClawLiveProviderIds: (...args: unknown[]) =>
      getOpenClawLiveProviderIdsMock(...args),
    openTerminal: (...args: unknown[]) => openTerminalMock(...args),
  },
  settingsApi: {
    openExternal: (...args: unknown[]) => openExternalMock(...args),
    pickDirectory: (...args: unknown[]) => pickDirectoryMock(...args),
  },
}));

vi.mock("@/hooks/useOpenClaw", () => ({
  openclawKeys: {
    liveProviderIds: ["openclaw", "live-provider-ids"],
    health: ["openclaw", "health"],
  },
}));

function createProvider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: "provider-1",
    name: "Provider 1",
    settingsConfig: { token: "abc" },
    category: "custom",
    ...overrides,
  };
}

function createQueryClientMock() {
  return {
    ensureQueryData: vi.fn(
      async (options?: { queryFn?: () => Promise<unknown> }) => {
        if (options?.queryFn) {
          return await options.queryFn();
        }
        return [];
      },
    ),
    invalidateQueries: vi.fn().mockResolvedValue(undefined),
    refetchQueries: vi.fn().mockResolvedValue(undefined),
  };
}

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

const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

beforeAll(() => {
  consoleErrorSpy.mockImplementation(() => {});
});

afterAll(() => {
  consoleErrorSpy.mockRestore();
});

beforeEach(() => {
  removeFromLiveConfigMock.mockReset();
  updateSortOrderMock.mockReset();
  updateTrayMenuMock.mockReset();
  getOpenCodeLiveProviderIdsMock.mockReset();
  getOpenClawLiveProviderIdsMock.mockReset();
  openExternalMock.mockReset();
  pickDirectoryMock.mockReset();
  openTerminalMock.mockReset();
  toastSuccessMock.mockReset();
  toastErrorMock.mockReset();
  consoleErrorSpy.mockClear();
  getOpenCodeLiveProviderIdsMock.mockResolvedValue([]);
  getOpenClawLiveProviderIdsMock.mockResolvedValue([]);
});

describe("useProviderWorkspaceActions", () => {
  it("removes provider from live config and clears confirm state", async () => {
    removeFromLiveConfigMock.mockResolvedValueOnce(undefined);
    const queryClientMock = createQueryClientMock();
    const clearConfirmAction = vi.fn();
    const confirmAction = {
      provider: createProvider({ id: "opencode-provider" }),
      action: "remove" as const,
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "opencode",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: queryClientMock as unknown as QueryClient,
        t,
        confirmAction,
        clearConfirmAction,
      }),
    );

    await act(async () => {
      await result.current.handleConfirmAction();
    });

    expect(removeFromLiveConfigMock).toHaveBeenCalledWith(
      "opencode-provider",
      "opencode",
    );
    expect(queryClientMock.invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["opencodeLiveProviderIds"],
    });
    expect(clearConfirmAction).toHaveBeenCalledTimes(1);
    expect(toastSuccessMock).toHaveBeenCalledTimes(1);
  });

  it("deletes provider when confirm action is delete", async () => {
    const deleteProvider = vi.fn().mockResolvedValue(undefined);
    const clearConfirmAction = vi.fn();
    const confirmAction = {
      provider: createProvider({ id: "provider-delete" }),
      action: "delete" as const,
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider,
        refetchProviders: vi.fn(),
        queryClient: createQueryClientMock() as unknown as QueryClient,
        t,
        confirmAction,
        clearConfirmAction,
      }),
    );

    await act(async () => {
      await result.current.handleConfirmAction();
    });

    expect(deleteProvider).toHaveBeenCalledWith("provider-delete");
    expect(clearConfirmAction).toHaveBeenCalledTimes(1);
  });

  it("shows an error and clears confirm state when removing from live config fails", async () => {
    removeFromLiveConfigMock.mockRejectedValueOnce(new Error("remove failed"));
    const queryClientMock = createQueryClientMock();
    const clearConfirmAction = vi.fn();
    const confirmAction = {
      provider: createProvider({ id: "provider-remove-failed" }),
      action: "remove" as const,
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: queryClientMock as unknown as QueryClient,
        t,
        confirmAction,
        clearConfirmAction,
      }),
    );

    await act(async () => {
      await result.current.handleConfirmAction();
    });

    expect(toastErrorMock).toHaveBeenCalledWith(
      "从配置移除失败: remove failed",
    );
    expect(clearConfirmAction).toHaveBeenCalledTimes(1);
  });

  it("shows an error and clears confirm state when deleting a provider fails", async () => {
    const deleteProvider = vi
      .fn()
      .mockRejectedValueOnce(new Error("delete failed"));
    const clearConfirmAction = vi.fn();
    const confirmAction = {
      provider: createProvider({ id: "provider-delete-failed" }),
      action: "delete" as const,
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider,
        refetchProviders: vi.fn(),
        queryClient: createQueryClientMock() as unknown as QueryClient,
        t,
        confirmAction,
        clearConfirmAction,
      }),
    );

    await act(async () => {
      await result.current.handleConfirmAction();
    });

    expect(toastErrorMock).toHaveBeenCalledWith(
      "删除供应商失败: delete failed",
    );
    expect(clearConfirmAction).toHaveBeenCalledTimes(1);
  });

  it("stops duplication when sort order update fails", async () => {
    updateSortOrderMock.mockRejectedValueOnce(new Error("sort failed"));
    const addProvider = vi.fn().mockResolvedValue(undefined);
    const original = createProvider({
      id: "provider-a",
      sortIndex: 2,
    });
    const providers = {
      "provider-a": original,
      "provider-b": createProvider({ id: "provider-b", sortIndex: 3 }),
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers,
        addProvider,
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: createQueryClientMock() as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleDuplicateProvider(original);
    });

    expect(updateSortOrderMock).toHaveBeenCalledTimes(1);
    expect(addProvider).not.toHaveBeenCalled();
    expect(toastErrorMock).toHaveBeenCalledWith("排序更新失败");
  });

  it("generates unique provider key when duplicating opencode provider", async () => {
    const addProvider = vi.fn().mockResolvedValue(undefined);
    const queryClientMock = createQueryClientMock();
    const original = createProvider({
      id: "provider-a",
      sortIndex: undefined,
    });
    const providers = {
      "provider-a": original,
      "provider-a-copy": createProvider({ id: "provider-a-copy" }),
      "provider-a-copy-2": createProvider({ id: "provider-a-copy-2" }),
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "opencode",
        providers,
        addProvider,
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: queryClientMock as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleDuplicateProvider(original);
    });

    expect(addProvider).toHaveBeenCalledTimes(1);
    expect(addProvider.mock.calls[0]?.[0]).toMatchObject({
      name: "Provider 1 copy",
      providerKey: "provider-a-copy-3",
      addToLive: false,
    });
    expect(queryClientMock.ensureQueryData).toHaveBeenCalledWith({
      queryKey: ["opencodeLiveProviderIds"],
      queryFn: expect.any(Function),
    });
    expect(getOpenCodeLiveProviderIdsMock).toHaveBeenCalledTimes(1);
  });

  it("includes openclaw live provider IDs when generating duplicated key", async () => {
    const addProvider = vi.fn().mockResolvedValue(undefined);
    const queryClientMock = createQueryClientMock();
    getOpenClawLiveProviderIdsMock.mockResolvedValueOnce(["provider-a-copy"]);

    const original = createProvider({
      id: "provider-a",
      sortIndex: undefined,
    });
    const providers = {
      "provider-a": original,
      "provider-a-copy-2": createProvider({ id: "provider-a-copy-2" }),
    };

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "openclaw",
        providers,
        addProvider,
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: queryClientMock as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleDuplicateProvider(original);
    });

    expect(addProvider).toHaveBeenCalledTimes(1);
    expect(addProvider.mock.calls[0]?.[0]).toMatchObject({
      providerKey: "provider-a-copy-3",
      addToLive: false,
    });
    expect(queryClientMock.ensureQueryData).toHaveBeenCalledWith({
      queryKey: ["openclaw", "live-provider-ids"],
      queryFn: expect.any(Function),
    });
    expect(getOpenClawLiveProviderIdsMock).toHaveBeenCalledTimes(1);
  });

  it("aborts duplication when loading live provider ids fails", async () => {
    const addProvider = vi.fn().mockResolvedValue(undefined);
    const queryClientMock = createQueryClientMock();
    queryClientMock.ensureQueryData.mockRejectedValueOnce(
      new Error("load failed"),
    );
    const original = createProvider({
      id: "provider-a",
      sortIndex: undefined,
    });

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "opencode",
        providers: {
          "provider-a": original,
        },
        addProvider,
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: queryClientMock as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleDuplicateProvider(original);
    });

    expect(addProvider).not.toHaveBeenCalled();
    expect(toastErrorMock).toHaveBeenCalledTimes(1);
    expect(toastErrorMock.mock.calls[0]?.[0]).toContain(
      "读取配置中的供应商标识失败，请先修复配置后再试",
    );
  });

  it("falls back to refetch when provider cache refresh fails after import", async () => {
    const queryClientMock = createQueryClientMock();
    const refetchProviders = vi.fn().mockResolvedValue(undefined);
    queryClientMock.invalidateQueries.mockRejectedValueOnce(
      new Error("invalidate failed"),
    );
    updateTrayMenuMock.mockResolvedValueOnce(undefined);

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider: vi.fn(),
        refetchProviders,
        queryClient: queryClientMock as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleImportSuccess();
    });

    expect(queryClientMock.invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["providers"],
      refetchType: "all",
    });
    expect(queryClientMock.refetchQueries).not.toHaveBeenCalled();
    expect(refetchProviders).toHaveBeenCalledTimes(1);
    expect(updateTrayMenuMock).toHaveBeenCalledTimes(1);
  });

  it("shows readable error when opening provider website fails", async () => {
    openExternalMock.mockRejectedValueOnce(new Error("blocked"));

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: createQueryClientMock() as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleOpenWebsite("https://example.com");
    });

    expect(openExternalMock).toHaveBeenCalledWith("https://example.com");
    expect(toastErrorMock).toHaveBeenCalledWith("blocked");
  });

  it("opens terminal from selected directory", async () => {
    pickDirectoryMock.mockResolvedValueOnce("C:\\project");
    openTerminalMock.mockResolvedValueOnce(undefined);
    const provider = createProvider({ id: "provider-terminal" });

    const { result } = renderHook(() =>
      useProviderWorkspaceActions({
        activeApp: "claude",
        providers: {},
        addProvider: vi.fn(),
        deleteProvider: vi.fn(),
        refetchProviders: vi.fn(),
        queryClient: createQueryClientMock() as unknown as QueryClient,
        t,
        confirmAction: null,
        clearConfirmAction: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.handleOpenTerminal(provider);
    });

    expect(openTerminalMock).toHaveBeenCalledWith(
      "provider-terminal",
      "claude",
      {
        cwd: "C:\\project",
      },
    );
    expect(toastSuccessMock).toHaveBeenCalledWith("终端已打开");
  });
});
