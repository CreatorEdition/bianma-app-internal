import { useCallback } from "react";
import type { TFunction } from "i18next";
import { toast } from "sonner";
import {
  useDisableCurrentOmo,
  useDisableCurrentOmoSlim,
} from "@/lib/query/omo";
import { extractErrorMessage } from "@/utils/errorUtils";

interface UseProviderOmoActionsOptions {
  t: TFunction;
}

export function useProviderOmoActions({ t }: UseProviderOmoActionsOptions) {
  const disableOmoMutation = useDisableCurrentOmo();
  const disableOmoSlimMutation = useDisableCurrentOmoSlim();

  const handleDisableOmo = useCallback(() => {
    disableOmoMutation.mutate(undefined, {
      onSuccess: () => {
        toast.success(t("omo.disabled", { defaultValue: "OMO 已停用" }));
      },
      onError: (error: Error) => {
        toast.error(
          t("omo.disableFailed", {
            defaultValue: "停用 OMO 失败: {{error}}",
            error: extractErrorMessage(error),
          }),
        );
      },
    });
  }, [disableOmoMutation, t]);

  const handleDisableOmoSlim = useCallback(() => {
    disableOmoSlimMutation.mutate(undefined, {
      onSuccess: () => {
        toast.success(t("omo.disabled", { defaultValue: "OMO 已停用" }));
      },
      onError: (error: Error) => {
        toast.error(
          t("omo.disableFailed", {
            defaultValue: "停用 OMO 失败: {{error}}",
            error: extractErrorMessage(error),
          }),
        );
      },
    });
  }, [disableOmoSlimMutation, t]);

  return {
    handleDisableOmo,
    handleDisableOmoSlim,
  };
}
