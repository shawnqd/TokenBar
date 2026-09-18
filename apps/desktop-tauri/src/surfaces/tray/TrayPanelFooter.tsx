import { useLocale } from "../../hooks/useLocale";

/** Stroke-width / cap / join shared by every footer action glyph. */
const footerIconProps = {
  width: 12,
  height: 12,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.85,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

const RefreshIcon = () => (
  <svg {...footerIconProps}>
    <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
    <path d="M21 3v5h-5" />
    <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
    <path d="M3 21v-5h5" />
  </svg>
);

const GearIcon = () => (
  <svg {...footerIconProps}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 2-2 2 2 0 0 1 2 2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a2 2 0 0 1 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

const PowerIcon = () => (
  <svg {...footerIconProps}>
    <path d="M18.36 6.64a9 9 0 1 1-12.73 0" />
    <line x1="12" y1="2" x2="12" y2="12" />
  </svg>
);

export interface TrayPanelFooterProps {
  isRefreshing?: boolean;
  /** Settings previews keep the exact footer geometry but never execute actions. */
  interactive?: boolean;
  onRefresh?: () => void;
  onOpenSettings?: () => void;
  onQuit?: () => void;
}

export default function TrayPanelFooter({
  isRefreshing = false,
  interactive = true,
  onRefresh,
  onOpenSettings,
  onQuit,
}: TrayPanelFooterProps) {
  const { t } = useLocale();
  const actionProps = (onClick: (() => void) | undefined) => ({
    onClick: interactive ? onClick : undefined,
    "aria-disabled": interactive ? undefined : true,
    tabIndex: interactive ? undefined : -1,
  });

  return (
    <footer className="flyout-footer" aria-label={t("PanelMenu")}>
      <button type="button" className="footer-row" {...actionProps(onRefresh)}>
        <span className="footer-row__left">
          <span className={`footer-row__icon${isRefreshing ? " is-spinning" : ""}`}>
            <RefreshIcon />
          </span>
          <span className="footer-row__label">{t("ActionRefresh")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+R</span>
      </button>
      <button type="button" className="footer-row" {...actionProps(onOpenSettings)}>
        <span className="footer-row__left">
          <span className="footer-row__icon"><GearIcon /></span>
          <span className="footer-row__label">{t("MenuSettings")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+,</span>
      </button>
      <button type="button" className="footer-row" {...actionProps(onQuit)}>
        <span className="footer-row__left">
          <span className="footer-row__icon"><PowerIcon /></span>
          <span className="footer-row__label">{t("MenuQuit")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+Q</span>
      </button>
    </footer>
  );
}
