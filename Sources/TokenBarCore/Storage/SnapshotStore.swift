import Foundation

/// Persists the last successful snapshot per provider, used only as a fallback
/// when a fresh fetch fails. A fallback snapshot is ALWAYS marked `stale` in
/// the UI — it never impersonates fresh data.
public struct SnapshotStore: Sendable {
    public init() {}

    public struct Entry: Sendable {
        public let snapshot: ProviderSnapshot
        public let storedAt: Date
    }

    public func load(for id: ProviderID) -> Entry? {
        // TODO(phase1): read last successful snapshot from disk.
        return nil
    }

    public func save(_ snapshot: ProviderSnapshot) {
        // TODO(phase1): persist only on a successful (ok) fetch.
    }
}
