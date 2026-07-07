import Foundation

/// Quota window kinds. A provider may expose several at once.
public enum WindowKind: String, Sendable, CaseIterable, Codable {
    case fiveHour
    case sevenDay
    case weekly
    case monthly
    case balance
    case count
    case token
    case requestLimit

    public var displayName: String {
        switch self {
        case .fiveHour: "5-hour window"
        case .sevenDay: "7-day window"
        case .weekly: "Weekly"
        case .monthly: "Monthly"
        case .balance: "Balance"
        case .count: "Count"
        case .token: "Token"
        case .requestLimit: "Request limit"
        }
    }
}

/// A single usage window. `nil` means **unknown** — never fabricate.
public struct UsageWindow: Sendable {
    public let kind: WindowKind
    public let used: Double?
    public let total: Double?
    public let resetsAt: Date?

    public init(kind: WindowKind, used: Double?, total: Double?, resetsAt: Date?) {
        self.kind = kind
        self.used = used
        self.total = total
        self.resetsAt = resetsAt
    }

    public var isUnknown: Bool { used == nil && total == nil }
}

/// Per-source status, independent for every provider.
public enum SourceStatus: String, Sendable {
    case unknown
    case idle
    case fetching
    case ok
    case stale
    case error
}

/// Immutable snapshot produced by a provider fetch.
public struct ProviderSnapshot: Sendable {
    public let providerID: ProviderID
    public let windows: [UsageWindow]
    public let sourceStatuses: [UsageSource: SourceStatus]
    public let fetchedAt: Date

    public init(
        providerID: ProviderID,
        windows: [UsageWindow],
        sourceStatuses: [UsageSource: SourceStatus],
        fetchedAt: Date
    ) {
        self.providerID = providerID
        self.windows = windows
        self.sourceStatuses = sourceStatuses
        self.fetchedAt = fetchedAt
    }
}
