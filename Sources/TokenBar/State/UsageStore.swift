import Foundation
import TokenBarCore

@MainActor
final class UsageStore: ObservableObject {
    enum RefreshState: Equatable { case idle, refreshing, error }

    @Published private(set) var snapshots: [ProviderID: ProviderSnapshot] = [:]
    @Published private(set) var refreshState: RefreshState = .idle

    let registry = ProviderRegistry()
    let snapshotStore = SnapshotStore()

    init() {
        // TODO(phase1): register DeepSeekProvider here.
    }

    func refresh(providerIDs: [ProviderID]) async {
        // TODO(phase1): for each id, resolve provider from registry,
        // build source readers from ConfigStore, call fetch, publish on MainActor.
        // Single provider failure must not abort the batch.
    }
}
