import Foundation

/// All long-term provider identifiers. Do not delete entries to "simplify" —
/// the full scope is intentional and implemented in phases.
public enum ProviderID: String, Sendable, CaseIterable, Codable {
    case codex
    case openCodeGo
    case miniMax
    case miMo
    case deepSeek
    case doubao
    case volcAgent

    public var displayName: String {
        switch self {
        case .codex: "Codex"
        case .openCodeGo: "OpenCode Go"
        case .miniMax: "MiniMax"
        case .miMo: "MiMo"
        case .deepSeek: "DeepSeek"
        case .doubao: "Doubao / Volcengine Ark"
        case .volcAgent: "Volc Engine Agent"
        }
    }
}
