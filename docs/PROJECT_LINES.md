# 项目线与上游边界

本仓库同时保留 macOS 与 Windows 两条**独立项目线**。它们不是同一个应用的互相移植，也不是可以直接互相合并代码的上下游关系。

| 目标分支 | 直接代码来源 | 技术栈 | 允许追踪的上游 |
| --- | --- | --- | --- |
| `platform/macos` | TokenBar 自己的 macOS 原型 | Swift / SwiftUI | macOS 项目自身的产品与技术决策 |
| `platform/windows` | `Finesssee/Win-CodexBar` | Tauri / React / Rust | `Finesssee/Win-CodexBar` |

## 强制规则

1. 处理问题前先确认目标分支。Windows 的问题只能先查 Windows 直接上游；不得引用 macOS 线的实现作为已验证的 Windows 解决方案。
2. `steipete/CodexBar` 仅是 Windows 基线 README 中列出的历史灵感来源。它不是 TokenBar macOS 分支的上游，也不是 Windows 分支可以直接合并的代码源。
3. 不跨线推断窗口、托盘、Cookie、认证、更新或 UI 行为。相同名称不代表相同架构或能力。
4. 共享产品规则、分支说明和决策记录只写入 `main` 文档；应用代码必须留在对应平台分支。
5. 新模型或新开发者开始工作时，必须先阅读本文件和 `main` 的 branch map。

## 当前 Windows Cookie 结论

Windows Chrome / Edge 的 App-Bound Encryption 是 Windows 线的浏览器安全约束。任何修复或替代流程必须先在 `Finesssee/Win-CodexBar` 的代码与提交记录中查证；不得把 macOS Keychain 的 Cookie 读取方式称作 Windows 可直接移植的解决方案。
