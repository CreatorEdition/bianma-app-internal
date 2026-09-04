import type { ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSetAutoFailoverEnabled } from "@/lib/query/failover";
import { createTestQueryClient } from "../utils/testQueryClient";

const {
  setAutoFailoverEnabledMock,
  toastSuccessMock,
  toastErrorMock,
  translations,
} = vi.hoisted(() => ({
  setAutoFailoverEnabledMock: vi.fn(),
  toastSuccessMock: vi.fn(),
  toastErrorMock: vi.fn(),
  translations: {
    "failover.enabled": "{{app}} 故障转移已启用",
    "failover.disabled": "{{app}} 故障转移已关闭",
    "failover.toggleFailed": "操作失败: {{detail}}",
    "common.unknown": "未知错误",
  } as Record<string, string>,
}));

const formatTranslation = (
  key: string,
  options?: Record<string, unknown>,
): string => {
  const template = translations[key];
  if (!template) {
    return key;
  }

  return template.replace(/\{\{(\w+)\}\}/g, (_match, name: string) =>
    String(options?.[name] ?? ""),
  );
};

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      formatTranslation(key, options),
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));

vi.mock("@/lib/api/failover", () => ({
  failoverApi: {
    setAutoFailoverEnabled: (...args: unknown[]) =>
      setAutoFailoverEnabledMock(...args),
  },
}));

interface WrapperProps {
  children: ReactNode;
}

function createWrapper() {
  const queryClient = createTestQueryClient();

  const wrapper = ({ children }: WrapperProps) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );

  return { wrapper };
}

describe("useSetAutoFailoverEnabled", () => {
  beforeEach(() => {
    setAutoFailoverEnabledMock.mockReset();
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
    setAutoFailoverEnabledMock.mockResolvedValue(undefined);
  });

  it("shows the enabled toast from translation resources", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSetAutoFailoverEnabled(), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        appType: "claude",
        enabled: true,
      });
    });

    await waitFor(() =>
      expect(toastSuccessMock).toHaveBeenCalledWith(
        formatTranslation("failover.enabled", { app: "Claude" }),
        { closeButton: true },
      ),
    );
  });

  it("shows the disabled toast from translation resources", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSetAutoFailoverEnabled(), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        appType: "claude",
        enabled: false,
      });
    });

    await waitFor(() =>
      expect(toastSuccessMock).toHaveBeenCalledWith(
        formatTranslation("failover.disabled", { app: "Claude" }),
        { closeButton: true },
      ),
    );
  });

  it("shows the failure toast from translation resources and error detail", async () => {
    setAutoFailoverEnabledMock.mockRejectedValue(new Error("boom"));
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSetAutoFailoverEnabled(), {
      wrapper,
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync({
          appType: "claude",
          enabled: true,
        });
      }),
    ).rejects.toThrow("boom");

    await waitFor(() =>
      expect(toastErrorMock).toHaveBeenCalledWith(
        formatTranslation("failover.toggleFailed", {
          detail: "boom",
        }),
      ),
    );
  });
});
