import type { TFunction } from "i18next";
import type {
  DeleteSessionOptions,
  DeleteSessionResult,
} from "@/lib/api/sessions";
import type { SessionMeta } from "@/types";

export function getDeletableSessions(
  targets: SessionMeta[] | null,
): SessionMeta[] {
  return (targets ?? []).filter((session) => Boolean(session.sourcePath));
}

export function toDeleteSessionOptions(
  targets: SessionMeta[],
): DeleteSessionOptions[] {
  return targets
    .filter((session) => Boolean(session.sourcePath))
    .map((session) => ({
      providerId: session.providerId,
      sessionId: session.sessionId,
      sourcePath: session.sourcePath!,
    }));
}

export function getDeleteResultSummary(
  results: DeleteSessionResult[],
  t: TFunction,
) {
  const deletedKeys = results
    .filter((result) => result.success)
    .map(
      (result) =>
        `${result.providerId}:${result.sessionId}:${result.sourcePath ?? ""}`,
    );

  const failedErrors = results
    .filter((result) => !result.success)
    .map((result) => result.error || t("common.unknown"));

  return {
    deletedKeys,
    failedErrors,
  };
}
