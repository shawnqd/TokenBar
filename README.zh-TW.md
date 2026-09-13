# TokenBar

[English](./README.md) | [简体中文](./README.zh-CN.md) | [繁體中文（臺灣）](./README.zh-TW.md) | [日本語](./README.ja-JP.md) | [한국어](./README.ko-KR.md) | [Español mexicano](./README.es-MX.md)

TokenBar 是 Windows 系統匣應用程式，用來集中查看 AI 程式設計工具的使用量。它將 [CodexBar](https://github.com/steipete/CodexBar) 的用量工作流程帶到 Tauri + React 桌面殼層，並使用 Rust 共用服務商邏輯。

<p align="center">
  <img src="docs/images/tray-panel.png" width="320" alt="TokenBar 系統匣面板"/>
  <img src="docs/images/settings-providers.png" width="520" alt="TokenBar 服務商設定"/>
</p>

## 功能

- 系統匣面板顯示精簡的服務商額度卡片，並支援重新整理。
- 服務商設定可依能力提供憑據、工作階段 Cookie、API 金鑰、地區與顯示選項。
- 自動讀取瀏覽器工作階段與手動匯入 Cookie 是不同來源，不建立應用程式自有登入瀏覽器。
- 提供本機 CLI，用於查詢用量、成本、設定與診斷。
- 提供 Windows 安裝版與可攜版，並可在釋出頁取得 SHA-256 校驗檔。

## 安裝

請從 [TokenBar Releases](https://github.com/shawnqd/TokenBar/releases) 下載最新 Windows 安裝包或可攜版。可用資產與校驗檔以釋出頁為準。

目前 TokenBar 透過 GitHub Releases 發佈，尚未啟用 Winget。

## 首次執行

1. 從開始功能表或可攜版執行檔啟動 TokenBar。
2. 點選系統匣圖示開啟用量面板。
3. 開啟 **設定 → 服務商**，啟用需要的服務商。
4. 按服務商支援的方式設定來源：自動讀取瀏覽器、已儲存/手動 Cookie、API 金鑰或服務商 CLI/OAuth。

## 從原始碼建置

需求：Windows 10/11、Node.js（含 pnpm）與 Rust 工具鏈。

```powershell
git clone https://github.com/shawnqd/TokenBar.git
cd TokenBar
pnpm --dir apps/desktop-tauri install
pnpm --dir apps/desktop-tauri tauri:dev
```

建立正式版本：

```powershell
pnpm --dir apps/desktop-tauri tauri:build
```

更多建置說明請參閱 [docs/BUILDING.md](docs/BUILDING.md)。

## 隱私

- 服務商資料來自本機設定或使用者設定的服務商 API。
- 僅對已啟用的服務商讀取瀏覽器 Cookie，診斷資訊不包含 Cookie 原文。
- API 金鑰、手動 Cookie 與帳號資料使用 Windows 可用的受保護憑據儲存。
- 診斷只包含服務商、來源與狀態中繼資料，不包含憑據。

## macOS

TokenBar 維護 Windows 版本。macOS 使用者請直接使用原版 CodexBar：
[steipete/CodexBar](https://github.com/steipete/CodexBar)。

## 授權條款

TokenBar 使用 MIT 授權條款。專案參考了 [CodexBar](https://github.com/steipete/CodexBar) 與 [ccusage](https://github.com/ryoppippi/ccusage)。
