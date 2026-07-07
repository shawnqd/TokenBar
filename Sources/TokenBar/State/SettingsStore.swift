import Foundation
import TokenBarCore

@MainActor
final class SettingsStore: ObservableObject {
    @Published var config: TokenBarConfig
    let configStore = ConfigStore()

    init() {
        config = configStore.load()
    }

    func setAPIKey(_ key: String?, for provider: ProviderID) {
        // TODO(phase1): update config.providers[provider.rawValue].apiKey,
        // persist via configStore with 0600. Never log the key.
    }

    func toggle(_ provider: ProviderID, enabled: Bool) {
        // TODO(phase1): update config.providers[provider.rawValue].enabled, persist.
    }
}
