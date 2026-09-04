import { invoke } from "@tauri-apps/api/core";

export async function copyTextToClipboard(text: string): Promise<void> {
  if (text.length === 0) {
    return;
  }

  try {
    await invoke("copy_text_to_clipboard", { text });
    return;
  } catch {
    if (
      typeof navigator !== "undefined" &&
      typeof navigator.clipboard?.writeText === "function"
    ) {
      await navigator.clipboard.writeText(text);
      return;
    }

    throw new Error("Clipboard copy is unavailable");
  }
}

export async function copyText(text: string): Promise<void> {
  return copyTextToClipboard(text);
}
