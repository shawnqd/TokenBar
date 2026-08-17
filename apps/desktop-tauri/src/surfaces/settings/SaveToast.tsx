import { useLocale } from "../../hooks/useLocale";

export type SaveToastState = "hidden" | "saving" | "saved" | "error";

export default function SaveToast({
  state,
  error,
}: {
  state: SaveToastState;
  error?: string | null;
}) {
  const { t } = useLocale();
  if (state === "hidden") return null;

  const message =
    state === "saving"
      ? t("SettingsStatusSaving")
      : state === "saved"
        ? t("SettingsStatusSaved")
        : (error ?? t("StateError"));

  return (
    <div
      className={`settings-v5-toast${state === "error" ? " settings-v5-toast--error" : ""}`}
      role={state === "error" ? "alert" : "status"}
      aria-live="polite"
    >
      {message}
    </div>
  );
}
