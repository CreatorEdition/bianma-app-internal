import { BarChart2, Boxes, Route, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

interface HomeDashboardProps {
  activeAppLabel: string;
  providerCount: number;
  currentProviderName?: string;
  isProxyReady: boolean;
  onOpenServices: () => void;
  onOpenStrategy: () => void;
  onOpenStats: () => void;
}

interface HomeActionCardProps {
  title: string;
  description: string;
  detail: string;
  icon: LucideIcon;
  onClick: () => void;
}

/**
 * 首页只展示 App 已有的本地状态与导航入口，不发起额外聚合请求或后台刷新。
 */
function HomeActionCard({
  title,
  description,
  detail,
  icon: Icon,
  onClick,
}: HomeActionCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group rounded-2xl border border-border/70 bg-card/70 p-5 text-left shadow-sm transition-colors hover:border-primary/50 hover:bg-muted/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
    >
      <div className="flex items-start justify-between gap-4">
        <span className="rounded-xl bg-primary/10 p-2.5 text-primary">
          <Icon className="h-5 w-5" aria-hidden="true" />
        </span>
        <span className="text-sm text-muted-foreground group-hover:text-foreground">
          {detail}
        </span>
      </div>
      <h2 className="mt-5 text-lg font-semibold">{title}</h2>
      <p className="mt-1 text-sm leading-6 text-muted-foreground">
        {description}
      </p>
    </button>
  );
}

export function HomeDashboard({
  activeAppLabel,
  providerCount,
  currentProviderName,
  isProxyReady,
  onOpenServices,
  onOpenStrategy,
  onOpenStats,
}: HomeDashboardProps) {
  const { t } = useTranslation();

  const providerDetail = currentProviderName
    ? t("home.currentProvider", { name: currentProviderName })
    : t("home.providerCount", { count: providerCount });

  return (
    <section className="mx-auto w-full max-w-5xl px-6 py-8 sm:py-12">
      <div className="max-w-2xl">
        <p className="text-sm font-medium text-primary">{activeAppLabel}</p>
        <h1 className="mt-2 text-3xl font-bold tracking-tight">
          {t("home.title")}
        </h1>
        <p className="mt-3 text-base leading-7 text-muted-foreground">
          {t("home.description")}
        </p>
      </div>

      <div className="mt-8 grid gap-4 sm:grid-cols-2">
        <HomeActionCard
          title={t("navigation.services")}
          description={t("home.servicesDescription")}
          detail={providerDetail}
          icon={Boxes}
          onClick={onOpenServices}
        />
        <HomeActionCard
          title={t("navigation.strategy")}
          description={t("home.strategyDescription")}
          detail={isProxyReady ? t("home.proxyReady") : t("home.proxyNotReady")}
          icon={Route}
          onClick={onOpenStrategy}
        />
        <HomeActionCard
          title={t("navigation.stats")}
          description={t("home.statsDescription")}
          detail={t("home.statsDetail")}
          icon={BarChart2}
          onClick={onOpenStats}
        />
      </div>
    </section>
  );
}
