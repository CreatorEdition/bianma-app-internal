import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { TFunction } from "i18next";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { omoApi, omoSlimApi } from "@/lib/api/omo";
import { useProviderOmoActions } from "@/hooks/useProviderOmoActions";

let queryClient: QueryClient;
let disableOmoSpy: any;
let disableOmoSlimSpy: any;
let toastSuccessSpy: any;
let toastErrorSpy: any;

const t = ((key: string, options?: Record<string, unknown>) => {
  if (key === "omo.disabled") {
    return String(options?.defaultValue ?? key);
  }
  if (key === "omo.disableFailed") {
    return String(options?.defaultValue ?? key).replace(
      "{{error}}",
      String(options?.error ?? ""),
    );
  }
  return key;
}) as TFunction;

function createWrapper() {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );

  return wrapper;
}

beforeEach(() => {
  queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  disableOmoSpy = vi.spyOn(omoApi, "disableCurrentOmo");
  disableOmoSlimSpy = vi.spyOn(omoSlimApi, "disableCurrent");
  toastSuccessSpy = vi.spyOn(toast, "success");
  toastSuccessSpy.mockImplementation(() => "" as never);
  toastErrorSpy = vi.spyOn(toast, "error");
  toastErrorSpy.mockImplementation(() => "" as never);
});

afterEach(() => {
  disableOmoSpy.mockRestore();
  disableOmoSlimSpy.mockRestore();
  toastSuccessSpy.mockRestore();
  toastErrorSpy.mockRestore();
});

describe("useProviderOmoActions", () => {
  it("disables current OMO provider and shows success toast", async () => {
    disableOmoSpy.mockResolvedValueOnce(undefined);

    const { result } = renderHook(() => useProviderOmoActions({ t }), {
      wrapper: createWrapper(),
    });

    act(() => {
      result.current.handleDisableOmo();
    });

    await waitFor(() => {
      expect(disableOmoSpy).toHaveBeenCalledTimes(1);
      expect(toastSuccessSpy).toHaveBeenCalledWith("OMO 已停用");
    });
    expect(disableOmoSlimSpy).not.toHaveBeenCalled();
    expect(toastErrorSpy).not.toHaveBeenCalled();
  });

  it("disables current OMO Slim provider and shows success toast", async () => {
    disableOmoSlimSpy.mockResolvedValueOnce(undefined);

    const { result } = renderHook(() => useProviderOmoActions({ t }), {
      wrapper: createWrapper(),
    });

    act(() => {
      result.current.handleDisableOmoSlim();
    });

    await waitFor(() => {
      expect(disableOmoSlimSpy).toHaveBeenCalledTimes(1);
      expect(toastSuccessSpy).toHaveBeenCalledWith("OMO 已停用");
    });
    expect(disableOmoSpy).not.toHaveBeenCalled();
    expect(toastErrorSpy).not.toHaveBeenCalled();
  });

  it("shows translated error toast when disabling OMO fails", async () => {
    disableOmoSpy.mockRejectedValueOnce(new Error("broken omo"));

    const { result } = renderHook(() => useProviderOmoActions({ t }), {
      wrapper: createWrapper(),
    });

    act(() => {
      result.current.handleDisableOmo();
    });

    await waitFor(() => {
      expect(toastErrorSpy).toHaveBeenCalledWith("停用 OMO 失败: broken omo");
    });
    expect(toastSuccessSpy).not.toHaveBeenCalled();
  });
});
