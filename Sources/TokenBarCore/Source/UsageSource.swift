import Foundation

/// Orthogonal data sources a provider may support. See docs/PROVIDER_SOURCE_STRATEGY.md.
public enum UsageSource: String, Sendable, CaseIterable, Codable {
    case apiKey
    case browserCookie
    case localFile
    case cliConfig
    case snapshot
}

/// Typed source error kept Sendable for Swift 6 concurrency.
public struct SourceError: Error, Sendable {
    public let message: String
    public init(_ message: String) { self.message = message }
}

/// Result of reading from a single source for a single provider.
public enum SourceReadResult: Sendable {
    case unavailable          // not configured / not opted in
    case failure(SourceError)
    case success(Data)        // raw payload; the provider parses it
}

/// A reader knows how to fetch one source kind. Implementations live in Core;
/// UI never implements this.
public protocol SourceReader: Sendable {
    var source: UsageSource { get }
    func read(for provider: ProviderID) async -> SourceReadResult
}
