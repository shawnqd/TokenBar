import type { TaskbarFontFamily } from "../types/bridge";

/** DESIGN_SYSTEM.md §2 — the only families the font picker may offer. */
export const FONT_WHITELIST = [
  {
    name: "MiSans VF",
    label: "MiSans VF",
    aliases: ["misans vf", "misans"],
  },
  {
    name: "Source Han Sans VF",
    label: "Source Han Sans VF",
    aliases: [
      "source han sans vf",
      "source han sans cn vf",
      "source han sans sc vf",
      "source han sans sc",
      "source han sans",
      "思源黑体",
    ],
  },
  {
    name: "Noto Sans CJK VF",
    label: "Noto Sans CJK VF",
    aliases: [
      "noto sans cjk vf",
      "noto sans cjk sc vf",
      "noto sans cjk sc",
      "noto sans sc",
    ],
  },
  {
    name: "Source Han Serif VF",
    label: "Source Han Serif VF",
    aliases: [
      "source han serif vf",
      "source han serif cn vf",
      "source han serif sc vf",
      "source han serif sc",
      "source han serif",
      "思源宋体",
    ],
  },
  {
    name: "Noto Serif CJK VF",
    label: "Noto Serif CJK VF",
    aliases: [
      "noto serif cjk vf",
      "noto serif cjk sc vf",
      "noto serif cjk sc",
      "noto serif sc",
    ],
  },
] as const;

export const FONT_WHITELIST_DEFAULT = FONT_WHITELIST[0].name;

/** Only this family ships in the app package. The other four are install-guided. */
export const BUNDLED_FONT_NAME = "MiSans VF";

export type FontInstallGuide = {
  name: string;
  label: string;
  url: string;
  file: string;
};

export const FONT_INSTALL_GUIDES: readonly FontInstallGuide[] = [
  {
    name: "Source Han Sans VF",
    label: "Source Han Sans VF（思源黑体）",
    url: "https://github.com/adobe-fonts/source-han-sans/tree/release/Variable/TTF/Subset",
    file: "SourceHanSansCN-VF.ttf",
  },
  {
    name: "Noto Sans CJK VF",
    label: "Noto Sans CJK VF",
    url: "https://github.com/notofonts/noto-cjk/tree/main/Sans/Variable/TTF/Subset",
    file: "NotoSansSC-VF.ttf",
  },
  {
    name: "Source Han Serif VF",
    label: "Source Han Serif VF（思源宋体）",
    url: "https://github.com/adobe-fonts/source-han-serif/tree/release/Variable/TTF/Subset",
    file: "SourceHanSerifCN-VF.ttf",
  },
  {
    name: "Noto Serif CJK VF",
    label: "Noto Serif CJK VF",
    url: "https://github.com/notofonts/noto-cjk/tree/main/Serif/Variable/TTF/Subset",
    file: "NotoSerifSC-VF.ttf",
  },
];

function normalize(name: string): string {
  return name.trim().toLowerCase();
}

function aliasesInclude(
  aliases: readonly string[],
  name: string,
): boolean {
  return aliases.includes(name);
}

function aliasesOf(name: string): readonly string[] {
  const n = normalize(name);
  const entry = FONT_WHITELIST.find(
    (item) => normalize(item.name) === n || aliasesInclude(item.aliases, n),
  );
  return entry ? [normalize(entry.name), ...entry.aliases] : [n];
}

export function isWhitelistedFamily(name: string): boolean {
  const n = normalize(name);
  return FONT_WHITELIST.some(
    (entry) => normalize(entry.name) === n || aliasesInclude(entry.aliases, n),
  );
}

export function matchInstalledFamily(
  entry: (typeof FONT_WHITELIST)[number],
  installed: TaskbarFontFamily[],
): TaskbarFontFamily | undefined {
  return installed.find((family) => {
    const n = normalize(family.name);
    return normalize(entry.name) === n || aliasesInclude(entry.aliases, n);
  });
}

export function firstInstalledWhitelistFamily(
  installed: TaskbarFontFamily[],
): string {
  for (const entry of FONT_WHITELIST) {
    const hit = matchInstalledFamily(entry, installed);
    if (hit) return hit.name;
  }
  return FONT_WHITELIST_DEFAULT;
}

export function buildFontPickerOptions(
  installed: TaskbarFontFamily[],
  currentFamily: string,
  unavailableLabel = "未安装",
): { value: string; label: string }[] {
  const options = FONT_WHITELIST.map((entry) => {
    const hit = matchInstalledFamily(entry, installed);
    return {
      value: hit?.name ?? entry.name,
      label: hit ? entry.label : `${entry.label} · ${unavailableLabel}`,
    };
  });
  if (
    currentFamily &&
    !options.some((option) => option.value === currentFamily) &&
    !isWhitelistedFamily(currentFamily)
  ) {
    return [{ value: currentFamily, label: currentFamily }, ...options];
  }
  return options;
}

export function familyIsContinuousWeight(
  name: string,
  installed: TaskbarFontFamily[],
): boolean {
  if (isWhitelistedFamily(name)) return true;
  const n = normalize(name);
  return (
    installed.find((family) => normalize(family.name) === n)?.variableWeight ??
    false
  );
}

export function sameFontFamily(a: string, b: string): boolean {
  const left = aliasesOf(a);
  const right = normalize(b);
  return left.includes(right);
}

export function isBundledFamily(name: string): boolean {
  return sameFontFamily(name, BUNDLED_FONT_NAME);
}

export function whitelistEntry(
  name: string,
): (typeof FONT_WHITELIST)[number] | undefined {
  const n = normalize(name);
  return FONT_WHITELIST.find(
    (entry) => normalize(entry.name) === n || aliasesInclude(entry.aliases, n),
  );
}

export function fontInstallGuide(name: string): FontInstallGuide | undefined {
  const entry = whitelistEntry(name);
  if (!entry || isBundledFamily(entry.name)) return undefined;
  return FONT_INSTALL_GUIDES.find((guide) => guide.name === entry.name);
}

/** Bundled, already installed, or a previously saved non-whitelist family. */
export function isFamilyReady(
  name: string,
  installed: TaskbarFontFamily[],
): boolean {
  if (!name || isBundledFamily(name)) return true;
  const entry = whitelistEntry(name);
  if (!entry) return true;
  return Boolean(matchInstalledFamily(entry, installed));
}
