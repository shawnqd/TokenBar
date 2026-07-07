import SwiftUI
import TokenBarCore

/// One provider's detail card: windows (progress + countdown), source, last
/// refresh, status. `unknown` is shown explicitly — no fabricated numbers.
struct ProviderCardView: View {
    let snapshot: ProviderSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(snapshot.providerID.displayName).font(.headline)
                Spacer()
                statusLabel
            }
            ForEach(snapshot.windows, id: \.kind) { window in
                windowRow(window)
            }
            Text("Updated \(snapshot.fetchedAt.formatted(.dateTime.hour().minute()))")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding()
    }

    private var statusLabel: some View {
        // TODO(phase1): derive worst status across sources.
        Text("unknown").font(.caption).foregroundStyle(.secondary)
    }

    @ViewBuilder
    private func windowRow(_ window: UsageWindow) -> some View {
        HStack {
            Text(window.kind.displayName).font(.subheadline)
            Spacer()
            if window.isUnknown {
                Text("unknown").foregroundStyle(.secondary)
            } else {
                // TODO(phase1): progress bar + used/total + reset countdown.
                Text("\(window.used ?? 0, specifier: "%.0f") / \(window.total ?? 0, specifier: "%.0f")")
            }
        }
    }
}
