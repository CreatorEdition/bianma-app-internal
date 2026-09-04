import type { ProxyStatus, ProxyTakeoverStatus } from "@/types/proxy";

const proxyStatus: ProxyStatus = {
  running: false,
  address: "127.0.0.1",
  port: 15721,
  active_connections: 0,
  total_requests: 0,
  success_requests: 0,
  failed_requests: 0,
  success_rate: 0,
  uptime_seconds: 0,
  current_provider: null,
  current_provider_id: null,
  last_request_at: null,
  last_error: null,
  failover_count: 0,
  active_targets: [],
};

const takeoverStatus: ProxyTakeoverStatus = {
  claude: false,
  codex: false,
  gemini: false,
  opencode: false,
  openclaw: false,
};

/** 仅为纯 Vite 预览提供只读空状态，生产与 Tauri 构建不会启用。 */
export function installRendererMock() {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return;

  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {
      invoke: async (command: string) => {
        switch (command) {
          case "get_proxy_status":
            return proxyStatus;
          case "get_proxy_takeover_status":
            return takeoverStatus;
          case "get_universal_providers":
            return {};
          case "get_settings":
            return {
              visibleApps: {
                claude: true,
                codex: true,
                gemini: true,
                opencode: true,
                openclaw: true,
              },
            };
          case "get_usage_summary":
            return {
              totalRequests: 0,
              totalCost: "0",
              totalInputTokens: 0,
              totalOutputTokens: 0,
              totalCacheCreationTokens: 0,
              totalCacheReadTokens: 0,
              successRate: 0,
            };
          case "get_usage_trends":
          case "get_provider_stats":
          case "get_model_stats":
          case "get_model_pricing":
            return [];
          case "get_request_logs":
            return { data: [], total: 0, page: 0, pageSize: 20 };
          default:
            throw new Error(`纯 Vite 预览未模拟命令：${command}`);
        }
      },
      transformCallback: () => 0,
      unregisterCallback: () => undefined,
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { windowLabel: "main", label: "main" },
      },
    },
  });
}
