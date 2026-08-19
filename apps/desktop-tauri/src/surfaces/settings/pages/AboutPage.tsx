import { useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { getAppInfo, openExternalUrl } from "../../../lib/tauri";
import type { AppInfoBridge } from "../../../types/bridge";
import type { LocaleKey } from "../../../i18n/keys";
import type { SettingsPageProps } from "./pageTypes";
import codexbarIcon from "../../../assets/codexbar-icon.png";

const LINKS: { labelKey: LocaleKey; url: string; text: string }[] = [
  {
    labelKey: "AboutLinkGithub",
    url: "https://github.com/Finesssee/Win-CodexBar",
    text: "项目仓库",
  },
  {
    labelKey: "AboutLinkWebsite",
    url: "https://codexbar.app",
    text: "官网",
  },
  {
    labelKey: "AboutLinkOriginalProject",
    url: "https://github.com/steipete/CodexBar",
    text: "上游项目",
  },
];

export default function AboutPage(_props: SettingsPageProps) {
  const { t } = useLocale();
  const [appInfo, setAppInfo] = useState<AppInfoBridge | null>(null);
  const [linkError, setLinkError] = useState<string | null>(null);

  useEffect(() => {
    void getAppInfo().then(setAppInfo);
  }, []);

  const open = (url: string) => {
    setLinkError(null);
    openExternalUrl(url).catch((error) => setLinkError(String(error)));
  };

  return (
    <section className="s5-section" style={{ padding: 16 }}>
      <div className="s5-about">
        {appInfo ? (
          <img
            className="s5-about-mark"
            src={codexbarIcon}
            alt=""
            width={44}
            height={44}
          />
        ) : (
          <div className="s5-about-mark">TB</div>
        )}
        <div>
          <div style={{ fontSize: 16, fontWeight: 650 }}>
            {appInfo?.name ?? "TokenBar"}
          </div>
          <div
            style={{ fontSize: 12, color: "var(--flyout-sub)", marginTop: 2 }}
          >
            {t("Version")} {appInfo?.version ?? "—"}
            {appInfo?.buildNumber ? ` · ${appInfo.buildNumber}` : ""}
          </div>
          <div
            style={{ fontSize: 12, color: "var(--flyout-muted)", marginTop: 4 }}
          >
            {appInfo?.tagline || "Windows 上的额度状态栏"}
          </div>
        </div>
      </div>
      <div className="s5-actions" style={{ marginTop: 14 }}>
        {LINKS.map((link) => (
          <button
            key={link.url}
            type="button"
            className="s5-ghost"
            onClick={() => open(link.url)}
          >
            {t(link.labelKey)}
          </button>
        ))}
      </div>
      {linkError ? (
        <p className="s5-hint">
          {t("StateError")}: {linkError}
        </p>
      ) : null}
        <p className="s5-hint" style={{ marginTop: 14 }}>
          {t("AboutCopyrightPrefix")}{" "}
          <button
            type="button"
            className="s5-ghost"
            onClick={() => open("https://github.com/steipete/CodexBar")}
          >
            CodexBar
          </button>{" "}
          {t("AboutCopyrightSuffix")}
        </p>
    </section>
  );
}
