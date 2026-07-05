import type { ComponentProps } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider, useTheme } from "@/components/theme-provider";

function ThemeProbe() {
  const { theme } = useTheme();
  return <div data-testid="theme-value">{theme}</div>;
}

function renderThemeProvider(
  props: Partial<ComponentProps<typeof ThemeProvider>> = {},
) {
  return render(
    <ThemeProvider {...props}>
      <ThemeProbe />
    </ThemeProvider>,
  );
}

describe("ThemeProvider 存储兼容", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove("light", "dark");
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      writable: true,
      value: vi.fn().mockReturnValue({
        matches: false,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }),
    });
  });

  it("将旧 cc-switch-theme 迁移到 bianma-theme 并清理旧键", async () => {
    localStorage.setItem("cc-switch-theme", "dark");

    renderThemeProvider();

    expect(screen.getByTestId("theme-value")).toHaveTextContent("dark");
    await waitFor(() => {
      expect(localStorage.getItem("bianma-theme")).toBe("dark");
      expect(localStorage.getItem("cc-switch-theme")).toBeNull();
    });
  });

  it("新旧键同时存在时优先使用 bianma-theme", async () => {
    localStorage.setItem("bianma-theme", "light");
    localStorage.setItem("cc-switch-theme", "dark");

    renderThemeProvider();

    expect(screen.getByTestId("theme-value")).toHaveTextContent("light");
    await waitFor(() => {
      expect(localStorage.getItem("bianma-theme")).toBe("light");
      expect(localStorage.getItem("cc-switch-theme")).toBeNull();
    });
  });
});
