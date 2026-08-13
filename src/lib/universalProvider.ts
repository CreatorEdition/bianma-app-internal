import type { UniversalProvider } from "@/types";

/** 只有具备完整统一路由信息的上游才能解锁本地代理。 */
export function isUsableUniversalProvider(
  provider: UniversalProvider,
): boolean {
  if (!provider.baseUrl.trim() || !provider.apiKey.trim()) {
    return false;
  }

  return (
    (provider.apps.claude && Boolean(provider.models.claude?.model?.trim())) ||
    (provider.apps.codex && Boolean(provider.models.codex?.model?.trim())) ||
    (provider.apps.gemini && Boolean(provider.models.gemini?.model?.trim()))
  );
}
