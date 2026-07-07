import SwiftUI
import TokenBarCore

struct SettingsView: View {
    var body: some View {
        Form {
            Section("Providers") {
                // TODO(phase1): list ProviderID.allCases with toggle + apiKey field.
                Text("Provider list — TODO(phase1)")
            }
            Section("Refresh") {
                // TODO(phase1): cadence picker (manual / 1m / 2m / 5m / 15m).
                Text("Refresh cadence — TODO(phase1)")
            }
        }
        .formStyle(.grouped)
        .padding()
        .frame(minWidth: 420, minHeight: 320)
    }
}
