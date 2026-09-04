import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProviderKeyField } from "@/components/providers/forms/ProviderKeyField";

function renderProviderKeyField(
  props: Partial<React.ComponentProps<typeof ProviderKeyField>> = {},
) {
  const onChange = vi.fn();
  render(
    <ProviderKeyField
      inputId="provider-key"
      label="供应商标识"
      value=""
      placeholder="provider-key"
      existingKeys={[]}
      isEditMode={false}
      duplicateMessage="标识重复"
      invalidMessage="标识无效"
      hintMessage="请输入供应商标识"
      lockedHintMessage="标识已锁定"
      onChange={onChange}
      {...props}
    />,
  );
  return { onChange };
}

describe("ProviderKeyField", () => {
  it("输入时归一化为小写字母、数字与连字符", () => {
    const { onChange } = renderProviderKeyField();

    fireEvent.change(screen.getByLabelText(/供应商标识/), {
      target: { value: "Open_Claw-01!" },
    });

    expect(onChange).toHaveBeenCalledWith("openclaw-01");
  });

  it("重复标识优先显示重复提示并标红", () => {
    renderProviderKeyField({
      value: "openclaw",
      existingKeys: ["openclaw"],
    });

    const input = screen.getByLabelText(/供应商标识/);
    expect(input).toHaveClass("border-destructive");
    expect(screen.getByText("标识重复")).toBeInTheDocument();
    expect(screen.queryByText("请输入供应商标识")).not.toBeInTheDocument();
  });

  it("非空且格式非法时显示非法提示并标红", () => {
    renderProviderKeyField({ value: "openclaw--bad" });

    const input = screen.getByLabelText(/供应商标识/);
    expect(input).toHaveClass("border-destructive");
    expect(screen.getByText("标识无效")).toBeInTheDocument();
    expect(screen.queryByText("请输入供应商标识")).not.toBeInTheDocument();
  });

  it("编辑态锁定时禁用输入并显示锁定提示", () => {
    renderProviderKeyField({
      value: "openclaw",
      existingKeys: ["openclaw"],
      isEditMode: true,
      isLocked: true,
    });

    expect(screen.getByLabelText(/供应商标识/)).toBeDisabled();
    expect(screen.getByText("标识已锁定")).toBeInTheDocument();
    expect(screen.queryByText("标识重复")).not.toBeInTheDocument();
  });

  it("编辑态加载时禁用输入并优先显示加载提示", () => {
    renderProviderKeyField({
      value: "openclaw",
      existingKeys: ["openclaw"],
      isEditMode: true,
      isLoading: true,
      loadingMessage: "正在加载状态",
    });

    expect(screen.getByLabelText(/供应商标识/)).toBeDisabled();
    expect(screen.getByText("正在加载状态")).toBeInTheDocument();
    expect(screen.queryByText("标识重复")).not.toBeInTheDocument();
  });

  it("创建态加载时不禁用输入并显示普通提示", () => {
    renderProviderKeyField({
      isLoading: true,
      loadingMessage: "正在加载状态",
    });

    expect(screen.getByLabelText(/供应商标识/)).not.toBeDisabled();
    expect(screen.getByText("请输入供应商标识")).toBeInTheDocument();
    expect(screen.queryByText("正在加载状态")).not.toBeInTheDocument();
  });
});
