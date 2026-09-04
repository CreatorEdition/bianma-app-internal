import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TFunction } from "i18next";
import { toast } from "sonner";
import type { AppId } from "@/lib/api";
import { providersApi } from "@/lib/api/providers";
import { useAppEventSubscriptions } from "@/hooks/useAppEventSubscriptions";
import { emitTauriEvent } from "../msw/tauriMocks";

const refetchProvidersMock = vi.fn();
let queryClient: QueryClient;
let invalidateQueriesSpy: any;
let updateTrayMenuSpy: any;
let toastErrorSpy: any;
let consoleErrorSpy: any;

const t = ((key: string, options?: Record<string, unknown>) => {
  if (key === "settings.webdavSync.autoSyncFailedToast") {
    return `自动同步失败: ${options?.error}`;
  }
  if (key === "common.unknown") {
    return "未知错误";
  }
  return key;
}) as TFunction;

const flushEffects = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

const renderUseAppEventSubscriptions = (activeApp: AppId = "codex") =>
  renderHook(() =>
    useAppEventSubscriptions({
      activeApp,
      refetchProviders: refetchProvidersMock,
      queryClient,
      t,
    }),
  );

beforeEach(() => {
  queryClient = new QueryClient();
  refetchProvidersMock.mockReset();
  refetchProvidersMock.mockResolvedValue(undefined);
  invalidateQueriesSpy = vi.spyOn(queryClient, "invalidateQueries");
  updateTrayMenuSpy = vi.spyOn(providersApi, "updateTrayMenu");
  updateTrayMenuSpy.mockResolvedValue(true);
  toastErrorSpy = vi.spyOn(toast, "error");
  toastErrorSpy.mockImplementation(() => "" as never);
  consoleErrorSpy = vi.spyOn(console, "error");
  consoleErrorSpy.mockImplementation(() => {});
});

afterEach(() => {
  invalidateQueriesSpy.mockRestore();
  updateTrayMenuSpy.mockRestore();
  toastErrorSpy.mockRestore();
  consoleErrorSpy.mockRestore();
});

describe("useAppEventSubscriptions", () => {
  it("only refetches providers when provider-switched matches the active app", async () => {
    renderUseAppEventSubscriptions("codex");
    await flushEffects();

    emitTauriEvent("provider-switched", {
      appType: "claude",
      providerId: "claude-1",
    });

    expect(refetchProvidersMock).not.toHaveBeenCalled();

    emitTauriEvent("provider-switched", {
      appType: "codex",
      providerId: "codex-1",
    });

    await waitFor(() => {
      expect(refetchProvidersMock).toHaveBeenCalledTimes(1);
    });
  });

  it("invalidates providers and updates tray menu after universal provider sync", async () => {
    renderUseAppEventSubscriptions();
    await flushEffects();

    emitTauriEvent("universal-provider-synced", {
      id: "provider-1",
      appType: "codex",
    });

    await waitFor(() => {
      expect(invalidateQueriesSpy).toHaveBeenCalledWith({
        queryKey: ["providers"],
      });
      expect(updateTrayMenuSpy).toHaveBeenCalledTimes(1);
    });
  });

  it("logs tray menu update failures without blocking provider invalidation", async () => {
    updateTrayMenuSpy.mockRejectedValueOnce(new Error("tray failed"));

    renderUseAppEventSubscriptions();
    await flushEffects();

    emitTauriEvent("universal-provider-synced", {});

    await waitFor(() => {
      expect(invalidateQueriesSpy).toHaveBeenCalledWith({
        queryKey: ["providers"],
      });
      expect(consoleErrorSpy).toHaveBeenCalledWith(
        "[App] Failed to update tray menu",
        expect.any(Error),
      );
    });
  });

  it("invalidates settings and shows a toast for background WebDAV sync errors", async () => {
    renderUseAppEventSubscriptions();
    await flushEffects();

    emitTauriEvent("webdav-sync-status-updated", {
      source: "auto",
      status: "error",
      error: "network timeout",
    });

    await waitFor(() => {
      expect(invalidateQueriesSpy).toHaveBeenCalledWith({
        queryKey: ["settings"],
      });
      expect(toastErrorSpy).toHaveBeenCalledWith(
        "自动同步失败: network timeout",
      );
    });
  });
});
