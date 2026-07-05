import type { TFunction } from "i18next";
import { toast } from "sonner";
import { sessionsApi } from "@/lib/api";
import { isMac } from "@/lib/platform";
import { extractErrorMessage } from "@/utils/errorUtils";
import type { SessionMeta } from "@/types";

interface UseSessionActionsOptions {
  t: TFunction;
  selectedSession: SessionMeta | null;
}

export function useSessionActions({
  t,
  selectedSession,
}: UseSessionActionsOptions) {
  const handleCopy = async (text: string, successMessage: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(successMessage);
    } catch (error) {
      toast.error(
        extractErrorMessage(error) ||
          t("common.error", { defaultValue: "Copy failed" }),
      );
    }
  };

  const handleResume = async () => {
    if (!selectedSession?.resumeCommand) return;

    if (!isMac()) {
      await handleCopy(
        selectedSession.resumeCommand,
        t("sessionManager.resumeCommandCopied"),
      );
      return;
    }

    try {
      await sessionsApi.launchTerminal({
        command: selectedSession.resumeCommand,
        cwd: selectedSession.projectDir ?? undefined,
      });
      toast.success(t("sessionManager.terminalLaunched"));
    } catch (error) {
      const fallback = selectedSession.resumeCommand;
      await handleCopy(fallback, t("sessionManager.resumeFallbackCopied"));
      toast.error(extractErrorMessage(error) || t("sessionManager.openFailed"));
    }
  };

  return {
    handleCopy,
    handleResume,
  };
}
