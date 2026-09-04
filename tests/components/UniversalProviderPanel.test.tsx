import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UniversalProviderPanel } from "@/components/universal/UniversalProviderPanel";
import type { UniversalProvider } from "@/types";

const toastSuccessMock = vi.hoisted(() => vi.fn());
const toastErrorMock = vi.hoisted(() => vi.fn());
const getAllMock = vi.hoisted(() => vi.fn());
const upsertMock = vi.hoisted(() => vi.fn());
const deleteMock = vi.hoisted(() => vi.fn());
const syncMock = vi.hoisted(() => vi.fn());
const formPropsMock = vi.hoisted(() => vi.fn());

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));

vi.mock("@/lib/api", () => ({
  universalProvidersApi: {
    getAll: (...args: unknown[]) => getAllMock(...args),
    upsert: (...args: unknown[]) => upsertMock(...args),
    delete: (...args: unknown[]) => deleteMock(...args),
    sync: (...args: unknown[]) => syncMock(...args),
  },
}));

vi.mock("@/components/universal/UniversalProviderFormModal", () => ({
  UniversalProviderFormModal: (props: {
    isOpen: boolean;
    onSave: (provider: UniversalProvider) => void;
  }) => {
    formPropsMock(props);
    return props.isOpen ? (
      <button onClick={() => props.onSave(createProvider("new", "New"))}>
        submit-provider
      </button>
    ) : null;
  },
}));

vi.mock("@/components/ConfirmDialog", () => ({
  ConfirmDialog: ({
    isOpen,
    title,
    message,
    confirmText,
    onConfirm,
    onCancel,
  }: {
    isOpen: boolean;
    title: string;
    message: string;
    confirmText: string;
    onConfirm: () => void;
    onCancel: () => void;
  }) =>
    isOpen ? (
      <div data-testid="confirm-dialog">
        <div>{title}</div>
        <div>{message}</div>
        <button onClick={onConfirm}>{confirmText}</button>
        <button onClick={onCancel}>cancel</button>
      </div>
    ) : null,
}));

const createProvider = (id: string, name: string): UniversalProvider => ({
  id,
  name,
  providerType: "custom",
  apps: {
    claude: true,
    codex: true,
    gemini: true,
  },
  baseUrl: "https://api.example.com",
  apiKey: "test-key",
  models: {
    claude: { model: "gpt-5.4" },
    codex: { model: "gpt-5.4" },
    gemini: { model: "gpt-5.4" },
  },
});

const successfulSyncResult = (id: string) => ({
  providerId: id,
  success: true,
  apps: ["claude", "codex", "gemini"].map((app) => ({
    app,
    success: true,
    becameCurrent: false,
  })),
});

describe("UniversalProviderPanel", () => {
  beforeEach(() => {
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
    getAllMock.mockReset();
    upsertMock.mockReset();
    deleteMock.mockReset();
    syncMock.mockReset();
    formPropsMock.mockReset();
    localStorage.clear();

    getAllMock.mockResolvedValue({
      p1: createProvider("p1", "Provider One"),
      p2: createProvider("p2", "Provider Two"),
    });
    upsertMock.mockResolvedValue(true);
    deleteMock.mockResolvedValue(true);
    syncMock.mockImplementation(async (id: string) => successfulSyncResult(id));
  });

  it("简易模式新建上游只保存，客户端准备留到路由中心启动", async () => {
    render(<UniversalProviderPanel simpleMode />);

    await waitFor(() => {
      expect(screen.getByText("Provider One")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("添加上游"));
    fireEvent.click(screen.getByText("submit-provider"));

    await waitFor(() => {
      expect(upsertMock).toHaveBeenCalledWith(
        expect.objectContaining({ id: "new" }),
      );
    });
    expect(syncMock).not.toHaveBeenCalled();
    expect(toastSuccessMock).toHaveBeenCalledWith(
      "统一上游已保存，返回路由中心一键启动即可接入",
    );
  });

  it("简易模式编辑上游只更新统一配置", async () => {
    render(<UniversalProviderPanel simpleMode />);

    await waitFor(() => {
      expect(screen.getByText("Provider One")).toBeInTheDocument();
    });
    fireEvent.click(screen.getAllByTitle("编辑")[0]);
    fireEvent.click(screen.getByText("submit-provider"));

    await waitFor(() => {
      expect(upsertMock).toHaveBeenCalled();
    });
    expect(syncMock).not.toHaveBeenCalled();
  });

  it("简易模式只标记一个当前上游", async () => {
    localStorage.setItem("bianma-universal-active-provider", "p2");

    render(<UniversalProviderPanel simpleMode />);

    await waitFor(() => {
      expect(screen.getByTitle("当前上游")).toBeInTheDocument();
    });
    expect(screen.getAllByTitle("当前上游")).toHaveLength(1);
    expect(screen.getAllByTitle("设为当前上游")).toHaveLength(1);
  });

  it("简易模式不会让部分客户端配置成为当前上游", async () => {
    const partialProvider = createProvider("p2", "Partial Provider");
    partialProvider.apps.codex = false;
    partialProvider.models.codex = undefined;
    localStorage.setItem("bianma-universal-active-provider", "p2");
    getAllMock.mockResolvedValue({
      p1: createProvider("p1", "Provider One"),
      p2: partialProvider,
    });

    render(<UniversalProviderPanel simpleMode />);

    await waitFor(() => {
      expect(screen.getByTitle("当前上游")).toBeInTheDocument();
    });
    expect(screen.getAllByTitle("当前上游")).toHaveLength(1);
    expect(screen.queryByTitle("设为当前上游")).not.toBeInTheDocument();
  });

  it("删除当前上游后回退到下一个可用上游", async () => {
    localStorage.setItem("bianma-universal-active-provider", "p2");
    getAllMock.mockReset();
    getAllMock
      .mockResolvedValueOnce({
        p1: createProvider("p1", "Provider One"),
        p2: createProvider("p2", "Provider Two"),
      })
      .mockResolvedValueOnce({
        p1: createProvider("p1", "Provider One"),
      });

    render(<UniversalProviderPanel simpleMode />);

    await waitFor(() => {
      expect(screen.getByTitle("当前上游")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("delete-provider-p2"));
    fireEvent.click(screen.getByText("删除"));

    await waitFor(() => {
      expect(screen.queryByText("Provider Two")).not.toBeInTheDocument();
      expect(screen.getByTitle("当前上游")).toBeInTheDocument();
    });
    expect(screen.getAllByTitle("当前上游")).toHaveLength(1);
  });

  it("单个同步成功后显示最近同步成功并传入正确 provider id", async () => {
    render(<UniversalProviderPanel />);

    await waitFor(() => {
      expect(screen.getByText("Provider One")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("sync-provider-p1"));
    fireEvent.click(screen.getByText("同步"));

    await waitFor(() => {
      expect(screen.getByText("最近同步成功")).toBeInTheDocument();
    });
    expect(syncMock).toHaveBeenCalledWith("p1");
  });

  it("单个同步失败后显示最近同步失败和错误摘要并触发错误 toast", async () => {
    const consoleErrorSpy = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    try {
      syncMock.mockRejectedValueOnce(new Error("network down"));

      render(<UniversalProviderPanel />);

      await waitFor(() => {
        expect(screen.getByText("Provider One")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("sync-provider-p1"));
      fireEvent.click(screen.getByText("同步"));

      await waitFor(() => {
        expect(screen.getByText("最近同步失败")).toBeInTheDocument();
        expect(screen.getByText("network down")).toBeInTheDocument();
      });
      expect(toastErrorMock).toHaveBeenCalled();
    } finally {
      consoleErrorSpy.mockRestore();
    }
  });

  it("批量同步选中的两个统一供应商", async () => {
    render(<UniversalProviderPanel />);

    await waitFor(() => {
      expect(screen.getByText("Provider One")).toBeInTheDocument();
      expect(screen.getByText("Provider Two")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("select-provider-p1"));
    fireEvent.click(screen.getByTestId("select-provider-p2"));
    fireEvent.click(screen.getByTestId("batch-sync-button"));

    await waitFor(() => {
      expect(syncMock).toHaveBeenCalledTimes(2);
    });
    expect(syncMock).toHaveBeenCalledWith("p1");
    expect(syncMock).toHaveBeenCalledWith("p2");
  });
  it("供应商列表变化后清理已不存在项的选择和同步状态", async () => {
    render(<UniversalProviderPanel />);

    await waitFor(() => {
      expect(screen.getByText("Provider One")).toBeInTheDocument();
      expect(screen.getByText("Provider Two")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("select-provider-p2"));
    fireEvent.click(screen.getByTestId("sync-provider-p2"));
    fireEvent.click(screen.getByText("同步"));

    await waitFor(() => {
      expect(screen.getByTestId("sync-status-p2")).toHaveTextContent(
        "最近同步成功",
      );
    });

    getAllMock.mockResolvedValueOnce({
      p1: createProvider("p1", "Provider One"),
    });

    fireEvent.click(screen.getByTestId("delete-provider-p2"));
    fireEvent.click(screen.getByText("删除"));

    await waitFor(() => {
      expect(screen.queryByText("Provider Two")).not.toBeInTheDocument();
    });
    expect(screen.queryByTestId("sync-status-p2")).not.toBeInTheDocument();
    expect(screen.getByTestId("selected-count")).toHaveTextContent(
      "已选择 0 项",
    );
  });
});
