import Foundation

/// On-disk TokenBar config. Schema-versioned for forward migrations.
/// Keys are `ProviderID.rawValue` strings (JSON requires string keys).
public struct TokenBarConfig: Codable, Sendable {
    public var schemaVersion: Int
    public var providers: [String: ProviderConfig]
    public var refreshCadenceSeconds: Int

    public init(
        schemaVersion: Int = 1,
        providers: [String: ProviderConfig] = [:],
        refreshCadenceSeconds: Int = 300
    ) {
        self.schemaVersion = schemaVersion
        self.providers = providers
        self.refreshCadenceSeconds = refreshCadenceSeconds
    }
}

/// Per-provider config. `apiKey` is never logged and never exported raw.
public struct ProviderConfig: Codable, Sendable {
    public var enabled: Bool
    public var apiKey: String?

    public init(enabled: Bool = false, apiKey: String? = nil) {
        self.enabled = enabled
        self.apiKey = apiKey
    }
}

/// Reads / writes `~/.config/tokenbar/config.json` with restrictive permissions.
/// Phase 0: stub. Phase 1: real I/O with `0600`.
public struct ConfigStore: Sendable {
    public init() {}

    public func load() -> TokenBarConfig {
        // TODO(phase1): read & decode from disk with 0600.
        return TokenBarConfig()
    }

    public func save(_ config: TokenBarConfig) {
        // TODO(phase1): encode & write to disk with 0600. Never print apiKey.
    }
}
