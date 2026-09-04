import { useState } from "react";
import type { TFunction } from "i18next";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { sessionsApi } from "@/lib/api";
import { useDeleteSessionMutation } from "@/lib/query";
import type { SessionMeta } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";
import {
  getDeletableSessions,
  getDeleteResultSummary,
  toDeleteSessionOptions,
} from "../deleteUtils";
import { formatSessionTitle, getSessionKey } from "../utils";

interface UseSessionDeleteActionsOptions {
  t: TFunction;
  selectedSession: SessionMeta | null;
  selectedDeletableSessions: SessionMeta[];
  removeSelectedKeys: (keys: string[]) => void;
}

interface DeleteDialogContent {
  title: string;
  message: string;
  confirmText: string;
}

export function getDeleteDialogContent(
  deleteTargets: SessionMeta[] | null,
  t: TFunction,
): DeleteDialogContent {
  const isBatchDelete = Boolean(deleteTargets && deleteTargets.length > 1);

  if (isBatchDelete) {
    return {
      title: t("sessionManager.batchDeleteConfirmTitle", {
        defaultValue: "批量删除会话",
      }),
      message: t("sessionManager.batchDeleteConfirmMessage", {
        defaultValue:
          "将永久删除已选中的 {{count}} 个本地会话记录。\n\n此操作不可恢复。",
        count: deleteTargets?.length ?? 0,
      }),
      confirmText: t("sessionManager.batchDeleteConfirmAction", {
        defaultValue: "删除所选会话",
      }),
    };
  }

  return {
    title: t("sessionManager.deleteConfirmTitle", {
      defaultValue: "删除会话",
    }),
    message: deleteTargets?.[0]
      ? t("sessionManager.deleteConfirmMessage", {
          defaultValue:
            "将永久删除本地会话“{{title}}”\nSession ID: {{sessionId}}\n\n此操作不可恢复。",
          title: formatSessionTitle(deleteTargets[0]),
          sessionId: deleteTargets[0].sessionId,
        })
      : "",
    confirmText: t("sessionManager.deleteConfirmAction", {
      defaultValue: "删除会话",
    }),
  };
}

export function useSessionDeleteActions({
  t,
  selectedSession,
  selectedDeletableSessions,
  removeSelectedKeys,
}: UseSessionDeleteActionsOptions) {
  const queryClient = useQueryClient();
  const deleteSessionMutation = useDeleteSessionMutation();
  const [deleteTargets, setDeleteTargets] = useState<SessionMeta[] | null>(
    null,
  );
  const [isBatchDeleting, setIsBatchDeleting] = useState(false);
  const isDeleting = deleteSessionMutation.isPending || isBatchDeleting;

  const openBatchDeleteDialog = () => {
    if (selectedDeletableSessions.length === 0) return;
    setDeleteTargets(selectedDeletableSessions);
  };

  const openSingleDeleteDialog = () => {
    if (!selectedSession) return;
    setDeleteTargets([selectedSession]);
  };

  const closeDeleteDialog = () => {
    if (!isDeleting) {
      setDeleteTargets(null);
    }
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTargets || deleteTargets.length === 0 || isDeleting) {
      return;
    }

    const targets = getDeletableSessions(deleteTargets);
    const deleteOptions = toDeleteSessionOptions(targets);
    setDeleteTargets(null);

    if (deleteOptions.length === 0) {
      return;
    }

    if (deleteOptions.length === 1) {
      const [target] = targets;
      await deleteSessionMutation.mutateAsync(deleteOptions[0]);
      removeSelectedKeys([getSessionKey(target)]);
      return;
    }

    setIsBatchDeleting(true);
    try {
      const results = await sessionsApi.deleteMany(deleteOptions);
      const { deletedKeys, failedErrors } = getDeleteResultSummary(results, t);

      if (deletedKeys.length > 0) {
        const deletedKeySet = new Set(deletedKeys);
        queryClient.setQueryData<SessionMeta[]>(["sessions"], (current) =>
          (current ?? []).filter(
            (session) => !deletedKeySet.has(getSessionKey(session)),
          ),
        );
      }

      results
        .filter((result) => result.success)
        .forEach((result) => {
          queryClient.removeQueries({
            queryKey: ["sessionMessages", result.providerId, result.sourcePath],
          });
        });

      removeSelectedKeys(deletedKeys);

      await queryClient.invalidateQueries({ queryKey: ["sessions"] });

      if (deletedKeys.length > 0) {
        toast.success(
          t("sessionManager.batchDeleteSuccess", {
            defaultValue: "已删除 {{count}} 个会话",
            count: deletedKeys.length,
          }),
        );
      }

      if (failedErrors.length > 0) {
        toast.error(
          t("sessionManager.batchDeleteFailed", {
            defaultValue: "{{failed}} 个会话删除失败",
            failed: failedErrors.length,
          }),
          {
            description: failedErrors[0],
          },
        );
      }
    } catch (error) {
      toast.error(
        extractErrorMessage(error) ||
          t("sessionManager.batchDeleteRequestFailed", {
            defaultValue: "批量删除失败，请稍后重试",
          }),
      );
    } finally {
      setIsBatchDeleting(false);
    }
  };

  const dialogContent = getDeleteDialogContent(deleteTargets, t);

  return {
    deleteTargets,
    isBatchDeleting,
    isDeleting,
    openBatchDeleteDialog,
    openSingleDeleteDialog,
    closeDeleteDialog,
    handleDeleteConfirm,
    dialogTitle: dialogContent.title,
    dialogMessage: dialogContent.message,
    dialogConfirmText: dialogContent.confirmText,
  };
}
