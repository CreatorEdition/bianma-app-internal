import type { UniversalProvider } from "@/types";

/** 默认统一路由只接受可同时覆盖全部自动接入客户端的上游。 */
export function isUniversalRouteProvider(provider: UniversalProvider): boolean {
  if (!provider.baseUrl.trim() || !provider.apiKey.trim()) {
    return false;
  }

  return (["claude", "codex", "gemini"] as const).every(
    (appType) =>
      provider.apps[appType] &&
      Boolean(provider.models[appType]?.model?.trim()),
  );
}
