import type { ReactNode } from "react";

/**
 * Shared shells for the Settings → Providers detail workspace.
 *
 * Every provider family (subscription quota, balance/API, cookie/token plan,
 * OAuth/CLI, generic API) uses these wrappers so cards, titles, status badges
 * and nested auth methods share one Windows utility-card language.
 */

export function ProviderSection({
  title,
  headerAction,
  children,
  className = "",
  as = "section",
}: {
  title?: ReactNode;
  headerAction?: ReactNode;
  children: ReactNode;
  className?: string;
  as?: "section" | "div";
}) {
  const Tag = as;
  return (
    <Tag className={`provider-detail-section ${className}`.trim()}>
      {(title || headerAction) && (
        <div className="provider-detail-section__header">
          {title ? <h4>{title}</h4> : <span />}
          {headerAction}
        </div>
      )}
      {children}
    </Tag>
  );
}

export function ProviderAuthMethod({
  title,
  badge,
  badgeTone = "neutral",
  meta,
  actions,
  children,
  className = "",
}: {
  title: ReactNode;
  badge?: ReactNode;
  badgeTone?: "ok" | "warn" | "error" | "info" | "neutral" | "unset";
  meta?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={`provider-auth-method ${className}`.trim()}>
      <div className="provider-auth-method__header">
        <div className="provider-auth-method__info">
          <div className="provider-auth-method__title-row">
            <strong className="provider-auth-method__title">{title}</strong>
            {badge != null && badge !== false && (
              <span
                className={`provider-status-badge provider-status-badge--${badgeTone}`}
              >
                {badge}
              </span>
            )}
          </div>
          {meta != null && (
            <div className="provider-auth-method__meta">{meta}</div>
          )}
        </div>
        {actions != null && (
          <div className="provider-auth-method__actions">{actions}</div>
        )}
      </div>
      {children != null && (
        <div className="provider-auth-method__body">{children}</div>
      )}
    </div>
  );
}

export function ProviderStatusLine({
  tone = "neutral",
  children,
  className = "",
}: {
  tone?: "ok" | "warn" | "error" | "info" | "neutral" | "loading";
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={`provider-status-line provider-status-line--${tone} ${className}`.trim()}
      role="status"
    >
      {children}
    </div>
  );
}

export function ProviderHeaderSkeleton() {
  return (
    <header
      className="provider-detail-header-block provider-detail-header-block--skeleton"
      aria-busy="true"
    >
      <div className="provider-detail-header">
        <span className="provider-detail-skeleton provider-detail-skeleton--icon" />
        <div className="provider-detail-title-group">
          <span className="provider-detail-skeleton provider-detail-skeleton--title" />
          <span className="provider-detail-skeleton provider-detail-skeleton--subtitle" />
        </div>
      </div>
      <div className="provider-detail-skeleton-grid" aria-hidden>
        <span className="provider-detail-skeleton provider-detail-skeleton--label" />
        <span className="provider-detail-skeleton provider-detail-skeleton--value" />
        <span className="provider-detail-skeleton provider-detail-skeleton--label" />
        <span className="provider-detail-skeleton provider-detail-skeleton--value" />
      </div>
    </header>
  );
}

export function ProviderSectionSkeleton({ title }: { title?: string }) {
  return (
    <section
      className="provider-detail-section provider-detail-section--skeleton"
      aria-busy="true"
    >
      {title ? <h4>{title}</h4> : null}
      <div className="provider-detail-skeleton-stack" aria-hidden>
        <span className="provider-detail-skeleton provider-detail-skeleton--bar" />
        <span className="provider-detail-skeleton provider-detail-skeleton--bar provider-detail-skeleton--bar-short" />
        <span className="provider-detail-skeleton provider-detail-skeleton--line" />
      </div>
    </section>
  );
}

export function ProviderFieldRow({
  label,
  children,
  className = "",
}: {
  label: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`provider-detail-field provider-detail-field--row ${className}`.trim()}>
      <span className="provider-detail-field__label">{label}</span>
      <div className="provider-detail-field__control">{children}</div>
    </div>
  );
}
