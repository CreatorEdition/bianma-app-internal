import type { ReactNode } from "react";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestDetailPanel } from "@/components/usage/RequestDetailPanel";

const useRequestDetailMock = vi.fn();

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (_key: string, fallback?: string) => fallback ?? _key,
    i18n: {
      language: "zh",
      resolvedLanguage: "zh",
    },
  }),
}));

vi.mock("@/lib/query/usage", () => ({
  useRequestDetail: (requestId: string) => useRequestDetailMock(requestId),
}));

vi.mock("@/components/ui/dialog", () => ({
  Dialog: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  DialogContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  DialogHeader: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
}));

const baseRequest = {
  requestId: "req-1",
  providerId: "provider-1",
  providerName: "Claude Provider",
  appType: "claude",
  model: "claude-3.7-sonnet",
  statusCode: 200,
  costMultiplier: "1.0",
  inputTokens: 1234,
  outputTokens: 5678,
  cacheReadTokens: 90,
  cacheCreationTokens: 12,
  inputCostUsd: "0.001234",
  outputCostUsd: "0.005678",
  cacheReadCostUsd: "0.000090",
  cacheCreationCostUsd: "0.000012",
  totalCostUsd: "0.007014",
  latencyMs: 321,
  createdAt: 1_710_000_000,
  errorMessage: "",
};

describe("RequestDetailPanel", () => {
  beforeEach(() => {
    useRequestDetailMock.mockReset();
  });

  it("renders basic request, usage, cost, and latency details", () => {
    useRequestDetailMock.mockReturnValue({
      data: baseRequest,
      isLoading: false,
    });

    render(<RequestDetailPanel requestId="req-1" onClose={() => {}} />);

    expect(screen.getByText("请求详情")).toBeInTheDocument();
    expect(screen.getByText("req-1")).toBeInTheDocument();
    expect(screen.getByText("Claude Provider")).toBeInTheDocument();
    expect(screen.getByText("provider-1")).toBeInTheDocument();
    expect(screen.getByText("claude")).toBeInTheDocument();
    expect(screen.getByText("claude-3.7-sonnet")).toBeInTheDocument();
    expect(screen.getByText("200")).toBeInTheDocument();
    expect(screen.getByText("1,234")).toBeInTheDocument();
    expect(screen.getByText("5,678")).toBeInTheDocument();
    expect(screen.getByText("90")).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.getByText("6,912")).toBeInTheDocument();
    expect(screen.getByText("$0.007014")).toBeInTheDocument();
    expect(screen.getByText("321ms")).toBeInTheDocument();
  });

  it("renders error message when request failed", () => {
    useRequestDetailMock.mockReturnValue({
      data: {
        ...baseRequest,
        statusCode: 500,
        errorMessage: "upstream failed",
      },
      isLoading: false,
    });

    render(<RequestDetailPanel requestId="req-1" onClose={() => {}} />);

    expect(screen.getByText("500")).toBeInTheDocument();
    expect(screen.getByText("错误信息")).toBeInTheDocument();
    expect(screen.getByText("upstream failed")).toBeInTheDocument();
  });

  it("renders not found state when request detail is missing", () => {
    useRequestDetailMock.mockReturnValue({
      data: null,
      isLoading: false,
    });

    render(<RequestDetailPanel requestId="missing" onClose={() => {}} />);

    expect(screen.getByText("请求详情")).toBeInTheDocument();
    expect(screen.getByText("请求未找到")).toBeInTheDocument();
  });
});
