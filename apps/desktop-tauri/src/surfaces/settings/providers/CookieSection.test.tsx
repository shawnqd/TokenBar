import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../../i18n/LocaleProvider";
import { buildBundle } from "../../../test/localeHarness";
import { CookieSection } from "./CookieSection";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
  getManualCookies: vi.fn(),
  setManualCookie: vi.fn(),
  removeManualCookie: vi.fn(),
  openProviderLogin: vi.fn(),
  captureProviderLogin: vi.fn(),
  closeProviderLogin: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({ listen: vi.fn() }));

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("@tauri-apps/api/event", () => eventMocks);

function savedCookie(providerId = "cursor") {
  return { providerId, provider: "Cursor", savedAt: "2026-08-02 10:00" };
}

async function click(element: HTMLElement) {
  await act(async () => {
    fireEvent.click(element);
  });
}

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
    tauriMocks.closeProviderLogin.mockResolvedValue(undefined);
  });

  /**
   * The reason this component gained a login path at all: pasting a Cookie
   * header by hand was the only way to read a web-only quota once Chrome's
   * App-Bound Encryption blocked automatic browser extraction.
   */
  it("offers signing in as well as pasting a cookie", async () => {
    renderSection();

    expect(await screen.findByText("ProviderLoginOpen")).toBeInTheDocument();
    expect(screen.getByText("BrowserCookieSave")).toBeInTheDocument();
  });

  /**
   * Capture is a button, not a poll. Only the user knows when the provider's
   * own flow — SSO, a second factor, an org picker — has finished, so the
   * import controls stay hidden until a window is actually open.
   */
  it("does not offer to import a session before a window is open", async () => {
    renderSection();

    await screen.findByText("ProviderLoginOpen");
    expect(screen.queryByText("ProviderLoginCapture")).not.toBeInTheDocument();
  });

  it("stores the session the login window captured", async () => {
    tauriMocks.openProviderLogin.mockResolvedValue({
      providerId: "cursor",
      provider: "Cursor",
      url: "https://cursor.com/",
    });
    tauriMocks.captureProviderLogin.mockResolvedValue([savedCookie()]);

    renderSection();

    await click(await screen.findByText("ProviderLoginOpen"));
    await click(await screen.findByText("ProviderLoginCapture"));

    await waitFor(() =>
      expect(tauriMocks.captureProviderLogin).toHaveBeenCalledWith("cursor"),
    );
    expect(await screen.findByText("BrowserCookieSavedBadge")).toBeInTheDocument();
    expect(screen.queryByText("ProviderLoginCapture")).not.toBeInTheDocument();
  });

  /**
   * Capturing too early is the expected mistake, and the window holds the
   * half-finished sign-in. Closing it on failure would throw that away and
   * force the user to start over.
   */
  it("keeps the window open when capture finds no session yet", async () => {
    tauriMocks.openProviderLogin.mockResolvedValue({
      providerId: "cursor",
      provider: "Cursor",
      url: "https://cursor.com/",
    });
    tauriMocks.captureProviderLogin.mockRejectedValue(
      new Error("No Cursor session found yet."),
    );

    renderSection();

    await click(await screen.findByText("ProviderLoginOpen"));
    await click(await screen.findByText("ProviderLoginCapture"));

    expect(
      await screen.findByText("No Cursor session found yet."),
    ).toBeInTheDocument();
    expect(screen.getByText("ProviderLoginCapture")).toBeInTheDocument();
  });

  /**
   * A window left open while the user clicks through to another provider would
   * be signed in to the wrong account by the time capture runs.
   */
  it("closes any login window when the selected provider changes", async () => {
    const { rerender } = renderSection();
    await screen.findByText("ProviderLoginOpen");
    // Mounting also clears any stale window, so only calls made *after* this
    // point prove the provider switch itself did the closing.
    tauriMocks.closeProviderLogin.mockClear();

    rerender(
      <LocaleProvider>
        <CookieSection providerId="claude" cookieDomain="claude.ai" />
      </LocaleProvider>,
    );

    await waitFor(() =>
      expect(tauriMocks.closeProviderLogin).toHaveBeenCalled(),
    );
  });
});
