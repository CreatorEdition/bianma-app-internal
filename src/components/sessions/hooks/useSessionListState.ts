import { useEffect, useMemo, useState } from "react";
import { useSessionSearch } from "@/hooks/useSessionSearch";
import type { SessionMeta } from "@/types";
import { getSessionKey } from "../utils";

const PROVIDER_FILTERS = [
  "all",
  "codex",
  "claude",
  "opencode",
  "openclaw",
  "gemini",
] as const;

export type ProviderFilter = (typeof PROVIDER_FILTERS)[number];

interface UseSessionListStateOptions {
  sessions: SessionMeta[];
  appId: string;
}

const isProviderFilter = (value: string): value is ProviderFilter =>
  PROVIDER_FILTERS.some((providerFilter) => providerFilter === value);

export const resolveProviderFilter = (appId: string): ProviderFilter =>
  isProviderFilter(appId) ? appId : "all";

export function useSessionListState({
  sessions,
  appId,
}: UseSessionListStateOptions) {
  const [search, setSearch] = useState("");
  const [providerFilter, setProviderFilter] = useState<ProviderFilter>(() =>
    resolveProviderFilter(appId),
  );
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  const { search: searchSessions } = useSessionSearch({
    sessions,
    providerFilter,
  });

  const filteredSessions = useMemo(
    () => searchSessions(search),
    [search, searchSessions],
  );

  useEffect(() => {
    if (filteredSessions.length === 0) {
      setSelectedKey(null);
      return;
    }

    const exists = selectedKey
      ? filteredSessions.some(
          (session) => getSessionKey(session) === selectedKey,
        )
      : false;

    if (!exists) {
      setSelectedKey(getSessionKey(filteredSessions[0]));
    }
  }, [filteredSessions, selectedKey]);

  const selectedSession = useMemo(() => {
    if (!selectedKey) return null;
    return (
      filteredSessions.find(
        (session) => getSessionKey(session) === selectedKey,
      ) || null
    );
  }, [filteredSessions, selectedKey]);

  return {
    search,
    setSearch,
    providerFilter,
    setProviderFilter,
    selectedKey,
    setSelectedKey,
    filteredSessions,
    selectedSession,
  };
}
