// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TokenBar",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "tokenbar", targets: ["TokenBarCLI"]),
        .library(name: "TokenBarCore", targets: ["TokenBarCore"]),
    ],
    targets: [
        // Core: fetch + parse + provider adapters + source readers + storage. No UI.
        .target(
            name: "TokenBarCore",
            path: "Sources/TokenBarCore"
        ),
        // App: menu bar status item, popover, provider cards, settings, state stores.
        .executableTarget(
            name: "TokenBar",
            dependencies: ["TokenBarCore"],
            path: "Sources/TokenBar"
        ),
        // Bundled CLI for scripts / CI.
        .executableTarget(
            name: "TokenBarCLI",
            dependencies: ["TokenBarCore"],
            path: "Sources/TokenBarCLI"
        ),
        .testTarget(
            name: "TokenBarTests",
            dependencies: ["TokenBarCore"],
            path: "Tests/TokenBarTests"
        ),
    ]
)
