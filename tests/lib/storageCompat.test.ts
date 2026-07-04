import { beforeEach, describe, expect, it } from "vitest";
import {
  consumeLegacyStorage,
  readCompatibleStorage,
  removeCompatibleStorage,
  writeCompatibleStorage,
} from "@/lib/storageCompat";

describe("storageCompat", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("reads the primary key directly when present", () => {
    localStorage.setItem("bianma-theme", "dark");
    localStorage.setItem("cc-switch-theme", "light");

    const value = readCompatibleStorage("bianma-theme", ["cc-switch-theme"]);

    expect(value).toBe("dark");
    expect(localStorage.getItem("cc-switch-theme")).toBe("light");
  });

  it("migrates a legacy key into the primary key when needed", () => {
    localStorage.setItem("cc-switch-last-view", "providers");

    const value = readCompatibleStorage("bianma-last-view", [
      "cc-switch-last-view",
    ]);

    expect(value).toBe("providers");
    expect(localStorage.getItem("bianma-last-view")).toBe("providers");
    expect(localStorage.getItem("cc-switch-last-view")).toBeNull();
  });

  it("writes the primary key and clears legacy keys", () => {
    localStorage.setItem("ccswitch:update:dismissedVersion", "1.0.0");
    localStorage.setItem("dismissedUpdateVersion", "0.9.0");

    writeCompatibleStorage("bianma:update:dismissedVersion", "1.1.0", [
      "ccswitch:update:dismissedVersion",
      "dismissedUpdateVersion",
    ]);

    expect(localStorage.getItem("bianma:update:dismissedVersion")).toBe("1.1.0");
    expect(localStorage.getItem("ccswitch:update:dismissedVersion")).toBeNull();
    expect(localStorage.getItem("dismissedUpdateVersion")).toBeNull();
  });

  it("removes the primary key and all legacy keys together", () => {
    localStorage.setItem("bianma-last-app", "codex");
    localStorage.setItem("cc-switch-last-app", "claude");

    removeCompatibleStorage("bianma-last-app", ["cc-switch-last-app"]);

    expect(localStorage.getItem("bianma-last-app")).toBeNull();
    expect(localStorage.getItem("cc-switch-last-app")).toBeNull();
  });

  it("consumes the first available legacy key and clears it", () => {
    localStorage.setItem("legacy-a", "value-a");
    localStorage.setItem("legacy-b", "value-b");

    const value = consumeLegacyStorage(["legacy-a", "legacy-b"]);

    expect(value).toBe("value-a");
    expect(localStorage.getItem("legacy-a")).toBeNull();
    expect(localStorage.getItem("legacy-b")).toBe("value-b");
  });
});
