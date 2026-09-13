# Browser cookies on Windows

TokenBar keeps cookie reading separate from provider authentication. For
providers that support web sessions, the default source is a cookie that you
explicitly save or import. Automatic browser discovery is an optional source
that can be enabled for an individual provider.

## Sources

- **Saved cookie**: use a cookie header or supported cookie file that you add
  in the provider settings. The value is stored through the protected Windows
  credential layer and is not shown in diagnostics.
- **Automatic browser reading**: when enabled for a provider, TokenBar checks
  supported browser profiles for that provider's cookie domain. It does not
  open a login window or operate a browser for you.
- **Provider login**: CLI/OAuth and API-key flows remain separate and appear
  only when the provider supports them.

Cookie source is not a login type. Changing it only changes where a refresh
looks for a web session; it does not create or remove an account.

## Automatic browser reading

On Windows, Chromium cookies are protected with the signed-in user's DPAPI key.
TokenBar can try Chrome, Edge, Brave, Firefox, and compatible Chromium profiles
when the provider allows it. Only the provider's configured domains and cookie
names are considered, and cookie values are kept in memory while a request is
made.

If a browser profile is locked or uses an encryption mode that cannot be read,
TokenBar reports the failure and leaves the saved-cookie and provider-specific
login options available. It never converts a failed read into a successful
quota value.

## Manual import

Paste a Cookie request header or import a supported cookie file in the provider
detail view. Save it before refreshing. The raw value is not written to logs,
diagnostics, or the tray panel.

## WSL

Windows browser databases and DPAPI are normally unavailable to a Linux process
inside WSL. Use the native Windows app for automatic browser reading, or use a
manual cookie / provider CLI flow from WSL.
