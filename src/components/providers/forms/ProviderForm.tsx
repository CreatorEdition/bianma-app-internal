import { useEffect, useMemo, useState, useCallback } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Form, FormField, FormItem, FormMessage } from "@/components/ui/form";
import { providerSchema, type ProviderFormData } from "@/lib/schemas/provider";
import { providersApi, type AppId } from "@/lib/api";
import type {
  ProviderCategory,
  ProviderMeta,
  ProviderTestConfig,
  ProviderProxyConfig,
  ClaudeApiFormat,
  ClaudeApiKeyField,
} from "@/types";
import type { OpenClawSuggestedDefaults } from "@/config/openclawProviderPresets";
import { OpenCodeFormFields } from "./OpenCodeFormFields";
import { OpenClawFormFields } from "./OpenClawFormFields";
import type { UniversalProviderPreset } from "@/config/universalProviderPresets";
import { hasApiKeyField } from "@/utils/providerConfigUtils";
import { mergeProviderMeta } from "@/utils/providerMetaUtils";
import { getCodexCustomTemplate } from "@/config/codexTemplates";
import CodexConfigEditor from "./CodexConfigEditor";
import { CommonConfigEditor } from "./CommonConfigEditor";
import GeminiConfigEditor from "./GeminiConfigEditor";
import JsonEditor from "@/components/JsonEditor";
import { Label } from "@/components/ui/label";
import { ProviderPresetSelector } from "./ProviderPresetSelector";
import { ProviderKeyField } from "./ProviderKeyField";
import {
  getPresetCategoryKeys,
  getPresetCategoryLabels,
  getPresetEntriesByApp,
  groupPresetEntries,
} from "./providerPresetUtils";
import {
  getCustomPresetResetPlan,
  resolvePresetSelection,
  type PresetSelectionResult,
} from "./providerPresetApplyUtils";
import {
  isGithubCopilotProvider,
  resolveSubmitSettingsConfig,
  validateNonOfficialCredentials,
  validateProviderSpecificFields,
} from "./providerSubmitUtils";
import { BasicFormFields } from "./BasicFormFields";
import { ClaudeFormFields } from "./ClaudeFormFields";
import { CodexFormFields } from "./CodexFormFields";
import { GeminiFormFields } from "./GeminiFormFields";
import { OmoFormFields } from "./OmoFormFields";
import {
  ProviderAdvancedConfig,
  type PricingModelSourceOption,
} from "./ProviderAdvancedConfig";
import {
  useProviderCategory,
  useApiKeyState,
  useBaseUrlState,
  useModelState,
  useCodexConfigState,
  useApiKeyLink,
  useTemplateValues,
  useCommonConfigSnippet,
  useCodexCommonConfig,
  useSpeedTestEndpoints,
  useCodexTomlValidation,
  useGeminiConfigState,
  useGeminiCommonConfig,
  useOmoModelSource,
  useOpencodeFormState,
  useOmoDraftState,
  useOpenclawFormState,
  useCopilotAuth,
} from "./hooks";
import {
  CLAUDE_DEFAULT_CONFIG,
  CODEX_DEFAULT_CONFIG,
  GEMINI_DEFAULT_CONFIG,
  OPENCODE_DEFAULT_CONFIG,
  OPENCLAW_DEFAULT_CONFIG,
  normalizePricingSource,
} from "./helpers/opencodeFormUtils";
import { resolveManagedAccountId } from "@/lib/authBinding";
import { useOpenClawLiveProviderIds } from "@/hooks/useOpenClaw";

interface ProviderFormProps {
  appId: AppId;
  providerId?: string;
  submitLabel: string;
  onSubmit: (values: ProviderFormValues) => Promise<void> | void;
  onCancel: () => void;
  onUniversalPresetSelect?: (preset: UniversalProviderPreset) => void;
  onManageUniversalProviders?: () => void;
  onSubmittingChange?: (isSubmitting: boolean) => void;
  initialData?: {
    name?: string;
    websiteUrl?: string;
    notes?: string;
    settingsConfig?: Record<string, unknown>;
    category?: ProviderCategory;
    meta?: ProviderMeta;
    icon?: string;
    iconColor?: string;
  };
  showButtons?: boolean;
}

export function ProviderForm({
  appId,
  providerId,
  submitLabel,
  onSubmit,
  onCancel,
  onUniversalPresetSelect,
  onManageUniversalProviders,
  onSubmittingChange,
  initialData,
  showButtons = true,
}: ProviderFormProps) {
  const { t } = useTranslation();
  const isEditMode = Boolean(initialData);

  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(
    initialData ? null : "custom",
  );
  const [activePreset, setActivePreset] = useState<{
    id: string;
    category?: ProviderCategory;
    isPartner?: boolean;
    partnerPromotionKey?: string;
    suggestedDefaults?: OpenClawSuggestedDefaults;
  } | null>(null);
  const [isEndpointModalOpen, setIsEndpointModalOpen] = useState(false);
  const [isCodexEndpointModalOpen, setIsCodexEndpointModalOpen] =
    useState(false);

  const [draftCustomEndpoints, setDraftCustomEndpoints] = useState<string[]>(
    () => {
      if (initialData) return [];
      return [];
    },
  );
  const [endpointAutoSelect, setEndpointAutoSelect] = useState<boolean>(
    () => initialData?.meta?.endpointAutoSelect ?? true,
  );
  const supportsFullUrl = appId === "claude" || appId === "codex";
  const [localIsFullUrl, setLocalIsFullUrl] = useState<boolean>(() => {
    if (!supportsFullUrl) return false;
    return initialData?.meta?.isFullUrl ?? false;
  });

  const [testConfig, setTestConfig] = useState<ProviderTestConfig>(
    () => initialData?.meta?.testConfig ?? { enabled: false },
  );
  const [proxyConfig, setProxyConfig] = useState<ProviderProxyConfig>(
    () => initialData?.meta?.proxyConfig ?? { enabled: false },
  );
  const [pricingConfig, setPricingConfig] = useState<{
    enabled: boolean;
    costMultiplier?: string;
    pricingModelSource: PricingModelSourceOption;
  }>(() => ({
    enabled:
      initialData?.meta?.costMultiplier !== undefined ||
      initialData?.meta?.pricingModelSource !== undefined,
    costMultiplier: initialData?.meta?.costMultiplier,
    pricingModelSource: normalizePricingSource(
      initialData?.meta?.pricingModelSource,
    ),
  }));

  const { category } = useProviderCategory({
    appId,
    selectedPresetId,
    isEditMode,
    initialCategory: initialData?.category,
  });
  const isOmoCategory = appId === "opencode" && category === "omo";
  const isOmoSlimCategory = appId === "opencode" && category === "omo-slim";
  const isAnyOmoCategory = isOmoCategory || isOmoSlimCategory;

  useEffect(() => {
    setSelectedPresetId(initialData ? null : "custom");
    setActivePreset(null);

    if (!initialData) {
      setDraftCustomEndpoints([]);
    }
    setEndpointAutoSelect(initialData?.meta?.endpointAutoSelect ?? true);
    setLocalIsFullUrl(
      supportsFullUrl ? (initialData?.meta?.isFullUrl ?? false) : false,
    );
    setTestConfig(initialData?.meta?.testConfig ?? { enabled: false });
    setProxyConfig(initialData?.meta?.proxyConfig ?? { enabled: false });
    setPricingConfig({
      enabled:
        initialData?.meta?.costMultiplier !== undefined ||
        initialData?.meta?.pricingModelSource !== undefined,
      costMultiplier: initialData?.meta?.costMultiplier,
      pricingModelSource: normalizePricingSource(
        initialData?.meta?.pricingModelSource,
      ),
    });
  }, [appId, initialData, supportsFullUrl]);

  const defaultValues: ProviderFormData = useMemo(
    () => ({
      name: initialData?.name ?? "",
      websiteUrl: initialData?.websiteUrl ?? "",
      notes: initialData?.notes ?? "",
      settingsConfig: initialData?.settingsConfig
        ? JSON.stringify(initialData.settingsConfig, null, 2)
        : appId === "codex"
          ? CODEX_DEFAULT_CONFIG
          : appId === "gemini"
            ? GEMINI_DEFAULT_CONFIG
            : appId === "opencode"
              ? OPENCODE_DEFAULT_CONFIG
              : appId === "openclaw"
                ? OPENCLAW_DEFAULT_CONFIG
                : CLAUDE_DEFAULT_CONFIG,
      icon: initialData?.icon ?? "",
      iconColor: initialData?.iconColor ?? "",
    }),
    [initialData, appId],
  );

  const form = useForm<ProviderFormData>({
    resolver: zodResolver(providerSchema),
    defaultValues,
    mode: "onSubmit",
  });
  const { isSubmitting } = form.formState;

  const handleSettingsConfigChange = useCallback(
    (config: string) => {
      form.setValue("settingsConfig", config);
    },
    [form],
  );

  const [localApiKeyField, setLocalApiKeyField] = useState<ClaudeApiKeyField>(
    () => {
      if (appId !== "claude") return "ANTHROPIC_AUTH_TOKEN";
      if (initialData?.meta?.apiKeyField) return initialData.meta.apiKeyField;
      // Infer from existing config env
      const env = (initialData?.settingsConfig as Record<string, unknown>)
        ?.env as Record<string, unknown> | undefined;
      if (env?.ANTHROPIC_API_KEY !== undefined) return "ANTHROPIC_API_KEY";
      return "ANTHROPIC_AUTH_TOKEN";
    },
  );

  useEffect(() => {
    onSubmittingChange?.(isSubmitting);
  }, [isSubmitting, onSubmittingChange]);

  const {
    apiKey,
    handleApiKeyChange,
    showApiKey: shouldShowApiKey,
  } = useApiKeyState({
    initialConfig: form.getValues("settingsConfig"),
    onConfigChange: handleSettingsConfigChange,
    selectedPresetId,
    category,
    appType: appId,
    apiKeyField: appId === "claude" ? localApiKeyField : undefined,
  });

  const { baseUrl, handleClaudeBaseUrlChange } = useBaseUrlState({
    appType: appId,
    category,
    settingsConfig: form.getValues("settingsConfig"),
    codexConfig: "",
    onSettingsConfigChange: handleSettingsConfigChange,
    onCodexConfigChange: () => {},
  });

  const {
    claudeModel,
    reasoningModel,
    defaultHaikuModel,
    defaultSonnetModel,
    defaultOpusModel,
    handleModelChange,
  } = useModelState({
    settingsConfig: form.getValues("settingsConfig"),
    onConfigChange: handleSettingsConfigChange,
  });

  const [localApiFormat, setLocalApiFormat] = useState<ClaudeApiFormat>(() => {
    if (appId !== "claude") return "anthropic";
    return initialData?.meta?.apiFormat ?? "anthropic";
  });

  const handleApiFormatChange = useCallback((format: ClaudeApiFormat) => {
    setLocalApiFormat(format);
  }, []);

  const handleApiKeyFieldChange = useCallback(
    (field: ClaudeApiKeyField) => {
      const prev = localApiKeyField;
      setLocalApiKeyField(field);

      // Swap the env key name in settingsConfig
      try {
        const raw = form.getValues("settingsConfig");
        const config = JSON.parse(raw || "{}");
        if (config?.env && prev in config.env) {
          const value = config.env[prev];
          delete config.env[prev];
          config.env[field] = value;
          const updated = JSON.stringify(config, null, 2);
          form.setValue("settingsConfig", updated);
          handleSettingsConfigChange(updated);
        }
      } catch {
        // ignore parse errors during editing
      }
    },
    [localApiKeyField, form, handleSettingsConfigChange],
  );

  // Copilot OAuth 认证状态（仅 Claude 应用需要）
  const { isAuthenticated: isCopilotAuthenticated } = useCopilotAuth();

  // 选中的 GitHub 账号 ID（多账号支持）
  const [selectedGitHubAccountId, setSelectedGitHubAccountId] = useState<
    string | null
  >(() => resolveManagedAccountId(initialData?.meta, "github_copilot"));

  const {
    codexAuth,
    codexConfig,
    codexApiKey,
    codexBaseUrl,
    codexModelName,
    codexAuthError,
    setCodexAuth,
    handleCodexApiKeyChange,
    handleCodexBaseUrlChange,
    handleCodexModelNameChange,
    handleCodexConfigChange: originalHandleCodexConfigChange,
    resetCodexConfig,
  } = useCodexConfigState({ initialData });

  const { configError: codexConfigError, debouncedValidate } =
    useCodexTomlValidation();

  const handleCodexConfigChange = useCallback(
    (value: string) => {
      originalHandleCodexConfigChange(value);
      debouncedValidate(value);
    },
    [originalHandleCodexConfigChange, debouncedValidate],
  );

  useEffect(() => {
    if (appId === "codex" && !initialData && selectedPresetId === "custom") {
      const template = getCodexCustomTemplate();
      resetCodexConfig(template.auth, template.config);
    }
  }, [appId, initialData, selectedPresetId, resetCodexConfig]);

  useEffect(() => {
    form.reset(defaultValues);
  }, [defaultValues, form]);

  const presetCategoryLabels = useMemo(() => getPresetCategoryLabels(t), [t]);

  const presetEntries = useMemo(() => getPresetEntriesByApp(appId), [appId]);

  const {
    templateValues,
    templateValueEntries,
    selectedPreset: templatePreset,
    handleTemplateValueChange,
    validateTemplateValues,
  } = useTemplateValues({
    selectedPresetId: appId === "claude" ? selectedPresetId : null,
    presetEntries: appId === "claude" ? presetEntries : [],
    settingsConfig: form.getValues("settingsConfig"),
    onConfigChange: handleSettingsConfigChange,
  });

  const {
    useCommonConfig,
    commonConfigSnippet,
    commonConfigError,
    handleCommonConfigToggle,
    handleCommonConfigSnippetChange,
    isExtracting: isClaudeExtracting,
    handleExtract: handleClaudeExtract,
  } = useCommonConfigSnippet({
    settingsConfig: form.getValues("settingsConfig"),
    onConfigChange: handleSettingsConfigChange,
    initialData: appId === "claude" ? initialData : undefined,
    initialEnabled:
      appId === "claude" ? initialData?.meta?.commonConfigEnabled : undefined,
    selectedPresetId: selectedPresetId ?? undefined,
    enabled: appId === "claude",
  });

  const {
    useCommonConfig: useCodexCommonConfigFlag,
    commonConfigSnippet: codexCommonConfigSnippet,
    commonConfigError: codexCommonConfigError,
    handleCommonConfigToggle: handleCodexCommonConfigToggle,
    handleCommonConfigSnippetChange: handleCodexCommonConfigSnippetChange,
    isExtracting: isCodexExtracting,
    handleExtract: handleCodexExtract,
    clearCommonConfigError: clearCodexCommonConfigError,
  } = useCodexCommonConfig({
    codexConfig,
    onConfigChange: handleCodexConfigChange,
    initialData: appId === "codex" ? initialData : undefined,
    initialEnabled:
      appId === "codex" ? initialData?.meta?.commonConfigEnabled : undefined,
    selectedPresetId: selectedPresetId ?? undefined,
  });

  const {
    geminiEnv,
    geminiConfig,
    geminiApiKey,
    geminiBaseUrl,
    geminiModel,
    envError,
    configError: geminiConfigError,
    handleGeminiApiKeyChange: originalHandleGeminiApiKeyChange,
    handleGeminiBaseUrlChange: originalHandleGeminiBaseUrlChange,
    handleGeminiModelChange: originalHandleGeminiModelChange,
    handleGeminiEnvChange,
    handleGeminiConfigChange,
    resetGeminiConfig,
    envStringToObj,
    envObjToString,
  } = useGeminiConfigState({
    initialData: appId === "gemini" ? initialData : undefined,
  });

  const updateGeminiEnvField = useCallback(
    (
      key: "GEMINI_API_KEY" | "GOOGLE_GEMINI_BASE_URL" | "GEMINI_MODEL",
      value: string,
    ) => {
      try {
        const config = JSON.parse(form.getValues("settingsConfig") || "{}") as {
          env?: Record<string, unknown>;
        };
        if (!config.env || typeof config.env !== "object") {
          config.env = {};
        }
        config.env[key] = value;
        form.setValue("settingsConfig", JSON.stringify(config, null, 2));
      } catch {}
    },
    [form],
  );

  const handleGeminiApiKeyChange = useCallback(
    (key: string) => {
      originalHandleGeminiApiKeyChange(key);
      updateGeminiEnvField("GEMINI_API_KEY", key.trim());
    },
    [originalHandleGeminiApiKeyChange, updateGeminiEnvField],
  );

  const handleGeminiBaseUrlChange = useCallback(
    (url: string) => {
      originalHandleGeminiBaseUrlChange(url);
      updateGeminiEnvField(
        "GOOGLE_GEMINI_BASE_URL",
        url.trim().replace(/\/+$/, ""),
      );
    },
    [originalHandleGeminiBaseUrlChange, updateGeminiEnvField],
  );

  const handleGeminiModelChange = useCallback(
    (model: string) => {
      originalHandleGeminiModelChange(model);
      updateGeminiEnvField("GEMINI_MODEL", model.trim());
    },
    [originalHandleGeminiModelChange, updateGeminiEnvField],
  );

  const {
    useCommonConfig: useGeminiCommonConfigFlag,
    commonConfigSnippet: geminiCommonConfigSnippet,
    commonConfigError: geminiCommonConfigError,
    handleCommonConfigToggle: handleGeminiCommonConfigToggle,
    handleCommonConfigSnippetChange: handleGeminiCommonConfigSnippetChange,
    isExtracting: isGeminiExtracting,
    handleExtract: handleGeminiExtract,
    clearCommonConfigError: clearGeminiCommonConfigError,
  } = useGeminiCommonConfig({
    envValue: geminiEnv,
    onEnvChange: handleGeminiEnvChange,
    envStringToObj,
    envObjToString,
    initialData: appId === "gemini" ? initialData : undefined,
    initialEnabled:
      appId === "gemini" ? initialData?.meta?.commonConfigEnabled : undefined,
    selectedPresetId: selectedPresetId ?? undefined,
  });

  // ── Extracted hooks: OpenCode / OMO / OpenClaw ─────────────────────

  const {
    omoModelOptions,
    omoModelVariantsMap,
    omoPresetMetaMap,
    existingOpencodeKeys,
  } = useOmoModelSource({ isOmoCategory: isAnyOmoCategory, providerId });

  const {
    data: opencodeLiveProviderIds = [],
    isLoading: isOpencodeLiveProviderIdsLoading,
  } = useQuery({
    queryKey: ["opencodeLiveProviderIds"],
    queryFn: () => providersApi.getOpenCodeLiveProviderIds(),
    enabled: appId === "opencode" && !isAnyOmoCategory,
  });

  const opencodeForm = useOpencodeFormState({
    initialData,
    appId,
    providerId,
    onSettingsConfigChange: (config) => form.setValue("settingsConfig", config),
    getSettingsConfig: () => form.getValues("settingsConfig"),
  });

  const initialOmoSettings =
    appId === "opencode" &&
    (initialData?.category === "omo" || initialData?.category === "omo-slim")
      ? (initialData.settingsConfig as Record<string, unknown> | undefined)
      : undefined;

  const omoDraft = useOmoDraftState({
    initialOmoSettings,
    isEditMode,
    appId,
    category,
  });

  const openclawForm = useOpenclawFormState({
    initialData,
    appId,
    providerId,
    onSettingsConfigChange: (config) => form.setValue("settingsConfig", config),
    getSettingsConfig: () => form.getValues("settingsConfig"),
  });
  const {
    data: openclawLiveProviderIds = [],
    isLoading: isOpenclawLiveProviderIdsLoading,
  } = useOpenClawLiveProviderIds(appId === "openclaw");

  const additiveExistingProviderKeys = useMemo(() => {
    if (appId === "opencode" && !isAnyOmoCategory) {
      return Array.from(
        new Set(
          [...existingOpencodeKeys, ...opencodeLiveProviderIds].filter(
            (key) => key !== providerId,
          ),
        ),
      );
    }

    if (appId === "openclaw") {
      return Array.from(
        new Set(
          [
            ...openclawForm.existingOpenclawKeys,
            ...openclawLiveProviderIds,
          ].filter((key) => key !== providerId),
        ),
      );
    }

    return [];
  }, [
    appId,
    existingOpencodeKeys,
    isAnyOmoCategory,
    openclawForm.existingOpenclawKeys,
    openclawLiveProviderIds,
    opencodeLiveProviderIds,
    providerId,
  ]);

  const isProviderKeyLockStateLoading = useMemo(() => {
    if (!isEditMode) return false;
    if (appId === "opencode" && !isAnyOmoCategory) {
      return isOpencodeLiveProviderIdsLoading;
    }
    if (appId === "openclaw") {
      return isOpenclawLiveProviderIdsLoading;
    }
    return false;
  }, [
    appId,
    isAnyOmoCategory,
    isEditMode,
    isOpenclawLiveProviderIdsLoading,
    isOpencodeLiveProviderIdsLoading,
  ]);

  const isProviderKeyLocked = useMemo(() => {
    if (!isEditMode || !providerId) return false;
    if (appId === "opencode" && !isAnyOmoCategory) {
      return opencodeLiveProviderIds.includes(providerId);
    }
    if (appId === "openclaw") {
      return openclawLiveProviderIds.includes(providerId);
    }
    return false;
  }, [
    appId,
    isAnyOmoCategory,
    isEditMode,
    openclawLiveProviderIds,
    opencodeLiveProviderIds,
    providerId,
  ]);

  const [isCommonConfigModalOpen, setIsCommonConfigModalOpen] = useState(false);

  const handleSubmit = async (values: ProviderFormData) => {
    if (appId === "claude" && templateValueEntries.length > 0) {
      const validation = validateTemplateValues();
      if (!validation.isValid && validation.missingField) {
        toast.error(
          t("providerForm.fillParameter", {
            label: validation.missingField.label,
            defaultValue: `请填写 ${validation.missingField.label}`,
          }),
        );
        return;
      }
    }

    if (!values.name.trim()) {
      toast.error(
        t("providerForm.fillSupplierName", {
          defaultValue: "请填写供应商名称",
        }),
      );
      return;
    }

    const providerSpecificError = validateProviderSpecificFields({
      appId,
      isAnyOmoCategory,
      isProviderKeyLockStateLoading,
      isProviderKeyLocked,
      opencodeProviderKey: opencodeForm.opencodeProviderKey,
      openclawProviderKey: openclawForm.openclawProviderKey,
      additiveExistingProviderKeys,
      opencodeModels: opencodeForm.opencodeModels,
      t,
    });
    if (providerSpecificError) {
      toast.error(providerSpecificError);
      return;
    }

    // 非官方供应商必填校验：端点和 API Key
    // cloud_provider（如 Bedrock）通过模板变量处理认证，跳过通用校验
    // GitHub Copilot 使用 OAuth 认证，不需要 API Key
    const isCopilotProvider = isGithubCopilotProvider({
      templateProviderType: templatePreset?.providerType,
      initialProviderType: initialData?.meta?.providerType,
      baseUrl,
    });
    const credentialError = validateNonOfficialCredentials({
      appId,
      category,
      isCopilotProvider,
      isCopilotAuthenticated,
      baseUrl,
      apiKey,
      codexBaseUrl,
      codexApiKey,
      geminiBaseUrl,
      geminiApiKey,
      t,
    });
    if (credentialError) {
      toast.error(credentialError);
      return;
    }

    const settingsConfigResult = resolveSubmitSettingsConfig({
      appId,
      category,
      rawSettingsConfig: values.settingsConfig,
      codexAuth,
      codexConfig,
      geminiEnv,
      geminiConfig,
      envStringToObj,
      omoAgents: omoDraft.omoAgents,
      omoCategories: omoDraft.omoCategories,
      omoOtherFieldsStr: omoDraft.omoOtherFieldsStr,
      t,
    });
    if (!settingsConfigResult.ok) {
      toast.error(settingsConfigResult.errorMessage);
      return;
    }

    const payload: ProviderFormValues = {
      ...values,
      name: values.name.trim(),
      websiteUrl: values.websiteUrl?.trim() ?? "",
      settingsConfig: settingsConfigResult.settingsConfig,
    };

    if (appId === "opencode") {
      if (isAnyOmoCategory) {
        if (!isEditMode) {
          const prefix = category === "omo" ? "omo" : "omo-slim";
          payload.providerKey = `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
        }
      } else {
        payload.providerKey = opencodeForm.opencodeProviderKey;
      }
    } else if (appId === "openclaw") {
      payload.providerKey = openclawForm.openclawProviderKey;
    }

    if (isAnyOmoCategory && !payload.presetCategory) {
      payload.presetCategory = category;
    }

    if (activePreset) {
      payload.presetId = activePreset.id;
      if (activePreset.category) {
        payload.presetCategory = activePreset.category;
      }
      if (activePreset.isPartner) {
        payload.isPartner = activePreset.isPartner;
      }
      // OpenClaw: 传递预设的 suggestedDefaults 到提交数据
      if (activePreset.suggestedDefaults) {
        payload.suggestedDefaults = activePreset.suggestedDefaults;
      }
    }

    if (!isEditMode && draftCustomEndpoints.length > 0) {
      const customEndpointsToSave: Record<
        string,
        import("@/types").CustomEndpoint
      > = draftCustomEndpoints.reduce(
        (acc, url) => {
          const now = Date.now();
          acc[url] = { url, addedAt: now, lastUsed: undefined };
          return acc;
        },
        {} as Record<string, import("@/types").CustomEndpoint>,
      );

      const hadEndpoints =
        initialData?.meta?.custom_endpoints &&
        Object.keys(initialData.meta.custom_endpoints).length > 0;
      const needsClearEndpoints =
        hadEndpoints && draftCustomEndpoints.length === 0;

      let mergedMeta = needsClearEndpoints
        ? mergeProviderMeta(initialData?.meta, {})
        : mergeProviderMeta(initialData?.meta, customEndpointsToSave);

      if (activePreset?.isPartner) {
        mergedMeta = {
          ...(mergedMeta ?? {}),
          isPartner: true,
        };
      }

      if (activePreset?.partnerPromotionKey) {
        mergedMeta = {
          ...(mergedMeta ?? {}),
          partnerPromotionKey: activePreset.partnerPromotionKey,
        };
      }

      if (mergedMeta !== undefined) {
        payload.meta = mergedMeta;
      }
    }

    const baseMeta: ProviderMeta | undefined =
      payload.meta ?? (initialData?.meta ? { ...initialData.meta } : undefined);

    // 确定 providerType（新建时从预设获取，编辑时从现有数据获取）
    const providerType =
      templatePreset?.providerType || initialData?.meta?.providerType;

    payload.meta = {
      ...(baseMeta ?? {}),
      commonConfigEnabled:
        appId === "claude"
          ? useCommonConfig
          : appId === "codex"
            ? useCodexCommonConfigFlag
            : appId === "gemini"
              ? useGeminiCommonConfigFlag
              : undefined,
      endpointAutoSelect,
      // 保存 providerType（用于识别 Copilot 等特殊供应商）
      providerType,
      authBinding: isCopilotProvider
        ? {
            source: "managed_account",
            authProvider: "github_copilot",
            accountId: selectedGitHubAccountId ?? undefined,
          }
        : undefined,
      // GitHub Copilot 多账号：保存关联的账号 ID
      githubAccountId:
        isCopilotProvider && selectedGitHubAccountId
          ? selectedGitHubAccountId
          : undefined,
      testConfig: testConfig.enabled ? testConfig : undefined,
      proxyConfig: proxyConfig.enabled ? proxyConfig : undefined,
      costMultiplier: pricingConfig.enabled
        ? pricingConfig.costMultiplier
        : undefined,
      pricingModelSource:
        pricingConfig.enabled && pricingConfig.pricingModelSource !== "inherit"
          ? pricingConfig.pricingModelSource
          : undefined,
      apiFormat:
        appId === "claude" && category !== "official"
          ? localApiFormat
          : undefined,
      apiKeyField:
        appId === "claude" &&
        category !== "official" &&
        localApiKeyField !== "ANTHROPIC_AUTH_TOKEN"
          ? localApiKeyField
          : undefined,
      isFullUrl:
        supportsFullUrl && category !== "official" && localIsFullUrl
          ? true
          : undefined,
    };

    await onSubmit(payload);
  };

  const groupedPresets = useMemo(
    () => groupPresetEntries(presetEntries),
    [presetEntries],
  );

  const categoryKeys = useMemo(
    () => getPresetCategoryKeys(groupedPresets),
    [groupedPresets],
  );

  const shouldShowSpeedTest =
    category !== "official" && category !== "cloud_provider";

  const {
    shouldShowApiKeyLink: shouldShowClaudeApiKeyLink,
    websiteUrl: claudeWebsiteUrl,
    isPartner: isClaudePartner,
    partnerPromotionKey: claudePartnerPromotionKey,
  } = useApiKeyLink({
    appId: "claude",
    category,
    selectedPresetId,
    presetEntries,
    formWebsiteUrl: form.watch("websiteUrl") || "",
  });

  const {
    shouldShowApiKeyLink: shouldShowCodexApiKeyLink,
    websiteUrl: codexWebsiteUrl,
    isPartner: isCodexPartner,
    partnerPromotionKey: codexPartnerPromotionKey,
  } = useApiKeyLink({
    appId: "codex",
    category,
    selectedPresetId,
    presetEntries,
    formWebsiteUrl: form.watch("websiteUrl") || "",
  });

  const {
    shouldShowApiKeyLink: shouldShowGeminiApiKeyLink,
    websiteUrl: geminiWebsiteUrl,
    isPartner: isGeminiPartner,
    partnerPromotionKey: geminiPartnerPromotionKey,
  } = useApiKeyLink({
    appId: "gemini",
    category,
    selectedPresetId,
    presetEntries,
    formWebsiteUrl: form.watch("websiteUrl") || "",
  });

  const {
    shouldShowApiKeyLink: shouldShowOpencodeApiKeyLink,
    websiteUrl: opencodeWebsiteUrl,
    isPartner: isOpencodePartner,
    partnerPromotionKey: opencodePartnerPromotionKey,
  } = useApiKeyLink({
    appId: "opencode",
    category,
    selectedPresetId,
    presetEntries,
    formWebsiteUrl: form.watch("websiteUrl") || "",
  });

  // 使用 API Key 链接 hook (OpenClaw)
  const {
    shouldShowApiKeyLink: shouldShowOpenclawApiKeyLink,
    websiteUrl: openclawWebsiteUrl,
    isPartner: isOpenclawPartner,
    partnerPromotionKey: openclawPartnerPromotionKey,
  } = useApiKeyLink({
    appId: "openclaw",
    category,
    selectedPresetId,
    presetEntries,
    formWebsiteUrl: form.watch("websiteUrl") || "",
  });

  // 使用端点测速候选 hook
  const speedTestEndpoints = useSpeedTestEndpoints({
    appId,
    selectedPresetId,
    presetEntries,
    baseUrl,
    codexBaseUrl,
    initialData,
  });

  const applyPresetSelection = useCallback(
    (selection: PresetSelectionResult) => {
      if (selection.appId === "codex") {
        resetCodexConfig(selection.codexAuth, selection.codexConfig);
        form.reset(selection.formValues);
        return;
      }

      if (selection.appId === "gemini") {
        resetGeminiConfig(selection.geminiEnv, selection.geminiConfig);
        form.reset(selection.formValues);
        return;
      }

      if (selection.appId === "opencode") {
        if (selection.isOmoPreset) {
          omoDraft.resetOmoDraftState();
        } else {
          opencodeForm.resetOpencodeState(selection.opencodeConfig);
        }
        form.reset(selection.formValues);
        return;
      }

      if (selection.appId === "openclaw") {
        openclawForm.resetOpenclawState(selection.openclawConfig);
        form.reset(selection.formValues);
        return;
      }

      setLocalApiFormat(selection.apiFormat);
      setLocalApiKeyField(selection.apiKeyField);
      setLocalIsFullUrl(selection.isFullUrl);
      form.reset(selection.formValues);
    },
    [
      form,
      omoDraft,
      openclawForm,
      opencodeForm,
      resetCodexConfig,
      resetGeminiConfig,
      setLocalApiFormat,
      setLocalApiKeyField,
      setLocalIsFullUrl,
    ],
  );

  const handlePresetChange = (value: string) => {
    setSelectedPresetId(value);
    if (value === "custom") {
      setActivePreset(null);
      form.reset(defaultValues);

      const customPresetResetPlan = getCustomPresetResetPlan(appId);
      if (customPresetResetPlan.codex) {
        resetCodexConfig(
          customPresetResetPlan.codex.auth,
          customPresetResetPlan.codex.config,
        );
      }
      if (customPresetResetPlan.shouldResetGemini) {
        resetGeminiConfig({}, {});
      }
      if (customPresetResetPlan.shouldResetOpencode) {
        opencodeForm.resetOpencodeState();
      }
      if (customPresetResetPlan.shouldResetOmoDraft) {
        omoDraft.resetOmoDraftState();
      }
      if (customPresetResetPlan.shouldResetOpenclaw) {
        openclawForm.resetOpenclawState();
      }
      return;
    }

    const entry = presetEntries.find((item) => item.id === value);
    if (!entry) {
      return;
    }

    const selection = resolvePresetSelection({
      appId,
      presetId: value,
      entry,
      t,
    });

    setActivePreset(selection.activePreset);
    applyPresetSelection(selection);
  };

  const settingsConfigErrorField = (
    <FormField
      control={form.control}
      name="settingsConfig"
      render={() => (
        <FormItem className="space-y-0">
          <FormMessage />
        </FormItem>
      )}
    />
  );

  return (
    <Form {...form}>
      <form
        id="provider-form"
        onSubmit={form.handleSubmit(handleSubmit)}
        className="space-y-6 glass rounded-xl p-6 border border-white/10"
      >
        {!initialData && (
          <ProviderPresetSelector
            selectedPresetId={selectedPresetId}
            groupedPresets={groupedPresets}
            categoryKeys={categoryKeys}
            presetCategoryLabels={presetCategoryLabels}
            onPresetChange={handlePresetChange}
            onUniversalPresetSelect={onUniversalPresetSelect}
            onManageUniversalProviders={onManageUniversalProviders}
            category={category}
          />
        )}

        <BasicFormFields
          form={form}
          beforeNameSlot={
            appId === "opencode" && !isAnyOmoCategory ? (
              <ProviderKeyField
                inputId="opencode-key"
                label={t("opencode.providerKey")}
                value={opencodeForm.opencodeProviderKey}
                placeholder={t("opencode.providerKeyPlaceholder")}
                existingKeys={additiveExistingProviderKeys}
                isEditMode={isEditMode}
                isLocked={isProviderKeyLocked}
                isLoading={isProviderKeyLockStateLoading}
                duplicateMessage={t("opencode.providerKeyDuplicate")}
                invalidMessage={t("opencode.providerKeyInvalid")}
                hintMessage={t("opencode.providerKeyHint")}
                lockedHintMessage={t("opencode.providerKeyLockedHint", {
                  defaultValue:
                    "该供应商已添加到应用配置中，供应商标识不可修改",
                })}
                loadingMessage={t("providerForm.providerKeyStatusLoading", {
                  defaultValue: "正在加载供应商标识状态，请稍后再试",
                })}
                onChange={opencodeForm.setOpencodeProviderKey}
              />
            ) : appId === "openclaw" ? (
              <ProviderKeyField
                inputId="openclaw-key"
                label={t("openclaw.providerKey")}
                value={openclawForm.openclawProviderKey}
                placeholder={t("openclaw.providerKeyPlaceholder")}
                existingKeys={additiveExistingProviderKeys}
                isEditMode={isEditMode}
                isLocked={isProviderKeyLocked}
                isLoading={isProviderKeyLockStateLoading}
                duplicateMessage={t("openclaw.providerKeyDuplicate")}
                invalidMessage={t("openclaw.providerKeyInvalid")}
                hintMessage={t("openclaw.providerKeyHint")}
                lockedHintMessage={t("openclaw.providerKeyLockedHint", {
                  defaultValue:
                    "该供应商已添加到应用配置中，供应商标识不可修改",
                })}
                loadingMessage={t("providerForm.providerKeyStatusLoading", {
                  defaultValue: "正在加载供应商标识状态，请稍后再试",
                })}
                onChange={openclawForm.setOpenclawProviderKey}
              />
            ) : undefined
          }
        />

        {appId === "claude" && (
          <ClaudeFormFields
            providerId={providerId}
            shouldShowApiKey={
              (category !== "cloud_provider" ||
                hasApiKeyField(form.getValues("settingsConfig"), "claude")) &&
              shouldShowApiKey(form.getValues("settingsConfig"), isEditMode)
            }
            apiKey={apiKey}
            onApiKeyChange={handleApiKeyChange}
            category={category}
            shouldShowApiKeyLink={shouldShowClaudeApiKeyLink}
            websiteUrl={claudeWebsiteUrl}
            isPartner={isClaudePartner}
            partnerPromotionKey={claudePartnerPromotionKey}
            isCopilotPreset={
              templatePreset?.providerType === "github_copilot" ||
              initialData?.meta?.providerType === "github_copilot" ||
              baseUrl.includes("githubcopilot.com")
            }
            usesOAuth={
              templatePreset?.requiresOAuth === true ||
              templatePreset?.providerType === "github_copilot" ||
              initialData?.meta?.providerType === "github_copilot" ||
              baseUrl.includes("githubcopilot.com")
            }
            isCopilotAuthenticated={isCopilotAuthenticated}
            selectedGitHubAccountId={selectedGitHubAccountId}
            onGitHubAccountSelect={setSelectedGitHubAccountId}
            templateValueEntries={templateValueEntries}
            templateValues={templateValues}
            templatePresetName={templatePreset?.name || ""}
            onTemplateValueChange={handleTemplateValueChange}
            shouldShowSpeedTest={shouldShowSpeedTest}
            baseUrl={baseUrl}
            onBaseUrlChange={handleClaudeBaseUrlChange}
            isEndpointModalOpen={isEndpointModalOpen}
            onEndpointModalToggle={setIsEndpointModalOpen}
            onCustomEndpointsChange={
              isEditMode ? undefined : setDraftCustomEndpoints
            }
            autoSelect={endpointAutoSelect}
            onAutoSelectChange={setEndpointAutoSelect}
            shouldShowModelSelector={category !== "official"}
            claudeModel={claudeModel}
            reasoningModel={reasoningModel}
            defaultHaikuModel={defaultHaikuModel}
            defaultSonnetModel={defaultSonnetModel}
            defaultOpusModel={defaultOpusModel}
            onModelChange={handleModelChange}
            speedTestEndpoints={speedTestEndpoints}
            apiFormat={localApiFormat}
            onApiFormatChange={handleApiFormatChange}
            apiKeyField={localApiKeyField}
            onApiKeyFieldChange={handleApiKeyFieldChange}
            isFullUrl={localIsFullUrl}
            onFullUrlChange={setLocalIsFullUrl}
          />
        )}

        {appId === "codex" && (
          <CodexFormFields
            providerId={providerId}
            codexApiKey={codexApiKey}
            onApiKeyChange={handleCodexApiKeyChange}
            category={category}
            shouldShowApiKeyLink={shouldShowCodexApiKeyLink}
            websiteUrl={codexWebsiteUrl}
            isPartner={isCodexPartner}
            partnerPromotionKey={codexPartnerPromotionKey}
            shouldShowSpeedTest={shouldShowSpeedTest}
            codexBaseUrl={codexBaseUrl}
            onBaseUrlChange={handleCodexBaseUrlChange}
            isFullUrl={localIsFullUrl}
            onFullUrlChange={setLocalIsFullUrl}
            isEndpointModalOpen={isCodexEndpointModalOpen}
            onEndpointModalToggle={setIsCodexEndpointModalOpen}
            onCustomEndpointsChange={
              isEditMode ? undefined : setDraftCustomEndpoints
            }
            autoSelect={endpointAutoSelect}
            onAutoSelectChange={setEndpointAutoSelect}
            shouldShowModelField={category !== "official"}
            modelName={codexModelName}
            onModelNameChange={handleCodexModelNameChange}
            speedTestEndpoints={speedTestEndpoints}
          />
        )}

        {appId === "gemini" && (
          <GeminiFormFields
            providerId={providerId}
            shouldShowApiKey={shouldShowApiKey(
              form.getValues("settingsConfig"),
              isEditMode,
            )}
            apiKey={geminiApiKey}
            onApiKeyChange={handleGeminiApiKeyChange}
            category={category}
            shouldShowApiKeyLink={shouldShowGeminiApiKeyLink}
            websiteUrl={geminiWebsiteUrl}
            isPartner={isGeminiPartner}
            partnerPromotionKey={geminiPartnerPromotionKey}
            shouldShowSpeedTest={shouldShowSpeedTest}
            baseUrl={geminiBaseUrl}
            onBaseUrlChange={handleGeminiBaseUrlChange}
            isEndpointModalOpen={isEndpointModalOpen}
            onEndpointModalToggle={setIsEndpointModalOpen}
            onCustomEndpointsChange={setDraftCustomEndpoints}
            autoSelect={endpointAutoSelect}
            onAutoSelectChange={setEndpointAutoSelect}
            shouldShowModelField={true}
            model={geminiModel}
            onModelChange={handleGeminiModelChange}
            speedTestEndpoints={speedTestEndpoints}
          />
        )}

        {appId === "opencode" && !isAnyOmoCategory && (
          <OpenCodeFormFields
            npm={opencodeForm.opencodeNpm}
            onNpmChange={opencodeForm.handleOpencodeNpmChange}
            apiKey={opencodeForm.opencodeApiKey}
            onApiKeyChange={opencodeForm.handleOpencodeApiKeyChange}
            category={category}
            shouldShowApiKeyLink={shouldShowOpencodeApiKeyLink}
            websiteUrl={opencodeWebsiteUrl}
            isPartner={isOpencodePartner}
            partnerPromotionKey={opencodePartnerPromotionKey}
            baseUrl={opencodeForm.opencodeBaseUrl}
            onBaseUrlChange={opencodeForm.handleOpencodeBaseUrlChange}
            models={opencodeForm.opencodeModels}
            onModelsChange={opencodeForm.handleOpencodeModelsChange}
            extraOptions={opencodeForm.opencodeExtraOptions}
            onExtraOptionsChange={opencodeForm.handleOpencodeExtraOptionsChange}
          />
        )}

        {appId === "opencode" &&
          (category === "omo" || category === "omo-slim") && (
            <OmoFormFields
              modelOptions={omoModelOptions}
              modelVariantsMap={omoModelVariantsMap}
              presetMetaMap={omoPresetMetaMap}
              agents={omoDraft.omoAgents}
              onAgentsChange={omoDraft.setOmoAgents}
              categories={
                category === "omo" ? omoDraft.omoCategories : undefined
              }
              onCategoriesChange={
                category === "omo" ? omoDraft.setOmoCategories : undefined
              }
              otherFieldsStr={omoDraft.omoOtherFieldsStr}
              onOtherFieldsStrChange={omoDraft.setOmoOtherFieldsStr}
              isSlim={category === "omo-slim"}
            />
          )}

        {/* OpenClaw 专属字段 */}
        {appId === "openclaw" && (
          <OpenClawFormFields
            baseUrl={openclawForm.openclawBaseUrl}
            onBaseUrlChange={openclawForm.handleOpenclawBaseUrlChange}
            apiKey={openclawForm.openclawApiKey}
            onApiKeyChange={openclawForm.handleOpenclawApiKeyChange}
            category={category}
            shouldShowApiKeyLink={shouldShowOpenclawApiKeyLink}
            websiteUrl={openclawWebsiteUrl}
            isPartner={isOpenclawPartner}
            partnerPromotionKey={openclawPartnerPromotionKey}
            api={openclawForm.openclawApi}
            onApiChange={openclawForm.handleOpenclawApiChange}
            models={openclawForm.openclawModels}
            onModelsChange={openclawForm.handleOpenclawModelsChange}
            userAgent={openclawForm.openclawUserAgent}
            onUserAgentChange={openclawForm.handleOpenclawUserAgentChange}
          />
        )}

        {/* 配置编辑器：Codex、Claude、Gemini 分别使用不同的编辑器 */}
        {appId === "codex" ? (
          <>
            <CodexConfigEditor
              authValue={codexAuth}
              configValue={codexConfig}
              onAuthChange={setCodexAuth}
              onConfigChange={handleCodexConfigChange}
              useCommonConfig={useCodexCommonConfigFlag}
              onCommonConfigToggle={handleCodexCommonConfigToggle}
              commonConfigSnippet={codexCommonConfigSnippet}
              onCommonConfigSnippetChange={handleCodexCommonConfigSnippetChange}
              onCommonConfigErrorClear={clearCodexCommonConfigError}
              commonConfigError={codexCommonConfigError}
              authError={codexAuthError}
              configError={codexConfigError}
              onExtract={handleCodexExtract}
              isExtracting={isCodexExtracting}
            />
            {settingsConfigErrorField}
          </>
        ) : appId === "gemini" ? (
          <>
            <GeminiConfigEditor
              envValue={geminiEnv}
              configValue={geminiConfig}
              onEnvChange={handleGeminiEnvChange}
              onConfigChange={handleGeminiConfigChange}
              useCommonConfig={useGeminiCommonConfigFlag}
              onCommonConfigToggle={handleGeminiCommonConfigToggle}
              commonConfigSnippet={geminiCommonConfigSnippet}
              onCommonConfigSnippetChange={
                handleGeminiCommonConfigSnippetChange
              }
              onCommonConfigErrorClear={clearGeminiCommonConfigError}
              commonConfigError={geminiCommonConfigError}
              envError={envError}
              configError={geminiConfigError}
              onExtract={handleGeminiExtract}
              isExtracting={isGeminiExtracting}
            />
            {settingsConfigErrorField}
          </>
        ) : appId === "opencode" &&
          (category === "omo" || category === "omo-slim") ? (
          <div className="space-y-2">
            <Label>{t("provider.configJson")}</Label>
            <JsonEditor
              value={omoDraft.mergedOmoJsonPreview}
              onChange={() => {}}
              rows={14}
              showValidation={false}
              language="json"
            />
          </div>
        ) : appId === "opencode" &&
          category !== "omo" &&
          category !== "omo-slim" ? (
          <>
            <div className="space-y-2">
              <Label htmlFor="settingsConfig">{t("provider.configJson")}</Label>
              <JsonEditor
                value={form.getValues("settingsConfig")}
                onChange={(config) => form.setValue("settingsConfig", config)}
                placeholder={`{
  "npm": "@ai-sdk/openai-compatible",
  "options": {
    "baseURL": "https://your-api-endpoint.com",
    "apiKey": "your-api-key-here"
  },
  "models": {}
}`}
                rows={14}
                showValidation={true}
                language="json"
              />
            </div>
            {settingsConfigErrorField}
          </>
        ) : appId === "openclaw" ? (
          <>
            <div className="space-y-2">
              <Label htmlFor="settingsConfig">{t("provider.configJson")}</Label>
              <JsonEditor
                value={form.getValues("settingsConfig")}
                onChange={(config) => form.setValue("settingsConfig", config)}
                placeholder={`{
  "baseUrl": "https://api.example.com/v1",
  "apiKey": "your-api-key-here",
  "api": "openai-completions",
  "models": []
}`}
                rows={14}
                showValidation={true}
                language="json"
              />
            </div>
            <FormField
              control={form.control}
              name="settingsConfig"
              render={() => (
                <FormItem className="space-y-0">
                  <FormMessage />
                </FormItem>
              )}
            />
          </>
        ) : (
          <>
            <CommonConfigEditor
              value={form.getValues("settingsConfig")}
              onChange={(value) => form.setValue("settingsConfig", value)}
              useCommonConfig={useCommonConfig}
              onCommonConfigToggle={handleCommonConfigToggle}
              commonConfigSnippet={commonConfigSnippet}
              onCommonConfigSnippetChange={handleCommonConfigSnippetChange}
              commonConfigError={commonConfigError}
              onEditClick={() => setIsCommonConfigModalOpen(true)}
              isModalOpen={isCommonConfigModalOpen}
              onModalClose={() => setIsCommonConfigModalOpen(false)}
              onExtract={handleClaudeExtract}
              isExtracting={isClaudeExtracting}
            />
            {settingsConfigErrorField}
          </>
        )}

        {!isAnyOmoCategory && appId !== "opencode" && appId !== "openclaw" && (
          <ProviderAdvancedConfig
            testConfig={testConfig}
            proxyConfig={proxyConfig}
            pricingConfig={pricingConfig}
            onTestConfigChange={setTestConfig}
            onProxyConfigChange={setProxyConfig}
            onPricingConfigChange={setPricingConfig}
          />
        )}

        {showButtons && (
          <div className="flex justify-end gap-2">
            <Button variant="outline" type="button" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {submitLabel}
            </Button>
          </div>
        )}
      </form>
    </Form>
  );
}

export type ProviderFormValues = ProviderFormData & {
  presetId?: string;
  presetCategory?: ProviderCategory;
  isPartner?: boolean;
  meta?: ProviderMeta;
  providerKey?: string; // OpenCode/OpenClaw: user-defined provider key
  suggestedDefaults?: OpenClawSuggestedDefaults; // OpenClaw: suggested default model configuration
};
