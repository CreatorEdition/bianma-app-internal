import { useEffect, type Dispatch, type SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { toast } from "sonner";
import type { AppId } from "@/lib/api";
import { checkAllEnvConflicts, checkEnvConflicts } from "@/lib/api/env";
import type { EnvConflict } from "@/types/env";

type CheckAllEnvConflictsFn = () => Promise<Record<string, EnvConflict[]>>;
type CheckEnvConflictsFn = (appType: AppId) => Promise<EnvConflict[]>;
type InvokeFn = typeof invoke;

interface UseAppStartupChecksOptions {
  activeApp: AppId;
  setEnvConflicts: Dispatch<SetStateAction<EnvConflict[]>>;
  setShowEnvBanner: Dispatch<SetStateAction<boolean>>;
  t: TFunction;
  checkAllEnvConflictsFn?: CheckAllEnvConflictsFn;
  checkEnvConflictsFn?: CheckEnvConflictsFn;
  invokeFn?: InvokeFn;
}

export function useAppStartupChecks({
  activeApp,
  setEnvConflicts,
  setShowEnvBanner,
  t,
  checkAllEnvConflictsFn = checkAllEnvConflicts,
  checkEnvConflictsFn = checkEnvConflicts,
  invokeFn = invoke,
}: UseAppStartupChecksOptions) {
  const queryClient = useQueryClient();

  useEffect(() => {
    const checkEnvOnStartup = async () => {
      try {
        const allConflicts = await checkAllEnvConflictsFn();
        const flatConflicts = Object.values(allConflicts).flat();

        if (flatConflicts.length > 0) {
          setEnvConflicts(flatConflicts);
          const dismissed = sessionStorage.getItem("env_banner_dismissed");
          if (!dismissed) {
            setShowEnvBanner(true);
          }
        }
      } catch (error) {
        console.error(
          "[App] Failed to check environment conflicts on startup:",
          error,
        );
      }
    };

    void checkEnvOnStartup();
  }, [checkAllEnvConflictsFn, setEnvConflicts, setShowEnvBanner]);

  useEffect(() => {
    const checkMigration = async () => {
      try {
        const migrated = await invokeFn<boolean>("get_migration_result");
        if (migrated) {
          toast.success(
            t("migration.success", { defaultValue: "配置迁移成功" }),
            { closeButton: true },
          );
        }
      } catch (error) {
        console.error("[App] Failed to check migration result:", error);
      }
    };

    void checkMigration();
  }, [invokeFn, t]);

  useEffect(() => {
    const checkSkillsMigration = async () => {
      try {
        const result = await invokeFn<{ count: number; error?: string } | null>(
          "get_skills_migration_result",
        );
        if (result?.error) {
          toast.error(t("migration.skillsFailed"), {
            description: t("migration.skillsFailedDescription"),
            closeButton: true,
          });
          console.error("[App] Skills SSOT migration failed:", result.error);
          return;
        }
        if (result && result.count > 0) {
          toast.success(t("migration.skillsSuccess", { count: result.count }), {
            closeButton: true,
          });
          await queryClient.invalidateQueries({ queryKey: ["skills"] });
        }
      } catch (error) {
        console.error("[App] Failed to check skills migration result:", error);
      }
    };

    void checkSkillsMigration();
  }, [invokeFn, queryClient, t]);

  useEffect(() => {
    const checkEnvOnSwitch = async () => {
      try {
        const conflicts = await checkEnvConflictsFn(activeApp);

        if (conflicts.length > 0) {
          setEnvConflicts((prev) => {
            const existingKeys = new Set(
              prev.map((c) => `${c.varName}:${c.sourcePath}`),
            );
            const newConflicts = conflicts.filter(
              (c) => !existingKeys.has(`${c.varName}:${c.sourcePath}`),
            );
            return [...prev, ...newConflicts];
          });
          const dismissed = sessionStorage.getItem("env_banner_dismissed");
          if (!dismissed) {
            setShowEnvBanner(true);
          }
        }
      } catch (error) {
        console.error(
          "[App] Failed to check environment conflicts on app switch:",
          error,
        );
      }
    };

    void checkEnvOnSwitch();
  }, [activeApp, checkEnvConflictsFn, setEnvConflicts, setShowEnvBanner]);
}
