import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { TFunction } from "i18next";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { EnvConflict } from "@/types/env";
import type { AppId } from "@/lib/api";
import { useAppStartupChecks } from "@/hooks/useAppStartupChecks";

const emptyConflicts = { claude: [], codex: [], gemini: [] };

const startupConflict: EnvConflict = {
  varName: "OPENAI_API_KEY",
  varValue: "test-key",
  sourceType: "file",
  sourcePath: "/tmp/.env:1",
};

const duplicateConflict: EnvConflict = {
  varName: "ANTHROPIC_API_KEY",
  varValue: "duplicate",
  sourceType: "file",
  sourcePath: "/tmp/.zshrc:2",
};

const newConflict: EnvConflict = {
  varName: "OPENAI_API_KEY",
  varValue: "new",
  sourceType: "file",
  sourcePath: "/tmp/.bashrc:3",
};

const checkAllEnvConflictsMock = vi.fn();
const checkEnvConflictsMock = vi.fn();
const invokeMock = vi.fn();

let queryClient: QueryClient;
let invalidateQueriesSpy: any;
let toastSuccessSpy: any;
let toastErrorSpy: any;
let consoleErrorSpy: any;

const t = ((key: string, options?: Record<string, unknown>) => {
  if (key === "migration.success") {
    return String(options?.defaultValue ?? key);
  }
  if (key === "migration.skillsSuccess") {
    return `技能迁移成功: ${options?.count}`;
  }
  if (key === "migration.skillsFailed") {
    return "技能迁移失败";
  }
  if (key === "migration.skillsFailedDescription") {
    return "技能迁移失败描述";
  }
  return key;
}) as TFunction;

const defaultInvoke = async (command: string) => {
  if (command === "get_migration_result") {
    return false;
  }
  if (command === "get_skills_migration_result") {
    return null;
  }
  return undefined;
};

const renderUseAppStartupChecks = ({
  activeApp = "codex",
  setEnvConflicts = vi.fn(),
  setShowEnvBanner = vi.fn(),
}: {
  activeApp?: AppId;
  setEnvConflicts?: React.Dispatch<React.SetStateAction<EnvConflict[]>>;
  setShowEnvBanner?: React.Dispatch<React.SetStateAction<boolean>>;
} = {}) => {
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );

  return renderHook(
    ({ app }: { app: AppId }) =>
      useAppStartupChecks({
        activeApp: app,
        setEnvConflicts,
        setShowEnvBanner,
        t,
        checkAllEnvConflictsFn: checkAllEnvConflictsMock,
        checkEnvConflictsFn: checkEnvConflictsMock,
        invokeFn: invokeMock,
      }),
    { initialProps: { app: activeApp }, wrapper },
  );
};

beforeEach(() => {
  queryClient = new QueryClient();
  sessionStorage.clear();

  checkAllEnvConflictsMock.mockReset();
  checkAllEnvConflictsMock.mockResolvedValue(emptyConflicts);
  checkEnvConflictsMock.mockReset();
  checkEnvConflictsMock.mockResolvedValue([]);
  invokeMock.mockReset();
  invokeMock.mockImplementation(defaultInvoke);

  invalidateQueriesSpy = vi.spyOn(queryClient, "invalidateQueries");
  toastSuccessSpy = vi.spyOn(toast, "success");
  toastSuccessSpy.mockImplementation(() => "" as never);
  toastErrorSpy = vi.spyOn(toast, "error");
  toastErrorSpy.mockImplementation(() => "" as never);
  consoleErrorSpy = vi.spyOn(console, "error");
  consoleErrorSpy.mockImplementation(() => {});
});

afterEach(() => {
  invalidateQueriesSpy.mockRestore();
  toastSuccessSpy.mockRestore();
  toastErrorSpy.mockRestore();
  consoleErrorSpy.mockRestore();
});

describe("useAppStartupChecks", () => {
  it("shows the environment banner on startup when conflicts exist", async () => {
    const setEnvConflicts = vi.fn();
    const setShowEnvBanner = vi.fn();
    checkAllEnvConflictsMock.mockResolvedValueOnce({
      claude: [startupConflict],
      codex: [],
      gemini: [],
    });

    renderUseAppStartupChecks({ setEnvConflicts, setShowEnvBanner });

    await waitFor(() => {
      expect(setEnvConflicts).toHaveBeenCalledWith([startupConflict]);
      expect(setShowEnvBanner).toHaveBeenCalledWith(true);
    });
  });

  it("keeps the environment banner hidden when startup conflicts were dismissed", async () => {
    const setEnvConflicts = vi.fn();
    const setShowEnvBanner = vi.fn();
    sessionStorage.setItem("env_banner_dismissed", "true");
    checkAllEnvConflictsMock.mockResolvedValueOnce({
      claude: [],
      codex: [startupConflict],
      gemini: [],
    });

    renderUseAppStartupChecks({ setEnvConflicts, setShowEnvBanner });

    await waitFor(() => {
      expect(setEnvConflicts).toHaveBeenCalledWith([startupConflict]);
    });
    expect(setShowEnvBanner).not.toHaveBeenCalledWith(true);
  });

  it("shows the config migration success toast", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_migration_result") {
        return true;
      }
      if (command === "get_skills_migration_result") {
        return null;
      }
      return undefined;
    });

    renderUseAppStartupChecks();

    await waitFor(() => {
      expect(toastSuccessSpy).toHaveBeenCalledWith("配置迁移成功", {
        closeButton: true,
      });
    });
  });

  it("shows the skills migration success toast and invalidates skills", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_migration_result") {
        return false;
      }
      if (command === "get_skills_migration_result") {
        return { count: 2 };
      }
      return undefined;
    });

    renderUseAppStartupChecks();

    await waitFor(() => {
      expect(toastSuccessSpy).toHaveBeenCalledWith("技能迁移成功: 2", {
        closeButton: true,
      });
      expect(invalidateQueriesSpy).toHaveBeenCalledWith({
        queryKey: ["skills"],
      });
    });
  });

  it("shows the skills migration failure toast and logs the error", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_migration_result") {
        return false;
      }
      if (command === "get_skills_migration_result") {
        return { count: 0, error: "bad source" };
      }
      return undefined;
    });

    renderUseAppStartupChecks();

    await waitFor(() => {
      expect(toastErrorSpy).toHaveBeenCalledWith("技能迁移失败", {
        description: "技能迁移失败描述",
        closeButton: true,
      });
      expect(consoleErrorSpy).toHaveBeenCalledWith(
        "[App] Skills SSOT migration failed:",
        "bad source",
      );
    });
  });

  it("merges new environment conflicts when the active app changes", async () => {
    const setEnvConflicts = vi.fn();
    const setShowEnvBanner = vi.fn();
    const rendered = renderUseAppStartupChecks({
      activeApp: "codex",
      setEnvConflicts,
      setShowEnvBanner,
    });

    await waitFor(() => {
      expect(checkEnvConflictsMock).toHaveBeenCalledWith("codex");
    });

    checkEnvConflictsMock.mockClear();
    checkEnvConflictsMock.mockResolvedValueOnce([
      duplicateConflict,
      newConflict,
    ]);

    rendered.rerender({ app: "gemini" });

    await waitFor(() => {
      expect(checkEnvConflictsMock).toHaveBeenCalledWith("gemini");
      expect(setEnvConflicts).toHaveBeenCalledWith(expect.any(Function));
    });

    const mergeConflicts = setEnvConflicts.mock.calls.at(-1)?.[0] as (
      previous: EnvConflict[],
    ) => EnvConflict[];

    expect(mergeConflicts([duplicateConflict])).toEqual([
      duplicateConflict,
      newConflict,
    ]);
    expect(setShowEnvBanner).toHaveBeenCalledWith(true);
  });
});
