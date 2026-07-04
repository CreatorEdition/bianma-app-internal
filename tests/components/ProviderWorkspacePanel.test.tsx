import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProviderWorkspacePanel } from "@/components/providers/ProviderWorkspacePanel";
import type { AppId } from "@/lib/api";
import type { Provider } from "@/types";

const updateProviderMock = vi.fn();
const discoverModelsMock = vi.fn();
const testProvidersLatencyMock = vi.fn();
const getCachedLatencyResultsMock = vi.fn();
const checkProviderMock = vi.fn();
const originalConsoleError = console.error.bind(console);
let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

vi.mock("@/hooks/useDragSort", () => ({
  useDragSort: (providers: Record<string, Provider>) => ({
    sortedProviders: Object.values(providers),
    sensors: [],
    handleDragEnd: vi.fn(),
  }),
}));

vi.mock("@/hooks/useOpenClaw", () => ({
  useOpenClawDefaultModel: () => ({ data: null }),
}));

vi.mock("@/hooks/useStreamCheck", () => ({
  useStreamCheck: () => ({
    checkProvider: checkProviderMock,
    isChecking: () => false,
  }),
}));

vi.mock("@/lib/query/failover", () => ({
  useAutoFailoverEnabled: () => ({ data: false }),
}));

vi.mock("@/lib/api/providers", () => ({
  providersApi: {
    update: (...args: unknown[]) => updateProviderMock(...args),
    discoverModels: (...args: unknown[]) => discoverModelsMock(...args),
    testProvidersLatency: (...args: unknown[]) =>
      testProvidersLatencyMock(...args),
    getCachedLatencyResults: (...args: unknown[]) =>
      getCachedLatencyResultsMock(...args),
  },
}));

vi.mock("@/components/providers/ProviderList", () => ({
  ProviderList: ({ providers }: { providers: Record<string, Provider> }) => (
    <div data-testid="service-actions-provider-id">
      {Object.keys(providers)[0] ?? "none"}
    </div>
  ),
}));

const createProvider = (
  id: string,
  name: string,
  overrides?: Partial<Provider>,
): Provider => ({
  id,
  name,
  category: "custom",
  settingsConfig: {
    env: {
      ANTHROPIC_BASE_URL: `https://${id}.example.com`,
      ANTHROPIC_AUTH_TOKEN: `sk-${id}`,
    },
  },
  ...overrides,
});

const baseProviders: Record<string, Provider> = {
  p1: createProvider("p1", "Alpha"),
  p2: createProvider("p2", "Bravo", {
    meta: {
      usage_script: { enabled: true },
      favoriteProvider: true,
      favoriteModelsByApp: {
        claude: ["claude-sonnet-4-20250514"],
      },
    } as Provider["meta"],
  }),
  p3: createProvider("p3", "Charlie"),
};

const getStorageKey = (appId: AppId, suffix: "sort" | "selected-provider") =>
  `bianma-model-workspace-${appId}-${suffix}`;

const getLegacyStorageKey = (
  appId: AppId,
  suffix: "sort" | "selected-provider",
) => `cc-switch-model-workspace-${appId}-${suffix}`;

function renderPanel(
  overrides?: Partial<ComponentProps<typeof ProviderWorkspacePanel>>,
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <QueryClientProvider client={client}>
      <ProviderWorkspacePanel
        activeApp="claude"
        providers={baseProviders}
        currentProviderId="p1"
        isLoading={false}
        isProxyRunning={false}
        isCurrentAppTakeoverActive={false}
        onSwitch={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
        onConfigureUsage={vi.fn()}
        onOpenWebsite={vi.fn()}
        onCreate={vi.fn()}
        onOpenProxySettings={vi.fn()}
        {...overrides}
      />
    </QueryClientProvider>,
  );
}

describe("ProviderWorkspacePanel", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
    consoleErrorSpy = vi
      .spyOn(console, "error")
      .mockImplementation((...args) => {
        const message = args
          .map((value) =>
            typeof value === "string"
              ? value
              : value instanceof Error
                ? value.message
                : "",
          )
          .join(" ");

        if (message.includes("not wrapped in act")) {
          return;
        }

        originalConsoleError(...args);
      });
    discoverModelsMock.mockResolvedValue([
      {
        id: "claude-sonnet-4-20250514",
        name: "Claude Sonnet 4",
      },
      {
        id: "claude-opus-4-1-20250805",
        name: "Claude Opus 4.1",
      },
    ]);
    getCachedLatencyResultsMock.mockResolvedValue([]);
    testProvidersLatencyMock.mockResolvedValue({
      results: [],
      total: 0,
      success: 0,
      failed: 0,
    });
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockImplementation(() => ({
        matches: false,
        media: "",
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    );
  });

  afterEach(() => {
    consoleErrorSpy.mockRestore();
    vi.unstubAllGlobals();
  });

  it("uses active provider as default selected item", () => {
    renderPanel({
      activeProviderId: "p2",
      isCurrentAppTakeoverActive: true,
    });

    expect(screen.getByTestId("detail-provider-id")).toHaveTextContent("p2");
  });

  it("migrates legacy selected provider storage", () => {
    window.localStorage.setItem(
      getLegacyStorageKey("claude", "selected-provider"),
      "p3",
    );

    renderPanel();

    expect(screen.getByTestId("detail-provider-id")).toHaveTextContent("p3");
    expect(
      window.localStorage.getItem(getStorageKey("claude", "selected-provider")),
    ).toBe("p3");
    expect(
      window.localStorage.getItem(
        getLegacyStorageKey("claude", "selected-provider"),
      ),
    ).toBeNull();
  });

  it("persists sort strategy and selected provider", async () => {
    renderPanel();

    fireEvent.change(screen.getByTestId("provider-sort-select"), {
      target: { value: "activeFirst" },
    });
    fireEvent.click(screen.getByTestId("service-row-p2"));

    await waitFor(() => {
      expect(window.localStorage.getItem(getStorageKey("claude", "sort"))).toBe(
        "activeFirst",
      );
      expect(
        window.localStorage.getItem(
          getStorageKey("claude", "selected-provider"),
        ),
      ).toBe("p2");
      expect(
        window.localStorage.getItem(getLegacyStorageKey("claude", "sort")),
      ).toBeNull();
    });
  });

  it("focuses service search input on Ctrl+F", () => {
    renderPanel();

    fireEvent.keyDown(window, { key: "f", ctrlKey: true });

    expect(screen.getByTestId("service-search-input")).toHaveFocus();
  });

  it("supports keyboard navigation and Enter switch", () => {
    const onSwitch = vi.fn();
    renderPanel({ onSwitch });

    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(screen.getByTestId("detail-provider-id")).toHaveTextContent("p2");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(onSwitch).toHaveBeenCalledWith(baseProviders.p2);
  });

  it("does not change selection when arrow keys are pressed in search input", () => {
    renderPanel();

    const searchInput = screen.getByTestId("service-search-input");
    searchInput.focus();
    fireEvent.keyDown(searchInput, { key: "ArrowDown" });

    expect(screen.getByTestId("detail-provider-id")).toHaveTextContent("p1");
  });

  it("switches provider on double click", () => {
    const onSwitch = vi.fn();
    renderPanel({ onSwitch });

    fireEvent.doubleClick(screen.getByTestId("service-row-p2"));

    expect(onSwitch).toHaveBeenCalledWith(baseProviders.p2);
  });

  it("loads discovered models for the selected provider", async () => {
    renderPanel();

    await waitFor(() => {
      expect(discoverModelsMock).toHaveBeenCalled();
      expect(screen.getByText("Claude Sonnet 4")).toBeInTheDocument();
    });
  });

  it("renders single-card ProviderList region", () => {
    renderPanel();

    expect(
      screen.getByTestId("service-detail-actions-provider-list"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("service-actions-provider-id")).toHaveTextContent(
      "p1",
    );
  });
});
