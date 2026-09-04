import { useCallback, type Dispatch, type SetStateAction } from "react";
import { checkAllEnvConflicts } from "@/lib/api/env";
import type { EnvConflict } from "@/types/env";

interface UseEnvBannerActionsOptions {
  setEnvConflicts: Dispatch<SetStateAction<EnvConflict[]>>;
  setShowEnvBanner: Dispatch<SetStateAction<boolean>>;
  checkAllEnvConflictsFn?: typeof checkAllEnvConflicts;
}

export function useEnvBannerActions({
  setEnvConflicts,
  setShowEnvBanner,
  checkAllEnvConflictsFn = checkAllEnvConflicts,
}: UseEnvBannerActionsOptions) {
  const handleEnvBannerDismiss = useCallback(() => {
    setShowEnvBanner(false);
    sessionStorage.setItem("env_banner_dismissed", "true");
  }, [setShowEnvBanner]);

  const handleEnvBannerDeleted = useCallback(async () => {
    try {
      const allConflicts = await checkAllEnvConflictsFn();
      const flatConflicts = Object.values(allConflicts).flat();
      setEnvConflicts(flatConflicts);
      if (flatConflicts.length === 0) {
        setShowEnvBanner(false);
      }
    } catch (error) {
      console.error(
        "[App] Failed to re-check conflicts after deletion:",
        error,
      );
    }
  }, [checkAllEnvConflictsFn, setEnvConflicts, setShowEnvBanner]);

  return {
    handleEnvBannerDismiss,
    handleEnvBannerDeleted,
  };
}
