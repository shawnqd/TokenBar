import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../../i18n/LocaleProvider";
import { buildBundle } from "../../../test/localeHarness";
import { TEST_PROVIDER_CATALOG } from "../../../test/providerCatalog";
import {
  ProvidersSidebar,
  type ProviderSidebarRow,
} from "./ProvidersSidebar";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn(),
}));

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("@tauri-apps/api/event", () => eventMocks);

function rows(): ProviderSidebarRow[] {
  return TEST_PROVIDER_CATALOG.map(([id, displayName], index) => ({
    id,
    displayName,
    enabled: index < 2,
    status: index < 2 ? "ok" : "disabled",
    subtitlePrimary: index < 2 ? "auto" : "Disabled - auto",
    subtitleSecondary: index < 2 ? `${index + 1}%` : undefined,
  }));
}

describe("ProvidersSidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle({
      ProviderSidebarSearch: "Search",
      ProviderSidebarNoMatches: "No matching providers",
    }));
    eventMocks.listen.mockResolvedValue(() => {});
  });

  it("renders the full provider catalog without dropping rows", async () => {
    const { container } = render(
      <LocaleProvider>
        <ProvidersSidebar
          providers={rows()}
          selectedId="codex"
          searchText=""
          onSearchTextChange={vi.fn()}
          onSelect={vi.fn()}
          onReorder={vi.fn()}
          onToggleEnabled={vi.fn()}
        />
      </LocaleProvider>,
    );

    expect(await screen.findByRole("listbox", { name: "Providers" })).toBeInTheDocument();
    expect(screen.getAllByRole("option")).toHaveLength(TEST_PROVIDER_CATALOG.length);
    const names = Array.from(
      container.querySelectorAll(".providers-sidebar__name"),
      (node) => node.textContent,
    );
    expect(names).toEqual(TEST_PROVIDER_CATALOG.map(([, displayName]) => displayName));
  });

  it("reserves the secondary metadata line before a metric arrives", async () => {
    const { container } = render(
      <LocaleProvider>
        <ProvidersSidebar
          providers={rows()}
          selectedId="codex"
          searchText=""
          onSearchTextChange={vi.fn()}
          onSelect={vi.fn()}
          onReorder={vi.fn()}
          onToggleEnabled={vi.fn()}
        />
      </LocaleProvider>,
    );

    await screen.findByRole("listbox", { name: "Providers" });
    const secondaryLines = Array.from(
      container.querySelectorAll(".providers-sidebar__subtitle-secondary"),
    );
    expect(secondaryLines).toHaveLength(TEST_PROVIDER_CATALOG.length);
    expect(secondaryLines[0]).toHaveTextContent("1%");
    expect(secondaryLines[2].textContent).toBe("\u00a0");
    expect(secondaryLines[2]).toHaveAttribute("aria-hidden", "true");
  });

  it("renders provider search and empty matches state", async () => {
    render(
      <LocaleProvider>
        <ProvidersSidebar
          providers={[]}
          selectedId={null}
          searchText="zzzz"
          onSearchTextChange={vi.fn()}
          onSelect={vi.fn()}
          onReorder={vi.fn()}
          onToggleEnabled={vi.fn()}
        />
      </LocaleProvider>,
    );

    expect(await screen.findByRole("searchbox", { name: "Search" })).toBeInTheDocument();
    expect(screen.getByText("No matching providers")).toBeInTheDocument();
  });

  it("reorders providers via Alt+ArrowDown on the focused row", async () => {
    const onReorder = vi.fn();
    const { container } = render(
      <LocaleProvider>
        <ProvidersSidebar
          providers={rows()}
          selectedId="codex"
          searchText=""
          onSearchTextChange={vi.fn()}
          onSelect={vi.fn()}
          onReorder={onReorder}
          onToggleEnabled={vi.fn()}
        />
      </LocaleProvider>,
    );

    const codexRow = await screen.findByRole("option", { name: /Codex/ });
    fireEvent.keyDown(codexRow, { key: "ArrowDown", altKey: true });

    await waitFor(() => {
      const names = Array.from(
        container.querySelectorAll(".providers-sidebar__name"),
        (node) => node.textContent,
      );
      expect(names.slice(0, 3)).toEqual(["Claude", "Codex", "Cursor"]);
    });
    expect(onReorder).toHaveBeenCalledWith([
      "claude",
      "codex",
      ...TEST_PROVIDER_CATALOG.slice(2).map(([id]) => id),
    ]);
  });

  it("does not reorder past the first row on Alt+ArrowUp", async () => {
    const onReorder = vi.fn();
    render(
      <LocaleProvider>
        <ProvidersSidebar
          providers={rows()}
          selectedId="codex"
          searchText=""
          onSearchTextChange={vi.fn()}
          onSelect={vi.fn()}
          onReorder={onReorder}
          onToggleEnabled={vi.fn()}
        />
      </LocaleProvider>,
    );

    const firstRow = await screen.findByRole("option", { name: /Codex/ });
    fireEvent.keyDown(firstRow, { key: "ArrowUp", altKey: true });

    expect(onReorder).not.toHaveBeenCalled();
  });
});
