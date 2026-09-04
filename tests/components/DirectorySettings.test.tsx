import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import { DirectorySettings } from "@/components/settings/DirectorySettings";
import type { AppId } from "@/lib/api";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

const resolvedDirs = {
  appConfig: "/resolved/app",
  claude: "/resolved/claude",
  codex: "/resolved/codex",
  gemini: "/resolved/gemini",
  opencode: "/resolved/opencode",
  openclaw: "/resolved/openclaw",
};

function renderDirectorySettings(
  overrides: Partial<ComponentProps<typeof DirectorySettings>> = {},
) {
  const props = {
    appConfigDir: undefined,
    resolvedDirs,
    onAppConfigChange: vi.fn(),
    onBrowseAppConfig: vi.fn(async () => undefined),
    onResetAppConfig: vi.fn(async () => undefined),
    claudeDir: "/custom/claude",
    codexDir: undefined,
    geminiDir: undefined,
    opencodeDir: undefined,
    openclawDir: "/custom/openclaw",
    onDirectoryChange: vi.fn<(app: AppId, value?: string) => void>(),
    onBrowseDirectory: vi.fn(async (_app: AppId) => undefined),
    onResetDirectory: vi.fn(async (_app: AppId) => undefined),
    ...overrides,
  };

  render(<DirectorySettings {...props} />);

  return props;
}

function getDirectoryControlButton(inputValue: string, title: string) {
  const input = screen.getByDisplayValue(inputValue);
  const controls = input.parentElement;
  const button = controls?.querySelector(`button[title="${title}"]`);

  if (!(button instanceof HTMLButtonElement)) {
    throw new Error(`missing ${title} button for ${inputValue}`);
  }

  return button;
}

describe("DirectorySettings", () => {
  it("renders app config and supported app directory inputs", () => {
    renderDirectorySettings();

    expect(screen.getByDisplayValue("/resolved/app")).toBeInTheDocument();
    expect(screen.getByDisplayValue("/custom/claude")).toBeInTheDocument();
    expect(screen.getByDisplayValue("/resolved/codex")).toBeInTheDocument();
    expect(screen.getByDisplayValue("/resolved/gemini")).toBeInTheDocument();
    expect(screen.getByDisplayValue("/resolved/opencode")).toBeInTheDocument();
    expect(screen.getByDisplayValue("/custom/openclaw")).toBeInTheDocument();
    expect(screen.getByText("settings.openclawConfigDir")).toBeInTheDocument();
    expect(
      screen.getByText("settings.openclawConfigDirDescription"),
    ).toBeInTheDocument();
  });

  it("routes manual directory changes to the correct callbacks", () => {
    const props = renderDirectorySettings();

    fireEvent.change(screen.getByDisplayValue("/resolved/app"), {
      target: { value: "/manual/app" },
    });
    fireEvent.change(screen.getByDisplayValue("/resolved/codex"), {
      target: { value: "/manual/codex" },
    });
    fireEvent.change(screen.getByDisplayValue("/custom/openclaw"), {
      target: { value: "/manual/openclaw" },
    });

    expect(props.onAppConfigChange).toHaveBeenCalledWith("/manual/app");
    expect(props.onDirectoryChange).toHaveBeenCalledWith(
      "codex",
      "/manual/codex",
    );
    expect(props.onDirectoryChange).toHaveBeenCalledWith(
      "openclaw",
      "/manual/openclaw",
    );
  });

  it("routes browse and reset buttons without triggering unrelated apps", () => {
    const props = renderDirectorySettings();

    fireEvent.click(
      getDirectoryControlButton("/resolved/app", "settings.browseDirectory"),
    );
    fireEvent.click(
      getDirectoryControlButton("/resolved/app", "settings.resetDefault"),
    );
    fireEvent.click(
      getDirectoryControlButton("/resolved/codex", "settings.browseDirectory"),
    );
    fireEvent.click(
      getDirectoryControlButton("/custom/openclaw", "settings.resetDefault"),
    );

    expect(props.onBrowseAppConfig).toHaveBeenCalledTimes(1);
    expect(props.onResetAppConfig).toHaveBeenCalledTimes(1);
    expect(props.onBrowseDirectory).toHaveBeenCalledWith("codex");
    expect(props.onResetDirectory).toHaveBeenCalledWith("openclaw");
    expect(props.onBrowseDirectory).not.toHaveBeenCalledWith("claude");
    expect(props.onResetDirectory).not.toHaveBeenCalledWith("gemini");
  });
});
