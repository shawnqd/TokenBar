import Foundation

/// A provider adapter. UI is agnostic to concrete providers — it only talks
/// to this protocol. Add new providers without touching UI.
public protocol Provider: Sendable {
    var id: ProviderID { get }
    var displayName: String { get }
    var supportedSources: Set<UsageSource> { get }

    /// Fetch a fresh snapshot using the configured source readers.
    /// On any failure, return a snapshot with `nil` window values and the
    /// corresponding `.error` / `.unknown` source statuses. Never fabricate.
    func fetch(using readers: [UsageSource: any SourceReader]) async -> ProviderSnapshot
}

/// Central registry. Providers register themselves; the app looks up by id.
public final class ProviderRegistry: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [ProviderID: any Provider] = [:]

    public init() {}

    public func register(_ provider: any Provider) {
        lock.lock(); defer { lock.unlock() }
        storage[provider.id] = provider
    }

    public func provider(for id: ProviderID) -> (any Provider)? {
        lock.lock(); defer { lock.unlock() }
        return storage[id]
    }

    public var allIDs: [ProviderID] {
        lock.lock(); defer { lock.unlock() }
        return Array(storage.keys)
    }
}
