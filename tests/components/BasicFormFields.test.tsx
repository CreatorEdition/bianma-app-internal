import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useForm } from "react-hook-form";
import { Form } from "@/components/ui/form";
import { BasicFormFields } from "@/components/providers/forms/BasicFormFields";
import type { ProviderFormData } from "@/lib/schemas/provider";
import { getIconMetadata } from "@/icons/extracted/metadata";

vi.mock("@/components/IconPicker", () => ({
  IconPicker: ({
    onValueChange,
  }: {
    onValueChange: (icon: string) => void;
  }) => (
    <button
      type="button"
      data-testid="mock-icon-option"
      onClick={() => onValueChange("gemini")}
    >
      choose-gemini
    </button>
  ),
}));

function renderBasicFormFields() {
  function Harness() {
    const form = useForm<ProviderFormData>({
      defaultValues: {
        name: "Provider",
        notes: "",
        websiteUrl: "",
        settingsConfig: "{}",
        icon: "",
        iconColor: "",
      },
    });

    const icon = form.watch("icon");
    const iconColor = form.watch("iconColor");

    return (
      <Form {...form}>
        <BasicFormFields form={form} />
        <div data-testid="icon-value">{icon || ""}</div>
        <div data-testid="icon-color-value">{iconColor || ""}</div>
      </Form>
    );
  }

  return render(<Harness />);
}

describe("BasicFormFields icon picker behavior", () => {
  it("selects icon and closes picker in one tap", () => {
    renderBasicFormFields();

    fireEvent.click(screen.getByTestId("provider-icon-trigger"));
    expect(screen.getByTestId("mock-icon-option")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("mock-icon-option"));

    expect(screen.getByTestId("icon-value")).toHaveTextContent("gemini");
    expect(screen.getByTestId("icon-color-value")).toHaveTextContent(
      getIconMetadata("gemini")?.defaultColor ?? "",
    );
    expect(screen.queryByTestId("mock-icon-option")).not.toBeInTheDocument();
  });

  it("keeps back button to close picker without changing icon", () => {
    renderBasicFormFields();

    fireEvent.click(screen.getByTestId("provider-icon-trigger"));
    fireEvent.click(screen.getByTestId("icon-picker-back"));

    expect(screen.getByTestId("icon-value")).toHaveTextContent("");
    expect(screen.getByTestId("icon-color-value")).toHaveTextContent("");
    expect(screen.queryByTestId("mock-icon-option")).not.toBeInTheDocument();
  });
});
