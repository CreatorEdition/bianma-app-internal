import { useCallback, useEffect, useMemo, useState } from "react";
import type { SessionMeta } from "@/types";
import { getSessionKey } from "../utils";

interface UseSessionSelectionStateOptions {
  sessions: SessionMeta[];
  filteredSessions: SessionMeta[];
  selectionMode: boolean;
}

export function useSessionSelectionState({
  sessions,
  filteredSessions,
  selectionMode,
}: UseSessionSelectionStateOptions) {
  const [selectedSessionKeys, setSelectedSessionKeys] = useState<Set<string>>(
    () => new Set(),
  );

  const deletableFilteredSessions = useMemo(
    () => filteredSessions.filter((session) => Boolean(session.sourcePath)),
    [filteredSessions],
  );

  const selectedSessions = useMemo(
    () =>
      sessions.filter((session) =>
        selectedSessionKeys.has(getSessionKey(session)),
      ),
    [sessions, selectedSessionKeys],
  );

  const selectedDeletableSessions = useMemo(
    () => selectedSessions.filter((session) => Boolean(session.sourcePath)),
    [selectedSessions],
  );

  const allFilteredSelected =
    deletableFilteredSessions.length > 0 &&
    deletableFilteredSessions.every((session) =>
      selectedSessionKeys.has(getSessionKey(session)),
    );

  useEffect(() => {
    const validKeys = new Set(
      sessions.map((session) => getSessionKey(session)),
    );
    setSelectedSessionKeys((current) => {
      let changed = false;
      const next = new Set<string>();
      current.forEach((key) => {
        if (validKeys.has(key)) {
          next.add(key);
        } else {
          changed = true;
        }
      });
      return changed ? next : current;
    });
  }, [sessions]);

  useEffect(() => {
    if (!selectionMode) return;

    const visibleKeys = new Set(
      deletableFilteredSessions.map((session) => getSessionKey(session)),
    );

    setSelectedSessionKeys((current) => {
      let changed = false;
      const next = new Set<string>();
      current.forEach((key) => {
        if (visibleKeys.has(key)) {
          next.add(key);
        } else {
          changed = true;
        }
      });
      return changed ? next : current;
    });
  }, [deletableFilteredSessions, selectionMode]);

  const toggleSessionChecked = useCallback(
    (session: SessionMeta, checked: boolean) => {
      if (!session.sourcePath) return;
      const key = getSessionKey(session);
      setSelectedSessionKeys((current) => {
        const next = new Set(current);
        if (checked) {
          next.add(key);
        } else {
          next.delete(key);
        }
        return next;
      });
    },
    [],
  );

  const toggleSelectAll = useCallback(() => {
    setSelectedSessionKeys((current) => {
      const next = new Set(current);
      if (allFilteredSelected) {
        deletableFilteredSessions.forEach((session) =>
          next.delete(getSessionKey(session)),
        );
      } else {
        deletableFilteredSessions.forEach((session) =>
          next.add(getSessionKey(session)),
        );
      }
      return next;
    });
  }, [allFilteredSelected, deletableFilteredSessions]);

  const clearSelection = useCallback(() => {
    setSelectedSessionKeys(new Set());
  }, []);

  const removeSelectedKeys = useCallback((keys: string[]) => {
    setSelectedSessionKeys((current) => {
      const next = new Set(current);
      keys.forEach((key) => next.delete(key));
      return next;
    });
  }, []);

  return {
    selectedSessionKeys,
    deletableFilteredSessions,
    selectedSessions,
    selectedDeletableSessions,
    allFilteredSelected,
    toggleSessionChecked,
    toggleSelectAll,
    clearSelection,
    removeSelectedKeys,
  };
}
