import type { ReactNode } from "react";

export default function SettingsPageHead({
  eyebrow,
  title,
  description,
}: {
  eyebrow?: string;
  title: string;
  description?: string;
  children?: ReactNode;
}) {
  return (
    <header className="settings-v5-page-head">
      {eyebrow ? (
        <span className="settings-v5-page-head__eyebrow">{eyebrow}</span>
      ) : null}
      <h2 className="settings-v5-page-head__title">{title}</h2>
      {description ? (
        <p className="settings-v5-page-head__description">{description}</p>
      ) : null}
    </header>
  );
}
