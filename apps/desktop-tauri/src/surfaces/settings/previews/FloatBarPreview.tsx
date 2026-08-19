import { useMemo } from "react";
import FloatBar from "../../../floatbar/FloatBar";
import { useProviders } from "../../../hooks/useProviders";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type { SettingsSnapshot } from "../../../types/bridge";
import { floatBarEntriesFromIds } from "../floatBarEntries";
import { previewProviderList } from "./catalogFixtures";

interface Props {
  settings: SettingsSnapshot;
}

export default function FloatBarPreview({ settings }: Props) {
  const { providers } = useProviders({ refreshOnMount: false });
  const catalog = useMemo(() => previewProviderList(providers), [providers]);
  const entries = useMemo(
    () => settings.floatBarEntries ?? floatBarEntriesFromIds(settings.floatBarProviderIds),
    [settings.floatBarEntries, settings.floatBarProviderIds],
  );
  const previewProviders = useMemo(() => {
    const specific = entries
      .map((entry) => entry.providerId)
      .filter((id) => id && id !== TASKBAR_PROVIDER_AUTO);
    if (specific.length > 0) {
      return catalog.filter((row) => specific.includes(row.providerId));
    }
    return catalog.slice(0, Math.max(1, entries.length));
  }, [catalog, entries]);
  const previewSettings = useMemo(
    () => ({
      ...settings,
      floatBarProviderIds: previewProviders.map((row) => row.providerId),
        floatBarEntries: entries,
    }),
    [previewProviders, settings],
  );
  const state = useMemo(
    () => ({
      contractVersion: "preview",
      providers: [],
      settings: previewSettings,
    }),
    [previewSettings],
  );

  return (
    <FloatBar
      state={state}
      preview={{ settings: previewSettings, providers: previewProviders }}
    />
  );
}
