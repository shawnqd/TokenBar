import type { ReactNode } from "react";
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
  /** 预览三页：Settings 下发的页头节点，渲染在左列顶部。 */
  head?: ReactNode;
}
