import XCTest
@testable import TokenBarCore

final class TokenBarTests: XCTestCase {
    /// Guard against accidentally cutting the long-term provider scope.
    func testProviderIDContainsAllLongTermProviders() {
        let ids = Set(ProviderID.allCases)
        XCTAssertEqual(ids, [
            .codex, .openCodeGo, .miniMax, .miMo,
            .deepSeek, .doubao, .volcAgent,
        ])
    }

    /// All source kinds must remain available — sources are orthogonally composable.
    func testUsageSourceContainsAllSourceKinds() {
        let sources = Set(UsageSource.allCases)
        XCTAssertEqual(sources, [
            .apiKey, .browserCookie, .localFile, .cliConfig, .snapshot,
        ])
    }

    /// All window kinds must remain available — multi-window rendering is core.
    func testWindowKindContainsAllWindowKinds() {
        let kinds = Set(WindowKind.allCases)
        XCTAssertEqual(kinds, [
            .fiveHour, .sevenDay, .weekly, .monthly,
            .balance, .count, .token, .requestLimit,
        ])
    }

    /// A window with no values is unknown — the contract we never violate.
    func testUnknownWindowIsUnknown() {
        let w = UsageWindow(kind: .balance, used: nil, total: nil, resetsAt: nil)
        XCTAssertTrue(w.isUnknown)
    }
}
