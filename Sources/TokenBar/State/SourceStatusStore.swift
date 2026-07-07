import Foundation
import TokenBarCore

/// Tracks per-provider, per-source status for the "data source status" UI.
@MainActor
final class SourceStatusStore: ObservableObject {
    @Published private(set) var statuses: [ProviderID: [UsageSource: SourceStatus]] = [:]

    init() {}

    func apply(_ snapshot: ProviderSnapshot) {
        // TODO(phase1): mirror snapshot.sourceStatuses into `statuses`.
    }
}
