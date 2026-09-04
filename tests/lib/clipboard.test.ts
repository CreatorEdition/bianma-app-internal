import { beforeEach, describe, expect, it, vi } from "vitest";
import { copyText, copyTextToClipboard } from "@/lib/clipboard";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

const writeTextMock = vi.fn();

function setNavigatorClipboard(writeText?: typeof writeTextMock) {
  Object.defineProperty(globalThis.navigator, "clipboard", {
    configurable: true,
    value: writeText ? { writeText } : undefined,
  });
}

describe("clipboard", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    writeTextMock.mockReset();
    setNavigatorClipboard(writeTextMock);
  });

  it("does not call native or browser clipboard for empty text", async () => {
    await copyTextToClipboard("");

    expect(invokeMock).not.toHaveBeenCalled();
    expect(writeTextMock).not.toHaveBeenCalled();
  });

  it("does not call browser clipboard when native copy succeeds", async () => {
    invokeMock.mockResolvedValue(undefined);

    await copyTextToClipboard("abc");

    expect(invokeMock).toHaveBeenCalledWith("copy_text_to_clipboard", {
      text: "abc",
    });
    expect(writeTextMock).not.toHaveBeenCalled();
  });

  it("calls browser clipboard when native copy fails", async () => {
    invokeMock.mockRejectedValue(new Error("native failed"));
    writeTextMock.mockResolvedValue(undefined);

    await copyTextToClipboard("fallback");

    expect(writeTextMock).toHaveBeenCalledWith("fallback");
  });

  it("throws unavailable error when native copy fails without browser clipboard", async () => {
    invokeMock.mockRejectedValue(new Error("native failed"));
    setNavigatorClipboard();

    await expect(copyTextToClipboard("missing")).rejects.toThrow(
      "Clipboard copy is unavailable",
    );
  });

  it("keeps copyText as a compatible alias", async () => {
    invokeMock.mockResolvedValue(undefined);

    await copyText("alias");

    expect(invokeMock).toHaveBeenCalledWith("copy_text_to_clipboard", {
      text: "alias",
    });
  });
});
