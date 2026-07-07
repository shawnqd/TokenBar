import AppKit
import TokenBarCore

/// Owns the single menu bar status item. Icon reflects the highest-usage window
/// across enabled providers; errors dim the icon.
@MainActor
final class StatusItemController {
    private let statusItem: NSStatusItem = {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        return item
    }()

    init() {
        statusItem.button?.title = "—"
        // TODO(phase1): build menu, wire to UsageStore, render usage meter icon.
    }
}
