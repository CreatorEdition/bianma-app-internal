import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { toast } from "sonner";
import type { DiscoveredModel, ProviderProtocolHint } from "@/types";
import { parseStructuredError } from "@/utils/errorUtils";

export interface FetchedModel {
  id: string;
  ownedBy: string | null;
}

export interface FetchProviderModelsOptions {
  baseUrl: string;
  apiKey?: string;
  protocolHint?: ProviderProtocolHint;
}

export type ModelFetchErrorCode =
  | "missing_endpoint"
  | "invalid_url"
  | "unauthorized"
  | "endpoint_not_found"
  | "timeout"
  | "empty_model_list"
  | "invalid_model_array"
  | "generic";

export interface ModelFetchStructuredError {
  code: ModelFetchErrorCode;
  message?: string;
  detail?: string;
  endpoint?: string;
  status?: number;
  rawMessage: string;
}

const MODEL_FETCH_ERROR_CODES: ReadonlySet<ModelFetchErrorCode> = new Set([
  "missing_endpoint",
  "invalid_url",
  "unauthorized",
  "endpoint_not_found",
  "timeout",
  "empty_model_list",
  "invalid_model_array",
  "generic",
]);

function asKnownModelFetchCode(code: unknown): ModelFetchErrorCode | null {
  if (typeof code !== "string") {
    return null;
  }
  return MODEL_FETCH_ERROR_CODES.has(code as ModelFetchErrorCode)
    ? (code as ModelFetchErrorCode)
    : null;
}

export function parseModelFetchStructuredError(
  error: unknown,
): ModelFetchStructuredError | null {
  if (error && typeof error === "object") {
    const candidate = error as Partial<ModelFetchStructuredError>;
    const code = asKnownModelFetchCode(candidate.code);
    if (code && typeof candidate.rawMessage === "string") {
      return {
        code,
        message: candidate.message,
        detail: candidate.detail,
        endpoint: candidate.endpoint,
        status: candidate.status,
        rawMessage: candidate.rawMessage,
      };
    }
  }

  const parsed = parseStructuredError(error);
  const code = asKnownModelFetchCode(parsed?.code);
  if (!parsed || !code) {
    return null;
  }

  const context = parsed.context ?? {};
  const endpoint =
    typeof context.endpoint === "string" ? context.endpoint : undefined;
  const detail = typeof context.detail === "string" ? context.detail : undefined;
  const status =
    typeof context.status === "number" ? context.status : undefined;

  return {
    code,
    message: parsed.message,
    detail,
    endpoint,
    status,
    rawMessage: parsed.rawMessage,
  };
}

/**
 * 从供应商获取可用模型列表
 *
 * 使用 OpenAI 兼容的 GET /v1/models 端点。
 * 主要面向第三方聚合站（硅基流动、OpenRouter 等）。
 */
export async function fetchModelsForConfig(
  baseUrl: string,
  apiKey: string,
  isFullUrl?: boolean,
): Promise<FetchedModel[]> {
  return invoke("fetch_models_for_config", { baseUrl, apiKey, isFullUrl });
}

export const modelFetchApi = {
  /**
   * 获取通用模型发现结果，供 Provider Workspace 后续接入。
   */
  async fetchProviderModels(
    options: FetchProviderModelsOptions,
  ): Promise<DiscoveredModel[]> {
    const { baseUrl, apiKey, protocolHint } = options;
    try {
      return await invoke("fetch_provider_models", {
        baseUrl,
        apiKey,
        protocolHint,
      });
    } catch (error) {
      const structuredError = parseModelFetchStructuredError(error);
      if (structuredError) {
        throw structuredError;
      }
      throw error;
    }
  },

  async fetchModelsForConfig(
    baseUrl: string,
    apiKey: string,
    isFullUrl?: boolean,
  ): Promise<FetchedModel[]> {
    return fetchModelsForConfig(baseUrl, apiKey, isFullUrl);
  },
};

/**
 * 根据错误类型显示对应的 toast 提示
 */
export function showFetchModelsError(
  err: unknown,
  t: TFunction,
  opts?: { hasApiKey: boolean; hasBaseUrl: boolean },
): void {
  // 前端预检：缺少必填字段
  if (opts && !opts.hasBaseUrl && !opts.hasApiKey) {
    toast.error(t("providerForm.fetchModelsNeedConfig"));
    return;
  }
  if (opts && !opts.hasApiKey) {
    toast.error(t("providerForm.fetchModelsNeedApiKey"));
    return;
  }
  if (opts && !opts.hasBaseUrl) {
    toast.error(t("providerForm.fetchModelsNeedEndpoint"));
    return;
  }

  // 解析后端错误字符串
  const structuredError = parseModelFetchStructuredError(err);
  if (structuredError?.code === "unauthorized") {
    toast.error(t("providerForm.fetchModelsAuthFailed"));
    return;
  }
  if (structuredError?.code === "endpoint_not_found") {
    toast.error(t("providerForm.fetchModelsNotSupported"));
    return;
  }
  if (structuredError?.code === "timeout") {
    toast.error(t("providerForm.fetchModelsTimeout"));
    return;
  }
  if (
    structuredError?.code === "invalid_model_array" ||
    structuredError?.code === "empty_model_list"
  ) {
    toast.error(t("providerForm.fetchModelsNotSupported"));
    return;
  }

  const msg = String(err);

  if (msg.includes("HTTP 401") || msg.includes("HTTP 403")) {
    toast.error(t("providerForm.fetchModelsAuthFailed"));
    return;
  }
  if (msg.includes("HTTP 404") || msg.includes("HTTP 405")) {
    toast.error(t("providerForm.fetchModelsNotSupported"));
    return;
  }
  if (msg.includes("timeout") || msg.includes("timed out")) {
    toast.error(t("providerForm.fetchModelsTimeout"));
    return;
  }
  if (msg.includes("Failed to parse")) {
    toast.error(t("providerForm.fetchModelsNotSupported"));
    return;
  }

  // 通用兜底
  toast.error(t("providerForm.fetchModelsFailed"));
}
