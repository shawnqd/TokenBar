import { useCallback, useEffect, useMemo, useState } from "react";
import {
  buildFontPickerOptions,
  familyIsContinuousWeight,
  firstInstalledWhitelistFamily,
  fontInstallGuide,
  isFamilyReady,
  matchInstalledFamily,
  whitelistEntry,
  type FontInstallGuide,
} from "../lib/fontWhitelist";
import { getTaskbarFontFamilies, openExternalUrl } from "../lib/tauri";
import type { TaskbarFontFamily } from "../types/bridge";

/**
 * Shared font-picker state for every settings surface.
 *
 * Options come only from DESIGN_SYSTEM.md §2 (plus a previously saved
 * non-whitelist family). Installed-face enumeration is used to label
 * missing families and pick a restore-default — never to grow the list.
 */
export function useFontPicker(currentFamily: string) {
  const [families, setFamilies] = useState<TaskbarFontFamily[]>([]);
  const [install, setInstall] = useState<FontInstallGuide | null>(null);
  const [installMiss, setInstallMiss] = useState(false);

  const refresh = useCallback(() => {
    return getTaskbarFontFamilies()
      .then((list) => {
        setFamilies(list);
        return list;
      })
      .catch(() => {
        // Enumeration failure still leaves the five whitelist rows, so the
        // dropdown never collapses to whatever Windows last enumerated.
        return [] as TaskbarFontFamily[];
      });
  }, []);

  useEffect(() => {
    let cancelled = false;
    refresh().then((list) => {
      if (cancelled) return;
      setFamilies(list);
    });
    return () => {
      cancelled = true;
    };
  }, [refresh]);

  const options = useMemo(
    () => buildFontPickerOptions(families, currentFamily),
    [currentFamily, families],
  );

  const chooseFamily = useCallback(
    (name: string, apply: (family: string) => void) => {
      if (isFamilyReady(name, families)) {
        apply(name);
        return;
      }
      const guide = fontInstallGuide(name);
      if (!guide) {
        apply(name);
        return;
      }
      setInstallMiss(false);
      setInstall(guide);
    },
    [families],
  );

  const cancelInstall = useCallback(() => {
    setInstall(null);
    setInstallMiss(false);
  }, []);

  const openInstallPage = useCallback(() => {
    if (!install) return;
    void openExternalUrl(install.url);
  }, [install]);

  const confirmInstalled = useCallback(
    async (apply: (family: string) => void) => {
      if (!install) return;
      const list = await refresh();
      const entry = whitelistEntry(install.name);
      const hit = entry ? matchInstalledFamily(entry, list) : undefined;
      if (hit) {
        apply(hit.name);
        setInstall(null);
        setInstallMiss(false);
        return;
      }
      setInstallMiss(true);
    },
    [install, refresh],
  );

  return {
    families,
    options,
    isContinuous: familyIsContinuousWeight(currentFamily, families),
    defaultFamily: firstInstalledWhitelistFamily(families),
    install,
    installMiss,
    chooseFamily,
    cancelInstall,
    openInstallPage,
    confirmInstalled,
  };
}
