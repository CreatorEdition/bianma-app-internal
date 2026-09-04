import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCommonConfigSnippet } from "@/components/providers/forms/hooks/useCommonConfigSnippet";
import { useCodexCommonConfig } from "@/components/providers/forms/hooks/useCodexCommonConfig";
import { useGeminiCommonConfig } from "@/components/providers/forms/hooks/useGeminiCommonConfig";

const getCommonConfigSnippetMock = vi.fn();
const setCommonConfigSnippetMock = vi.fn();
const extractCommonConfigSnippetMock = vi.fn();

vi.mock("@/lib/api", () => ({
  configApi: {
    getCommonConfigSnippet: (...args: unknown[]) =>
      getCommonConfigSnippetMock(...args),
    setCommonConfigSnippet: (...args: unknown[]) =>
      setCommonConfigSnippetMock(...args),
    extractCommonConfigSnippet: (...args: unknown[]) =>
      extractCommonConfigSnippetMock(...args),
  },
}));

describe("common config snippet saving", () => {
  beforeEach(() => {
    getCommonConfigSnippetMock.mockResolvedValue("");
    setCommonConfigSnippetMock.mockResolvedValue(undefined);
    extractCommonConfigSnippetMock.mockResolvedValue("");
    localStorage.clear();
  });

  it("migrates Claude common config snippet from legacy localStorage", async () => {
    localStorage.setItem(
      "cc-switch:common-config-snippet",
      '{"includeCoAuthoredBy":false}',
    );

    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCommonConfigSnippet({
        settingsConfig: "{}",
        onConfigChange,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "claude",
      '{"includeCoAuthoredBy":false}',
    );
    expect(localStorage.getItem("cc-switch:common-config-snippet")).toBeNull();
  });

  it("does not persist an invalid Codex common config snippet", async () => {
    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCodexCommonConfig({
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let saved = false;
    act(() => {
      saved = result.current.handleCommonConfigSnippetChange(
        "base_url = https://bad.example/v1",
      );
    });

    expect(saved).toBe(false);
    expect(setCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(onConfigChange).not.toHaveBeenCalled();
    expect(result.current.commonConfigError).toContain("invalid value");
  });

  it("does not persist an invalid Gemini common config snippet", async () => {
    const onEnvChange = vi.fn();
    const { result } = renderHook(() =>
      useGeminiCommonConfig({
        envValue: "",
        onEnvChange,
        envStringToObj: () => ({}),
        envObjToString: () => "",
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let saved = false;
    act(() => {
      saved = result.current.handleCommonConfigSnippetChange(
        JSON.stringify({ GEMINI_MODEL: 123 }),
      );
    });

    expect(saved).toBe(false);
    expect(setCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(onEnvChange).not.toHaveBeenCalled();
    expect(result.current.commonConfigError).toBe(
      "geminiConfig.commonConfigInvalidValues",
    );
  });

  it("migrates Codex common config snippet from legacy localStorage", async () => {
    localStorage.setItem(
      "cc-switch:codex-common-config-snippet",
      'model = "gpt-5"',
    );

    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCodexCommonConfig({
        codexConfig: "",
        onConfigChange,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex",
      'model = "gpt-5"',
    );
    expect(
      localStorage.getItem("cc-switch:codex-common-config-snippet"),
    ).toBeNull();
  });

  it("migrates Gemini common config snippet from legacy localStorage", async () => {
    localStorage.setItem(
      "cc-switch:gemini-common-config-snippet",
      '{"GEMINI_MODEL":"gemini-2.5-pro"}',
    );

    const onEnvChange = vi.fn();
    const { result } = renderHook(() =>
      useGeminiCommonConfig({
        envValue: "",
        onEnvChange,
        envStringToObj: () => ({}),
        envObjToString: () => "",
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "gemini",
      '{"GEMINI_MODEL":"gemini-2.5-pro"}',
    );
    expect(
      localStorage.getItem("cc-switch:gemini-common-config-snippet"),
    ).toBeNull();
  });
});
