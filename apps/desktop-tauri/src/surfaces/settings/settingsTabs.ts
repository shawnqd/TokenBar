import type { SettingsTabId } from "../../types/bridge";
import type { LocaleKey } from "../../i18n/keys";

export const SETTINGS_NAV_IDS = [
  "general",
  "providers",
  "trayPanel",
  "floatBar",
  "taskbarStatus",
  "notifications",
  "appearance",
  "privacy",
  "advanced",
  "about",
] as const;

export type SettingsNavId = (typeof SETTINGS_NAV_IDS)[number];

const LEGACY_TAB_MAP: Record<string, SettingsNavId> = {
  dashboard: "trayPanel",
  menuBar: "taskbarStatus",
  menu: "appearance",
};

export function isSettingsTab(value: string): value is SettingsTabId {
  return (
    (SETTINGS_NAV_IDS as readonly string[]).includes(value) ||
    value in LEGACY_TAB_MAP
  );
}

export function canonicalizeSettingsTab(tab: SettingsTabId): SettingsNavId {
  return LEGACY_TAB_MAP[tab] ?? (tab as SettingsNavId);
}

export const SETTINGS_PAGE_COPY: Record<
  SettingsNavId,
  { titleKey: LocaleKey; descriptionKey?: LocaleKey; eyebrowKey: LocaleKey }
> = {
  general: {
    titleKey: "TabGeneral",
    descriptionKey: "SettingsPageGeneralDescription",
    eyebrowKey: "SettingsNavGroupApp",
  },
  providers: {
    titleKey: "TabProviders",
    descriptionKey: "SettingsPageProvidersDescription",
    eyebrowKey: "SettingsNavGroupApp",
  },
  trayPanel: {
    titleKey: "TabTrayPanel",
    descriptionKey: "SettingsPageTrayPanelDescription",
    eyebrowKey: "SettingsNavGroupSurfaces",
  },
  floatBar: {
    titleKey: "TabFloatBar",
    descriptionKey: "SettingsPageFloatBarDescription",
    eyebrowKey: "SettingsNavGroupSurfaces",
  },
  taskbarStatus: {
    titleKey: "TabTaskbarStatus",
    descriptionKey: "SettingsPageTaskbarStatusDescription",
    eyebrowKey: "TaskbarSettingsEyebrow",
  },
  notifications: {
    titleKey: "SectionNotifications",
    descriptionKey: "SettingsPageNotificationsDescription",
    eyebrowKey: "SettingsNavGroupFeedback",
  },
  appearance: {
    titleKey: "TabAppearance",
    descriptionKey: "SettingsPageAppearanceDescription",
    eyebrowKey: "SettingsNavGroupFeedback",
  },
  privacy: {
    titleKey: "TabPrivacy",
    descriptionKey: "SettingsPagePrivacyDescription",
    eyebrowKey: "SettingsNavGroupData",
  },
  advanced: {
    titleKey: "TabAdvanced",
    descriptionKey: "SettingsPageAdvancedDescription",
    eyebrowKey: "SettingsNavGroupData",
  },
  about: {
    titleKey: "TabAbout",
    eyebrowKey: "SettingsNavGroupData",
  },
};
