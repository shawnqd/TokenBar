# TokenBar

[English](./README.md) | [简体中文](./README.zh-CN.md) | [繁體中文（臺灣）](./README.zh-TW.md) | [日本語](./README.ja-JP.md) | [한국어](./README.ko-KR.md) | [Español mexicano](./README.es-MX.md)

TokenBar는 Windows 시스템 트레이에서 AI 코딩 도구의 사용량을 확인하는
앱입니다. [CodexBar](https://github.com/steipete/CodexBar)의 사용량 흐름을
Tauri + React 데스크톱 셸과 공유 Rust 로직으로 Windows에 제공합니다.

<p align="center">
  <img src="docs/images/tray-panel.png" width="320" alt="TokenBar 트레이 패널"/>
  <img src="docs/images/settings-providers.png" width="520" alt="TokenBar 프로바이더 설정"/>
</p>

## 주요 기능

- 사용량 카드와 새로 고침을 제공하는 컴팩트한 트레이 패널.
- 프로바이더가 지원하는 인증 정보, 세션 쿠키, API 키, 리전, 표시 설정.
- 브라우저 쿠키 자동 읽기와 수동 가져오기를 별도 선택지로 제공하며, 앱 전용
  로그인 브라우저는 열지 않습니다.
- 사용량, 비용, 설정, 진단을 위한 로컬 CLI.
- Windows 설치형과 포터블 빌드, 공개 시 SHA-256 체크섬 파일 제공.

## 설치

[TokenBar Releases](https://github.com/shawnqd/TokenBar/releases)에서 최신
설치 파일이나 포터블 빌드를 받으세요. 현재 GitHub Releases로 배포하며 Winget
패키지는 아직 제공하지 않습니다.

## 처음 실행

1. 시작 메뉴 또는 포터블 실행 파일에서 TokenBar를 실행합니다.
2. 트레이 아이콘을 클릭해 패널을 엽니다.
3. **설정 → 프로바이더**를 열고 사용할 프로바이더를 켭니다.
4. 각 프로바이더가 지원하는 인증 소스(브라우저 자동 읽기, 저장/수동 쿠키,
   API 키, CLI/OAuth)만 설정합니다.

## 소스에서 빌드

요구 사항: Windows 10/11, pnpm이 포함된 Node.js, Rust.

```powershell
git clone https://github.com/shawnqd/TokenBar.git
cd TokenBar
pnpm --dir apps/desktop-tauri install
pnpm --dir apps/desktop-tauri tauri:dev
```

프로덕션 빌드:

```powershell
pnpm --dir apps/desktop-tauri tauri:build
```

자세한 내용은 [docs/BUILDING.md](docs/BUILDING.md)를 참조하세요.

## 개인정보 보호 및 macOS

프로바이더 데이터는 로컬 설정이나 사용자가 설정한 API에서 읽습니다. 쿠키 추출은
활성화한 프로바이더에만 수행되며 진단에 인증 정보가 포함되지 않습니다. TokenBar는
Windows용입니다. macOS에서는 [원본 CodexBar](https://github.com/steipete/CodexBar)를
사용하세요.

## 라이선스

MIT 라이선스입니다.
