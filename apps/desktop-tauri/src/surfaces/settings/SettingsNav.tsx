import { useCallback, type KeyboardEvent, type ReactElement, type ReactNode } from "react";
import { useLocale } from "../../hooks/useLocale";
import type { LocaleKey } from "../../i18n/keys";
import type { SettingsNavId } from "./settingsTabs";

const ICON_SIZE = 16;

function Svg({ children }: { children: ReactNode }) {
  return (
    <svg
      width={ICON_SIZE}
      height={ICON_SIZE}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  );
}

const NAV_ICONS: Record<SettingsNavId, ReactElement> = {
  general: (
    <Svg>
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
    </Svg>
  ),
  providers: (
    <Svg>
      <rect x="2" y="2" width="5" height="5" rx="1" />
      <rect x="9" y="2" width="5" height="5" rx="1" />
      <rect x="2" y="9" width="5" height="5" rx="1" />
      <rect x="9" y="9" width="5" height="5" rx="1" />
    </Svg>
  ),
  trayPanel: (
    <Svg>
      <rect x="1.8" y="2.5" width="12.4" height="11" rx="1.8" />
      <path d="M1.8 6h12.4M6 6v7.5" />
    </Svg>
  ),
  floatBar: (
    <Svg>
      <rect x="1.5" y="5.5" width="13" height="5" rx="2.5" />
    </Svg>
  ),
  taskbarStatus: (
    <Svg>
      <rect x="1.5" y="3" width="13" height="9.5" rx="1.6" />
      <path d="M3.5 8.5h3.2M9.2 6.5h2.8M9.2 9.5h2.8" />
    </Svg>
  ),
  notifications: (
    <Svg>
      <path d="M3.5 11.5h9l-1.2-1.8V7a3.3 3.3 0 0 0-6.6 0v2.7Z" />
      <path d="M6.5 13a1.7 1.7 0 0 0 3 0" />
    </Svg>
  ),
  appearance: (
    <Svg>
      <path d="M1.5 8c1.6-3 4-4.5 6.5-4.5S13 5 14.5 8c-1.5 3-4 4.5-6.5 4.5S3.1 11 1.5 8Z" />
      <circle cx="8" cy="8" r="2" />
    </Svg>
  ),
  privacy: (
    <Svg>
      <rect x="4" y="7" width="8" height="6.5" rx="1.4" />
      <path d="M5.5 7V5.4a2.5 2.5 0 0 1 5 0V7" />
    </Svg>
  ),
  advanced: (
    <Svg>
      <path d="M2 4h8M2 8h5M2 12h10" />
      <circle cx="11.5" cy="4" r="1.4" />
      <circle cx="8.5" cy="8" r="1.4" />
      <circle cx="13" cy="12" r="1.4" />
    </Svg>
  ),
  about: (
    <Svg>
      <circle cx="8" cy="8" r="6.25" />
      <path d="M8 7v4" />
      <circle cx="8" cy="5" r="0.6" fill="currentColor" stroke="none" />
    </Svg>
  ),
};

type NavEntry =
  | { type: "group"; labelKey: LocaleKey }
  | { type: "item"; id: SettingsNavId; labelKey: LocaleKey; footer?: boolean };

export const SETTINGS_NAV_ITEMS: NavEntry[] = [
  { type: "group", labelKey: "SettingsNavGroupApp" },
  { type: "item", id: "general", labelKey: "TabGeneral" },
  { type: "item", id: "providers", labelKey: "TabProviders" },
  { type: "group", labelKey: "SettingsNavGroupSurfaces" },
  { type: "item", id: "trayPanel", labelKey: "TabTrayPanel" },
  { type: "item", id: "floatBar", labelKey: "TabFloatBar" },
  { type: "item", id: "taskbarStatus", labelKey: "TabTaskbarStatus" },
  { type: "item", id: "appearance", labelKey: "TabAppearance" },
  { type: "group", labelKey: "SettingsNavGroupFeedback" },
  { type: "item", id: "notifications", labelKey: "SectionNotifications" },
  { type: "group", labelKey: "SettingsNavGroupData" },
  { type: "item", id: "privacy", labelKey: "TabPrivacy" },
  { type: "item", id: "advanced", labelKey: "TabAdvanced" },
  { type: "item", id: "about", labelKey: "TabAbout", footer: true },
];

export const SETTINGS_NAV_ORDER: SettingsNavId[] = SETTINGS_NAV_ITEMS.flatMap(
  (entry) => (entry.type === "item" ? [entry.id] : []),
);

export default function SettingsNav({
  active,
  onSelect,
}: {
  active: SettingsNavId;
  onSelect: (id: SettingsNavId, source: "pointer" | "keyboard") => void;
}) {
  const { t } = useLocale();

  const move = useCallback(
    (delta: number | "home" | "end") => {
      const current = SETTINGS_NAV_ORDER.indexOf(active);
      let next = current < 0 ? 0 : current;
      if (delta === "home") next = 0;
      else if (delta === "end") next = SETTINGS_NAV_ORDER.length - 1;
      else next = Math.min(SETTINGS_NAV_ORDER.length - 1, Math.max(0, current + delta));
      const id = SETTINGS_NAV_ORDER[next];
      if (id) onSelect(id, "keyboard");
    },
    [active, onSelect],
  );

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(-1);
        break;
      case "Home":
        event.preventDefault();
        move("home");
        break;
      case "End":
        event.preventDefault();
        move("end");
        break;
      default:
        break;
    }
  };

  return (
    <nav
      className="settings-v5-nav"
      aria-label={t("SettingsWindowTitle")}
      onKeyDown={onKeyDown}
    >
      {SETTINGS_NAV_ITEMS.map((entry) =>
        entry.type === "group" ? (
          <div key={entry.labelKey} className="settings-v5-nav__group">
            {t(entry.labelKey)}
          </div>
        ) : (
          <button
            key={entry.id}
            type="button"
            className={`settings-v5-nav__item${entry.footer ? " settings-v5-nav__item--footer" : ""}${
              active === entry.id ? " is-active" : ""
            }`}
            aria-current={active === entry.id ? "page" : undefined}
            onClick={() => onSelect(entry.id, "pointer")}
          >
            <span className="settings-v5-nav__icon">{NAV_ICONS[entry.id]}</span>
            <span className="settings-v5-nav__label">{t(entry.labelKey)}</span>
          </button>
        ),
      )}
    </nav>
  );
}
