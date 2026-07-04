import { describe, expect, it } from "vitest";
import type { Provider } from "@/types";
import {
  getProviderConnectionDetails,
  inferProviderProtocolHint,
} from "@/utils/providerConfigUtils";

const createProvider = (
  settingsConfig: Provider["settingsConfig"],
  meta?: Provider["meta"],
): Provider => ({
  id: "test-provider",
  name: "Test Provider",
  settingsConfig,
  meta,
});

describe("Provider connection details", () => {
  it("reads Claude endpoint, key and protocol hint", () => {
    const provider = createProvider({
      env: {
        ANTHROPIC_BASE_URL: "https://api.example.com/anthropic",
        ANTHROPIC_AUTH_TOKEN: "claude-token",
      },
    });

    expect(getProviderConnectionDetails(provider, "claude")).toEqual({
      baseUrl: "https://api.example.com/anthropic",
      apiKey: "claude-token",
      protocolHint: "anthropic",
    });
  });

  it("reads Codex endpoint, key and protocol hint", () => {
    const provider = createProvider({
      auth: {
        OPENAI_API_KEY: "codex-key",
      },
      config: [
        'model_provider = "custom"',
        "",
        "[model_providers.custom]",
        'base_url = "https://api.example.com/v1"',
        "",
      ].join("\n"),
    });

    expect(getProviderConnectionDetails(provider, "codex")).toEqual({
      baseUrl: "https://api.example.com/v1",
      apiKey: "codex-key",
      protocolHint: "openai",
    });
  });

  it("reads Gemini endpoint, key and protocol hint", () => {
    const provider = createProvider({
      env: {
        GOOGLE_GEMINI_BASE_URL: "https://api.example.com/gemini",
        GEMINI_API_KEY: "gemini-key",
      },
    });

    expect(getProviderConnectionDetails(provider, "gemini")).toEqual({
      baseUrl: "https://api.example.com/gemini",
      apiKey: "gemini-key",
      protocolHint: "openai",
    });
  });

  it("prefers explicit protocol, then apiFormat, then endpoint/app inference", () => {
    expect(
      inferProviderProtocolHint(
        createProvider({}, { modelDiscoveryProtocol: "openai" }),
        "claude",
      ),
    ).toBe("openai");

    expect(
      inferProviderProtocolHint(
        createProvider({}, { apiFormat: "openai_responses" }),
        "claude",
      ),
    ).toBe("openai");

    expect(
      inferProviderProtocolHint(
        createProvider({
          env: { ANTHROPIC_BASE_URL: "https://proxy.example.com/anthropic" },
        }),
        "claude",
      ),
    ).toBe("anthropic");
  });

  it("only returns a protocol hint for OpenCode/OpenClaw when inferable", () => {
    const provider = createProvider({}, { modelDiscoveryProtocol: "openai" });

    expect(getProviderConnectionDetails(provider, "opencode")).toEqual({
      protocolHint: "openai",
    });
    expect(getProviderConnectionDetails(createProvider({}), "openclaw")).toEqual(
      {
        protocolHint: undefined,
      },
    );
  });
});
