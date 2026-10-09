import { useLocale } from "../hooks/useLocale";
import type { Language } from "../types/bridge";
import "./UpcomingPlanList.css";

export interface UpcomingPlanItem {
  id: string;
  label: string;
  activationAt: string;
}

function localeCodeFor(language: Language): string {
  switch (language) {
    case "chinese": return "zh-CN";
    case "chinesetraditional": return "zh-TW";
    case "japanese": return "ja-JP";
    case "korean": return "ko-KR";
    case "spanish": return "es-MX";
    default: return "en-US";
  }
}

function formatActivationTime(activationAt: string, locale: string): string {
  const date = new Date(activationAt);
  if (!Number.isFinite(date.getTime())) return activationAt;

  try {
    return new Intl.DateTimeFormat(locale, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(date);
  } catch {
    return date.toLocaleString();
  }
}

export default function UpcomingPlanList({
  items,
  className = "",
}: {
  items: UpcomingPlanItem[];
  className?: string;
}) {
  const { t, language } = useLocale();
  if (items.length === 0) return null;
  const locale = localeCodeFor(language);

  return (
    <div className={`upcoming-plan-list ${className}`.trim()} role="list">
      {items.map((item) => (
        <div className="upcoming-plan-list__row" key={item.id} role="listitem">
          <div className="upcoming-plan-list__identity">
            <span className="upcoming-plan-list__title">{item.label}</span>
            <span className="upcoming-plan-list__state">
              {t("ProviderPlanPending")}
            </span>
          </div>
          <time
            className="upcoming-plan-list__time"
            dateTime={item.activationAt}
          >
            {t("ProviderPlanStartsAt")} {formatActivationTime(item.activationAt, locale)}
          </time>
        </div>
      ))}
    </div>
  );
}
