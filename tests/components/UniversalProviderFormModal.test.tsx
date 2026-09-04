import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UniversalProviderFormModal } from "@/components/universal/UniversalProviderFormModal";

vi.mock("@/components/JsonEditor", () => ({
  default: () => <div data-testid="json-editor" />,
}));

describe("UniversalProviderFormModal", () => {
  it("首次配置只显示地址、Key 和默认模型，高级项默认隐藏", async () => {
    const onSave = vi.fn();

    render(
      <UniversalProviderFormModal isOpen onClose={vi.fn()} onSave={onSave} />,
    );

    expect(screen.getByLabelText("API 地址")).toBeInTheDocument();
    expect(screen.getByLabelText("API Key")).toBeInTheDocument();
    expect(screen.getByLabelText("默认模型")).toBeInTheDocument();
    expect(screen.queryByLabelText("渠道名称（可选）")).not.toBeInTheDocument();
    expect(screen.queryByText("网关类型")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("API 地址"), {
      target: { value: "https://api.example.com" },
    });
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "test-key" },
    });
    fireEvent.change(screen.getByLabelText("默认模型"), {
      target: { value: "gpt-5.4" },
    });
    fireEvent.click(screen.getByText("添加"));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    const saved = onSave.mock.calls[0][0];
    expect(saved.name).toBe("example.com");
    expect(saved.models.claude.model).toBe("gpt-5.4");
    expect(saved.models.codex.model).toBe("gpt-5.4");
    expect(saved.models.gemini.model).toBe("gpt-5.4");
  });

  it("高级配置展开后才显示客户端映射和可选名称", () => {
    render(
      <UniversalProviderFormModal isOpen onClose={vi.fn()} onSave={vi.fn()} />,
    );

    fireEvent.click(screen.getByText("高级配置"));

    expect(screen.getByLabelText("渠道名称（可选）")).toBeInTheDocument();
    expect(screen.getByText("启用的应用")).toBeInTheDocument();
    expect(screen.getByText("模型配置")).toBeInTheDocument();
  });

  it("简易模式编辑只显示保存，不要求再次确认同步", () => {
    const provider = {
      id: "p1",
      name: "上游",
      providerType: "custom",
      apps: { claude: true, codex: true, gemini: true },
      baseUrl: "https://api.example.com",
      apiKey: "test-key",
      models: {
        claude: { model: "gpt-5.4" },
        codex: { model: "gpt-5.4" },
        gemini: { model: "gpt-5.4" },
      },
    };

    render(
      <UniversalProviderFormModal
        isOpen
        onClose={vi.fn()}
        onSave={vi.fn()}
        onSaveAndSync={vi.fn()}
        editingProvider={provider}
        simpleMode
      />,
    );

    expect(screen.getByText("保存")).toBeInTheDocument();
    expect(screen.queryByText("保存并同步")).not.toBeInTheDocument();
    expect(screen.queryByText("高级配置")).not.toBeInTheDocument();
    expect(screen.queryByText("启用的应用")).not.toBeInTheDocument();
  });
});
