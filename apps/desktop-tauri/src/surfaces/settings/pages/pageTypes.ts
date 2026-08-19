import type {
  ProviderCatalogEntry,
  SettingsSnapshot,
  SettingsUpdate,
} from "../../../types/bridge";

export interface SettingsPageProps {
  settings: SettingsSnapshot;
  set: (patch: SettingsUpdate) => void;
  saving: boolean;
  catalog?: ProviderCatalogEntry[];
}
