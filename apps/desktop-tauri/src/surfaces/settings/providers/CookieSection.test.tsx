import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../../i18n/LocaleProvider";
import { buildBundle } from "../../../test/localeHarness";
import { CookieSection } from "./CookieSection";
import { createActionDispatcher } from "../../../core/actionDispatcher";
import { setCoreBridgeDispatcher } from "../../../core/useCoreBridge";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
  getManualCookies: vi.fn(),
  setManualCookie: vi.fn(),
  removeManualCookie: vi.fn(),
  invokeSurfaceAction: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({ listen: vi.fn() }));

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("@tauri-apps/api/event", () => eventMocks);

function renderSection(providerId = "cursor") {
  return render(
    <LocaleProvider>
      <CookieSection providerId={providerId} cookieDomain="cursor.com" />
    </LocaleProvider>,
  );
}

describe("CookieSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle());
    eventMocks.listen.mockResolvedValue(() => {});
    tauriMocks.getManualCookies.mockResolvedValue([]);
    tauriMocks.invokeSurfaceAction.mockResolvedValue("ok");
    setCoreBridgeDispatcher(
      createActionDispatcher(
        {},
        {
          fallback: async (action) => {
            const data = await tauriMocks.invokeSurfaceAction(action);
            return { status: "handled", data };
          },
        },
      ),
    );
  });

  it("keeps automatic browser cookies as the primary path", async () => {
    renderSection();

    expect(await screen.findByText("BrowserCookieImportHint")).toBeInTheDocument();
    expect(screen.queryByText("ProviderLoginOpen")).not.toBeInTheDocument();
    expect(screen.queryByText("ProviderLoginCapture")).not.toBeInTheDocument();
    expect(screen.getByText("BrowserCookieSave")).toBeInTheDocument();
  });

  it("places manual cookie entry behind the advanced disclosure", async () => {
    renderSection();

    expect(await screen.findByText("TabAdvanced")).toBeInTheDocument();
    const details = screen.getByText("TabAdvanced").closest("details");
    expect(details).not.toBeNull();
    expect(details?.open).toBe(false);
    fireEvent.click(screen.getByText("TabAdvanced"));
    expect(details?.open).toBe(true);
  });
});
