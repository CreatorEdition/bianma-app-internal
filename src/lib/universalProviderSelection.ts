import type { UniversalProvider, UniversalProvidersMap } from "@/types";
import { isUniversalRouteProvider } from "@/lib/universalProvider";

export const UNIVERSAL_ACTIVE_PROVIDER_STORAGE_KEY =
  "bianma-universal-active-provider";

const byStableOrder = (left: UniversalProvider, right: UniversalProvider) => {
  const leftSort = left.sortIndex ?? Number.MAX_SAFE_INTEGER;
  const rightSort = right.sortIndex ?? Number.MAX_SAFE_INTEGER;
  if (leftSort !== rightSort) return leftSort - rightSort;

  const leftCreated = left.createdAt ?? Number.MAX_SAFE_INTEGER;
  const rightCreated = right.createdAt ?? Number.MAX_SAFE_INTEGER;
  if (leftCreated !== rightCreated) return leftCreated - rightCreated;

  return left.id.localeCompare(right.id);
};

export function getUniversalRouteProviders(
  providers: UniversalProvidersMap,
): UniversalProvider[] {
  return Object.values(providers)
    .filter(isUniversalRouteProvider)
    .sort(byStableOrder);
}

export function getActiveUniversalProviderId(
  providers: UniversalProvidersMap,
): string | null {
  const routeProviders = getUniversalRouteProviders(providers);
  if (routeProviders.length === 0) return null;

  const savedId =
    typeof window === "undefined"
      ? null
      : window.localStorage.getItem(UNIVERSAL_ACTIVE_PROVIDER_STORAGE_KEY);
  if (savedId && routeProviders.some((provider) => provider.id === savedId)) {
    return savedId;
  }

  return routeProviders[0].id;
}

export function setActiveUniversalProviderId(id: string): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(UNIVERSAL_ACTIVE_PROVIDER_STORAGE_KEY, id);
  window.dispatchEvent(
    new CustomEvent("universal-active-provider-changed", { detail: id }),
  );
}
