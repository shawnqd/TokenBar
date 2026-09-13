# TokenBar

[English](./README.md) | [简体中文](./README.zh-CN.md) | [繁體中文（臺灣）](./README.zh-TW.md) | [日本語](./README.ja-JP.md) | [한국어](./README.ko-KR.md) | [Español mexicano](./README.es-MX.md)

TokenBar は、AI コーディングツールの使用量を Windows のシステムトレイ
から確認できるアプリです。[CodexBar](https://github.com/steipete/CodexBar)
の使用量ワークフローを Tauri + React のデスクトップシェルと共有 Rust
ロジックで Windows に提供します。

<p align="center">
  <img src="docs/images/tray-panel.png" width="320" alt="TokenBar トレイパネル"/>
  <img src="docs/images/settings-providers.png" width="520" alt="TokenBar プロバイダー設定"/>
</p>

## 主な機能

- 使用量カードと更新操作を備えたコンパクトなトレイパネル。
- プロバイダーが対応する場合の認証情報、セッション Cookie、API キー、
  リージョン、表示設定。
- ブラウザー Cookie の自動読み取りと手動インポートを別の選択肢として提供。
  アプリ独自のログインブラウザーは開きません。
- 使用量、コスト、設定、診断を扱うローカル CLI。
- Windows 用インストーラーとポータブル版、公開時は SHA-256 チェックサム付き。

## インストール

[TokenBar Releases](https://github.com/shawnqd/TokenBar/releases) から最新の
インストーラーまたはポータブル版をダウンロードしてください。現在は
GitHub Releases から配布しており、Winget パッケージはまだありません。

## 初回起動

1. スタートメニューまたはポータブル exe から TokenBar を起動します。
2. トレイアイコンをクリックしてパネルを開きます。
3. **設定 → プロバイダー**を開き、使用するプロバイダーを有効にします。
4. 各プロバイダーが対応する認証ソース（ブラウザー自動読み取り、保存/手動
   Cookie、API キー、CLI/OAuth）だけを設定します。

## ソースからビルド

要件: Windows 10/11、pnpm を含む Node.js、Rust。

```powershell
git clone https://github.com/shawnqd/TokenBar.git
cd TokenBar
pnpm --dir apps/desktop-tauri install
pnpm --dir apps/desktop-tauri tauri:dev
```

本番ビルド:

```powershell
pnpm --dir apps/desktop-tauri tauri:build
```

詳細は [docs/BUILDING.md](docs/BUILDING.md) を参照してください。

## プライバシーと macOS

プロバイダーデータはローカル設定または設定した API から読み取ります。
Cookie の抽出は有効にしたプロバイダーに対してだけ実行され、診断に認証情報は
含まれません。TokenBar は Windows 向けです。macOS では
[CodexBar original](https://github.com/steipete/CodexBar) を使用してください。

## ライセンス

MIT ライセンスです。
