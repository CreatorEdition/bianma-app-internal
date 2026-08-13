import { useEffect, useMemo, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { motion, AnimatePresence } from "framer-motion";
import { useQueryClient } from "@tanstack/react-query";
import {
  Plus,
  Settings,
  ArrowLeft,
  Book,
  Wrench,
  RefreshCw,
  History,
  Download,
  FolderArchive,
  Search,
  FolderOpen,
  KeyRound,
  Shield,
  Cpu,
  Home,
  Boxes,
  Route,
  Gauge,
  PanelLeft,
} from "lucide-react";
import type { Provider, VisibleApps } from "@/types";
import type { EnvConflict } from "@/types/env";
import { useProvidersQuery, useSettingsQuery } from "@/lib/query";
import type { AppId } from "@/lib/api";
import { useProviderActions } from "@/hooks/useProviderActions";
import { useOpenClawHealth } from "@/hooks/useOpenClaw";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { useAutoCompact } from "@/hooks/useAutoCompact";
import { useAppUiState } from "@/hooks/useAppUiState";
import { useAppViewGuards } from "@/hooks/useAppViewGuards";
import { useEnvBannerActions } from "@/hooks/useEnvBannerActions";
import { useAppEventSubscriptions } from "@/hooks/useAppEventSubscriptions";
import { useAppStartupChecks } from "@/hooks/useAppStartupChecks";
import { useProviderOmoActions } from "@/hooks/useProviderOmoActions";
import { useProviderWorkspaceActions } from "@/hooks/useProviderWorkspaceActions";
import {
  useAppKeyboardShortcuts,
  type AppKeyboardShortcutView,
} from "@/hooks/useAppKeyboardShortcuts";
import { cn } from "@/lib/utils";
import { isWindows, isLinux } from "@/lib/platform";
import { BIANMA_DISPLAY_NAME, BIANMA_GITHUB_REPOSITORY_URL } from "@/lib/brand";
import {
  readCompatibleStorage,
  writeCompatibleStorage,
} from "@/lib/storageCompat";
import {
  LAST_APP_LEGACY_STORAGE_KEYS,
  LAST_APP_STORAGE_KEY,
  LAST_VIEW_LEGACY_STORAGE_KEYS,
  LAST_VIEW_STORAGE_KEY,
} from "@/lib/storageKeys";
import { AppSwitcher } from "@/components/AppSwitcher";
import { ProviderWorkspacePanel } from "@/components/providers/ProviderWorkspacePanel";
import { RouteCenterDashboard } from "@/components/control-plane/RouteCenterDashboard";
import { RouteCenterPanel } from "@/components/control-plane/RouteCenterPanel";
import { AddProviderDialog } from "@/components/providers/AddProviderDialog";
import { EditProviderDialog } from "@/components/providers/EditProviderDialog";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { UpdateBadge } from "@/components/UpdateBadge";
import { EnvWarningBanner } from "@/components/env/EnvWarningBanner";
import { ProxyToggle } from "@/components/proxy/ProxyToggle";
import { FailoverToggle } from "@/components/proxy/FailoverToggle";
import UsageScriptModal from "@/components/UsageScriptModal";
import UnifiedMcpPanel from "@/components/mcp/UnifiedMcpPanel";
import PromptPanel from "@/components/prompts/PromptPanel";
import { SkillsPage } from "@/components/skills/SkillsPage";
import UnifiedSkillsPanel from "@/components/skills/UnifiedSkillsPanel";
import { DeepLinkImportDialog } from "@/components/DeepLinkImportDialog";
import { AgentsPanel } from "@/components/agents/AgentsPanel";
import { UniversalProviderPanel } from "@/components/universal";
import { McpIcon } from "@/components/BrandIcons";
import { Button } from "@/components/ui/button";
import { SessionManagerPage } from "@/components/sessions/SessionManagerPage";
import WorkspaceFilesPanel from "@/components/workspace/WorkspaceFilesPanel";
import EnvPanel from "@/components/openclaw/EnvPanel";
import ToolsPanel from "@/components/openclaw/ToolsPanel";
import AgentsDefaultsPanel from "@/components/openclaw/AgentsDefaultsPanel";
import OpenClawHealthBanner from "@/components/openclaw/OpenClawHealthBanner";
import { UsageDashboard } from "@/components/usage/UsageDashboard";

type View = AppKeyboardShortcutView;

const DRAG_BAR_HEIGHT = isWindows() || isLinux() ? 0 : 28; // px
const HEADER_HEIGHT = 44; // px
const SIDEBAR_WIDTH = 188; // px
const CONTENT_TOP_OFFSET = DRAG_BAR_HEIGHT + HEADER_HEIGHT;

const VALID_APPS: AppId[] = [
  "claude",
  "codex",
  "gemini",
  "opencode",
  "openclaw",
];

const getInitialApp = (): AppId => {
  const saved = readCompatibleStorage(LAST_APP_STORAGE_KEY, [
    ...LAST_APP_LEGACY_STORAGE_KEYS,
  ]) as AppId | null;
  if (saved && VALID_APPS.includes(saved)) {
    return saved;
  }
  return "claude";
};

const VALID_VIEWS: View[] = [
  "home",
  "services",
  "strategy",
  "stats",
  "providers",
  "settings",
  "prompts",
  "skills",
  "skillsDiscovery",
  "mcp",
  "agents",
  "universal",
  "sessions",
  "workspace",
  "openclawEnv",
  "openclawTools",
  "openclawAgents",
];

const getInitialView = (): View => {
  if (import.meta.env.DEV) {
    const previewView = new URLSearchParams(window.location.search).get("view");
    if (previewView && VALID_VIEWS.includes(previewView as View)) {
      return previewView as View;
    }
  }

  const saved = readCompatibleStorage(LAST_VIEW_STORAGE_KEY, [
    ...LAST_VIEW_LEGACY_STORAGE_KEYS,
  ]) as View | null;
  if (saved && VALID_VIEWS.includes(saved)) {
    return saved === "providers" ? "services" : saved;
  }
  return "home";
};

function App() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const [activeApp, setActiveApp] = useState<AppId>(getInitialApp);
  const [currentView, setCurrentView] = useState<View>(getInitialView);
  const {
    settingsDefaultTab,
    isAddOpen,
    editingProvider,
    usageProvider,
    confirmAction,
    effectiveEditingProvider,
    effectiveUsageProvider,
    openGeneralSettings,
    openProxySettings,
    openAboutSettings,
    openAddDialog,
    handleAddDialogOpenChange,
    openEditDialog,
    handleEditDialogOpenChange,
    openUsageModal,
    closeUsageModal,
    openDeleteConfirm,
    openRemoveConfirm,
    clearConfirmAction,
  } = useAppUiState({ setCurrentView });

  useEffect(() => {
    writeCompatibleStorage(LAST_VIEW_STORAGE_KEY, currentView, [
      ...LAST_VIEW_LEGACY_STORAGE_KEYS,
    ]);
  }, [currentView]);

  const { data: settingsData } = useSettingsQuery();
  const visibleApps: VisibleApps = settingsData?.visibleApps ?? {
    claude: true,
    codex: true,
    gemini: true,
    opencode: true,
    openclaw: true,
  };

  useAppViewGuards({
    activeApp,
    currentView,
    visibleApps,
    setActiveApp,
    setCurrentView,
  });

  const [envConflicts, setEnvConflicts] = useState<EnvConflict[]>([]);
  const [showEnvBanner, setShowEnvBanner] = useState(false);
  const { handleEnvBannerDismiss, handleEnvBannerDeleted } =
    useEnvBannerActions({
      setEnvConflicts,
      setShowEnvBanner,
    });

  const toolbarRef = useRef<HTMLDivElement>(null);
  const isToolbarCompact = useAutoCompact(toolbarRef);

  const promptPanelRef = useRef<any>(null);
  const mcpPanelRef = useRef<any>(null);
  const skillsPageRef = useRef<any>(null);
  const unifiedSkillsPanelRef = useRef<any>(null);
  const addActionButtonClass =
    "bg-orange-500 hover:bg-orange-600 dark:bg-orange-500 dark:hover:bg-orange-600 text-white shadow-lg shadow-orange-500/30 dark:shadow-orange-500/40 rounded-full w-8 h-8";

  const {
    isRunning: isProxyRunning,
    takeoverStatus,
    status: proxyStatus,
  } = useProxyStatus();
  const isCurrentAppTakeoverActive = takeoverStatus?.[activeApp] || false;
  const takeoverCount = takeoverStatus
    ? Object.values(takeoverStatus).filter(Boolean).length
    : 0;
  const activeProviderId = useMemo(() => {
    const target = proxyStatus?.active_targets?.find(
      (t) => t.app_type === activeApp,
    );
    return target?.provider_id;
  }, [proxyStatus?.active_targets, activeApp]);

  const isServicesView = currentView === "providers";
  const { data, isLoading, refetch } = useProvidersQuery(activeApp, {
    enabled: isServicesView,
    isProxyRunning: isProxyRunning && isServicesView,
  });
  useAppEventSubscriptions({
    activeApp,
    watchActiveProvider: isServicesView,
    refetchProviders: refetch,
    queryClient,
    t,
  });
  useAppStartupChecks({
    activeApp,
    setEnvConflicts,
    setShowEnvBanner,
    t,
  });
  const providers = useMemo(() => data?.providers ?? {}, [data]);
  const currentProviderId = data?.currentProviderId ?? "";
  const isPrimaryView =
    currentView === "home" ||
    currentView === "services" ||
    currentView === "strategy" ||
    currentView === "stats";
  const isOpenClawView =
    activeApp === "openclaw" &&
    (isServicesView ||
      currentView === "workspace" ||
      currentView === "sessions" ||
      currentView === "openclawEnv" ||
      currentView === "openclawTools" ||
      currentView === "openclawAgents");
  const { data: openclawHealthWarnings = [] } =
    useOpenClawHealth(isOpenClawView);
  const hasSkillsSupport = true;
  const hasSessionSupport =
    activeApp === "claude" ||
    activeApp === "codex" ||
    activeApp === "opencode" ||
    activeApp === "openclaw" ||
    activeApp === "gemini";

  const {
    addProvider,
    updateProvider,
    switchProvider,
    deleteProvider,
    saveUsageScript,
    setAsDefaultModel,
  } = useProviderActions(activeApp, isProxyRunning);

  const { handleDisableOmo, handleDisableOmoSlim } = useProviderOmoActions({
    t,
  });

  useAppKeyboardShortcuts({
    currentView,
    setCurrentView,
  });

  const {
    handleOpenWebsite,
    handleConfirmAction,
    handleDuplicateProvider,
    handleOpenTerminal,
    handleImportSuccess,
  } = useProviderWorkspaceActions({
    activeApp,
    providers,
    addProvider,
    deleteProvider,
    refetchProviders: refetch,
    queryClient,
    t,
    confirmAction,
    clearConfirmAction,
  });

  const handleEditProvider = async ({
    provider,
    originalId,
  }: {
    provider: Provider;
    originalId?: string;
  }) => {
    await updateProvider(provider, originalId);
    handleEditDialogOpenChange(false);
  };

  const renderContent = () => {
    const content = (() => {
      switch (currentView) {
        case "home":
          return (
            <RouteCenterDashboard
              status={proxyStatus}
              isProxyRunning={isProxyRunning}
              takeoverCount={takeoverCount}
              onOpenUpstreams={() => setCurrentView("services")}
              onOpenRoutes={() => setCurrentView("strategy")}
            />
          );
        case "services":
          return (
            <div className="mx-auto w-full max-w-6xl px-8 py-8">
              <UniversalProviderPanel
                simpleMode
                onOpenAdvanced={() => setCurrentView("providers")}
              />
            </div>
          );
        case "strategy":
          return (
            <RouteCenterPanel
              onOpenAdvanced={() => setCurrentView("providers")}
            />
          );
        case "stats":
          return (
            <div className="px-6 py-6">
              <UsageDashboard />
            </div>
          );
        case "settings":
          return (
            <SettingsPage
              open={true}
              onOpenChange={() => setCurrentView("home")}
              onImportSuccess={handleImportSuccess}
              defaultTab={settingsDefaultTab}
            />
          );
        case "prompts":
          return (
            <PromptPanel
              ref={promptPanelRef}
              open={true}
              onOpenChange={() => setCurrentView("services")}
              appId={activeApp}
            />
          );
        case "skills":
          return (
            <UnifiedSkillsPanel
              ref={unifiedSkillsPanelRef}
              onOpenDiscovery={() => setCurrentView("skillsDiscovery")}
              currentApp={activeApp === "openclaw" ? "claude" : activeApp}
            />
          );
        case "skillsDiscovery":
          return (
            <SkillsPage
              ref={skillsPageRef}
              initialApp={activeApp === "openclaw" ? "claude" : activeApp}
            />
          );
        case "mcp":
          return (
            <UnifiedMcpPanel
              ref={mcpPanelRef}
              onOpenChange={() => setCurrentView("services")}
            />
          );
        case "agents":
          return (
            <AgentsPanel onOpenChange={() => setCurrentView("services")} />
          );
        case "universal":
          return (
            <div className="px-6 pt-4">
              <UniversalProviderPanel />
            </div>
          );

        case "sessions":
          return <SessionManagerPage key={activeApp} appId={activeApp} />;
        case "workspace":
          return <WorkspaceFilesPanel />;
        case "openclawEnv":
          return <EnvPanel />;
        case "openclawTools":
          return <ToolsPanel />;
        case "openclawAgents":
          return <AgentsDefaultsPanel />;
        case "providers":
          return (
            <div className="px-6 flex flex-col flex-1 min-h-0 overflow-hidden">
              <div className="flex-1 overflow-y-auto overflow-x-hidden pb-12 px-1">
                <div className="mx-auto mb-5 max-w-6xl border-b border-border py-5">
                  <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
                    Advanced
                  </p>
                  <h1 className="mt-2 text-xl font-semibold">客户端例外配置</h1>
                  <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
                    普通使用无需在这里逐个设置。仅在某个客户端需要独立渠道、模型映射或兼容参数时调整。
                  </p>
                </div>
                <AnimatePresence mode="wait">
                  <motion.div
                    key={activeApp}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.15 }}
                    className="space-y-4"
                  >
                    <ProviderWorkspacePanel
                      activeApp={activeApp}
                      providers={providers}
                      currentProviderId={currentProviderId}
                      isLoading={isLoading}
                      isProxyRunning={isProxyRunning}
                      isCurrentAppTakeoverActive={isCurrentAppTakeoverActive}
                      activeProviderId={activeProviderId}
                      onSwitch={switchProvider}
                      onEdit={openEditDialog}
                      onDelete={openDeleteConfirm}
                      onRemoveFromConfig={
                        activeApp === "opencode" || activeApp === "openclaw"
                          ? openRemoveConfirm
                          : undefined
                      }
                      onDisableOmo={
                        activeApp === "opencode" ? handleDisableOmo : undefined
                      }
                      onDisableOmoSlim={
                        activeApp === "opencode"
                          ? handleDisableOmoSlim
                          : undefined
                      }
                      onDuplicate={handleDuplicateProvider}
                      onConfigureUsage={openUsageModal}
                      onOpenWebsite={handleOpenWebsite}
                      onOpenTerminal={
                        activeApp === "claude" ? handleOpenTerminal : undefined
                      }
                      onCreate={openAddDialog}
                      onSetAsDefault={
                        activeApp === "openclaw" ? setAsDefaultModel : undefined
                      }
                      onOpenProxySettings={openProxySettings}
                    />
                  </motion.div>
                </AnimatePresence>
              </div>
            </div>
          );
        default:
          return null;
      }
    })();

    return <div className="flex-1 min-h-0">{content}</div>;
  };

  return (
    <div
      className="flex h-screen overflow-hidden bg-background text-foreground selection:bg-primary/30"
      style={{ overflowX: "hidden", paddingTop: CONTENT_TOP_OFFSET }}
    >
      <div
        className="fixed top-0 left-0 right-0 z-[60]"
        data-tauri-drag-region
        style={{ WebkitAppRegion: "drag", height: DRAG_BAR_HEIGHT } as any}
      />
      {showEnvBanner && envConflicts.length > 0 && (
        <EnvWarningBanner
          conflicts={envConflicts}
          onDismiss={handleEnvBannerDismiss}
          onDeleted={handleEnvBannerDeleted}
        />
      )}

      <header
        className="fixed z-50 w-full transition-all duration-300 bg-background/80 backdrop-blur-md"
        data-tauri-drag-region
        style={
          {
            WebkitAppRegion: "drag",
            top: DRAG_BAR_HEIGHT,
            height: HEADER_HEIGHT,
          } as any
        }
      >
        <div
          className="flex h-full items-center justify-between gap-2 px-6"
          data-tauri-drag-region
          style={{ WebkitAppRegion: "drag" } as any}
        >
          <div
            className="flex items-center gap-2"
            style={{ WebkitAppRegion: "no-drag" } as any}
          >
            {isPrimaryView ? (
              <div className="flex items-center gap-2">
                <PanelLeft className="h-4 w-4 text-muted-foreground" />
                <a
                  href={BIANMA_GITHUB_REPOSITORY_URL}
                  target="_blank"
                  rel="noreferrer"
                  className={cn(
                    "text-sm font-semibold transition-colors",
                    isProxyRunning && isCurrentAppTakeoverActive
                      ? "text-emerald-500 hover:text-emerald-600 dark:text-emerald-400 dark:hover:text-emerald-300"
                      : "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300",
                  )}
                >
                  {BIANMA_DISPLAY_NAME}
                </a>
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="icon"
                  onClick={() =>
                    setCurrentView(
                      currentView === "skillsDiscovery"
                        ? "skills"
                        : currentView === "settings"
                          ? "home"
                          : "services",
                    )
                  }
                  aria-label={t("common.back")}
                  className="mr-2 rounded-lg"
                >
                  <ArrowLeft className="w-4 h-4" />
                </Button>
                <h1 className="text-lg font-semibold">
                  {currentView === "settings" && t("settings.title")}
                  {currentView === "prompts" &&
                    t("prompts.title", { appName: t(`apps.${activeApp}`) })}
                  {currentView === "skills" && t("skills.title")}
                  {currentView === "skillsDiscovery" && t("skills.title")}
                  {currentView === "mcp" && t("mcp.unifiedPanel.title")}
                  {currentView === "agents" && t("agents.title")}
                  {currentView === "universal" &&
                    t("universalProvider.title", {
                      defaultValue: "统一供应商",
                    })}
                  {currentView === "sessions" && t("sessionManager.title")}
                  {currentView === "workspace" && t("workspace.title")}
                  {currentView === "openclawEnv" && t("openclaw.env.title")}
                  {currentView === "openclawTools" && t("openclaw.tools.title")}
                  {currentView === "openclawAgents" &&
                    t("openclaw.agents.title")}
                </h1>
              </div>
            )}
          </div>

          <div className="flex flex-1 min-w-0 items-center justify-end gap-1.5">
            {isPrimaryView && (
              <div
                className="flex shrink-0 items-center gap-1"
                style={{ WebkitAppRegion: "no-drag" } as any}
              >
                <UpdateBadge onClick={openAboutSettings} />
              </div>
            )}
            {isServicesView &&
              activeApp !== "opencode" &&
              activeApp !== "openclaw" && (
                <div
                  className="flex shrink-0 items-center gap-1.5"
                  style={{ WebkitAppRegion: "no-drag" } as any}
                >
                  {settingsData?.enableLocalProxy && (
                    <ProxyToggle activeApp={activeApp} />
                  )}
                  {settingsData?.enableFailoverToggle && (
                    <FailoverToggle activeApp={activeApp} />
                  )}
                </div>
              )}
            <div
              ref={toolbarRef}
              className="flex flex-1 min-w-0 overflow-x-hidden items-center"
            >
              <div
                className="flex shrink-0 items-center gap-1.5 ml-auto"
                style={{ WebkitAppRegion: "no-drag" } as any}
              >
                {currentView === "prompts" && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => promptPanelRef.current?.openAdd()}
                    className="hover:bg-black/5 dark:hover:bg-white/5"
                  >
                    <Plus className="w-4 h-4 mr-2" />
                    {t("prompts.add")}
                  </Button>
                )}
                {currentView === "mcp" && (
                  <>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => mcpPanelRef.current?.openImport()}
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <Download className="w-4 h-4 mr-2" />
                      {t("mcp.importExisting")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => mcpPanelRef.current?.openAdd()}
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <Plus className="w-4 h-4 mr-2" />
                      {t("mcp.addMcp")}
                    </Button>
                  </>
                )}
                {currentView === "skills" && (
                  <>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        unifiedSkillsPanelRef.current?.openRestoreFromBackup()
                      }
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <History className="w-4 h-4 mr-2" />
                      {t("skills.restoreFromBackup.button")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        unifiedSkillsPanelRef.current?.openInstallFromZip()
                      }
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <FolderArchive className="w-4 h-4 mr-2" />
                      {t("skills.installFromZip.button")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        unifiedSkillsPanelRef.current?.openImport()
                      }
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <Download className="w-4 h-4 mr-2" />
                      {t("skills.import")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setCurrentView("skillsDiscovery")}
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <Search className="w-4 h-4 mr-2" />
                      {t("skills.discover")}
                    </Button>
                  </>
                )}
                {currentView === "skillsDiscovery" && (
                  <>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => skillsPageRef.current?.refresh()}
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <RefreshCw className="w-4 h-4 mr-2" />
                      {t("skills.refresh")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => skillsPageRef.current?.openRepoManager()}
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <Settings className="w-4 h-4 mr-2" />
                      {t("skills.repoManager")}
                    </Button>
                  </>
                )}
                {isServicesView && (
                  <>
                    <AppSwitcher
                      activeApp={activeApp}
                      onSwitch={setActiveApp}
                      visibleApps={visibleApps}
                      compact={isToolbarCompact}
                    />

                    <div className="flex items-center gap-1 p-1 bg-muted rounded-xl">
                      <AnimatePresence mode="wait">
                        <motion.div
                          key={
                            activeApp === "openclaw" ? "openclaw" : "default"
                          }
                          className="flex items-center gap-1"
                          initial={{ opacity: 0 }}
                          animate={{ opacity: 1 }}
                          exit={{ opacity: 0 }}
                          transition={{ duration: 0.15 }}
                        >
                          {activeApp === "openclaw" ? (
                            <>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("workspace")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("workspace.manage")}
                              >
                                <FolderOpen className="w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("openclawEnv")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("openclaw.env.title")}
                              >
                                <KeyRound className="w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("openclawTools")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("openclaw.tools.title")}
                              >
                                <Shield className="w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("openclawAgents")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("openclaw.agents.title")}
                              >
                                <Cpu className="w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("sessions")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("sessionManager.title")}
                              >
                                <History className="w-4 h-4" />
                              </Button>
                            </>
                          ) : (
                            <>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("skills")}
                                className={cn(
                                  "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5",
                                  "transition-all duration-200 ease-in-out overflow-hidden",
                                  hasSkillsSupport
                                    ? "opacity-100 w-8 scale-100 px-2"
                                    : "opacity-0 w-0 scale-75 pointer-events-none px-0 -ml-1",
                                )}
                                title={t("skills.manage")}
                              >
                                <Wrench className="flex-shrink-0 w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("prompts")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("prompts.manage")}
                              >
                                <Book className="w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("sessions")}
                                className={cn(
                                  "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5",
                                  "transition-all duration-200 ease-in-out overflow-hidden",
                                  hasSessionSupport
                                    ? "opacity-100 w-8 scale-100 px-2"
                                    : "opacity-0 w-0 scale-75 pointer-events-none px-0 -ml-1",
                                )}
                                title={t("sessionManager.title")}
                              >
                                <History className="flex-shrink-0 w-4 h-4" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setCurrentView("mcp")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5"
                                title={t("mcp.title")}
                              >
                                <McpIcon size={16} />
                              </Button>
                            </>
                          )}
                        </motion.div>
                      </AnimatePresence>
                    </div>

                    <Button
                      onClick={openAddDialog}
                      size="icon"
                      className={`ml-2 ${addActionButtonClass}`}
                    >
                      <Plus className="w-5 h-5" />
                    </Button>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      </header>

      {isPrimaryView && (
        <aside
          className="fixed bottom-0 left-0 z-40 border-r border-border bg-muted/20"
          style={{ top: CONTENT_TOP_OFFSET, width: SIDEBAR_WIDTH }}
        >
          <nav
            aria-label={t("navigation.label")}
            className="flex h-full flex-col p-3"
          >
            <div className="space-y-1">
              {[
                {
                  view: "home",
                  label: "概览",
                  detail: "状态与一键启动",
                  icon: Home,
                },
                {
                  view: "services",
                  label: "上游",
                  detail: "渠道与模型",
                  icon: Boxes,
                },
                {
                  view: "strategy",
                  label: "路由",
                  detail: "统一入口与接入状态",
                  icon: Route,
                },
                {
                  view: "stats",
                  label: "用量",
                  detail: "请求、成本与记录",
                  icon: Gauge,
                },
              ].map(({ view, label, detail, icon: Icon }) => (
                <button
                  key={view}
                  type="button"
                  onClick={() => setCurrentView(view as View)}
                  aria-current={currentView === view ? "page" : undefined}
                  data-testid={`primary-nav-${view}`}
                  className={cn(
                    "flex w-full items-start gap-3 rounded-md px-3 py-2 text-left transition-colors",
                    currentView === view
                      ? "bg-accent text-foreground"
                      : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                  )}
                >
                  <Icon
                    className="mt-0.5 h-4 w-4 shrink-0"
                    aria-hidden="true"
                  />
                  <span>
                    <span className="block text-sm font-medium">{label}</span>
                    <span className="block text-[11px] leading-4 opacity-75">
                      {detail}
                    </span>
                  </span>
                </button>
              ))}
            </div>
            <button
              type="button"
              onClick={openGeneralSettings}
              className="mt-auto flex items-center gap-3 rounded-md px-3 py-2 text-sm text-muted-foreground hover:bg-accent/60 hover:text-foreground"
            >
              <Settings className="h-4 w-4" />
              设置
            </button>
          </nav>
        </aside>
      )}

      <main
        className="flex-1 min-h-0 flex flex-col overflow-y-auto"
        style={{ marginLeft: isPrimaryView ? SIDEBAR_WIDTH : 0 }}
        tabIndex={-1}
      >
        {isOpenClawView && openclawHealthWarnings.length > 0 && (
          <OpenClawHealthBanner warnings={openclawHealthWarnings} />
        )}
        {renderContent()}
      </main>

      <AddProviderDialog
        open={isAddOpen}
        onOpenChange={handleAddDialogOpenChange}
        appId={activeApp}
        onSubmit={addProvider}
      />

      <EditProviderDialog
        open={Boolean(editingProvider)}
        provider={effectiveEditingProvider}
        onOpenChange={handleEditDialogOpenChange}
        onSubmit={handleEditProvider}
        appId={activeApp}
        isProxyTakeover={isProxyRunning && isCurrentAppTakeoverActive}
      />

      {effectiveUsageProvider && (
        <UsageScriptModal
          key={effectiveUsageProvider.id}
          provider={effectiveUsageProvider}
          appId={activeApp}
          isOpen={Boolean(usageProvider)}
          onClose={closeUsageModal}
          onSave={(script) => {
            if (usageProvider) {
              void saveUsageScript(usageProvider, script);
            }
          }}
        />
      )}

      <ConfirmDialog
        isOpen={Boolean(confirmAction)}
        title={
          confirmAction?.action === "remove"
            ? t("confirm.removeProvider")
            : t("confirm.deleteProvider")
        }
        message={
          confirmAction
            ? confirmAction.action === "remove"
              ? t("confirm.removeProviderMessage", {
                  name: confirmAction.provider.name,
                })
              : t("confirm.deleteProviderMessage", {
                  name: confirmAction.provider.name,
                })
            : ""
        }
        onConfirm={() => void handleConfirmAction()}
        onCancel={clearConfirmAction}
      />

      <DeepLinkImportDialog />
    </div>
  );
}

export default App;
