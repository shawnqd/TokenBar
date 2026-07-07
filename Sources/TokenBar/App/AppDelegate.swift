import AppKit
import TokenBarCore

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let usageStore = UsageStore()
    private let settingsStore = SettingsStore()
    private let statusItemController = StatusItemController()

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Menu bar app: no Dock icon.
        NSApp.setActivationPolicy(.accessory)

        // TODO(phase1): wire usageStore -> statusItemController,
        // register providers, start refresh cadence from settingsStore.
    }
}
