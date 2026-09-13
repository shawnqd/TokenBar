# TokenBar

[English](./README.md) | [简体中文](./README.zh-CN.md) | [繁體中文（臺灣）](./README.zh-TW.md) | [日本語](./README.ja-JP.md) | [한국어](./README.ko-KR.md) | [Español mexicano](./README.es-MX.md)

TokenBar es una aplicación de bandeja del sistema para Windows que mantiene
visible el uso de herramientas de programación con IA. Lleva el flujo de uso
de [CodexBar](https://github.com/steipete/CodexBar) a una aplicación Tauri +
React con lógica compartida en Rust.

<p align="center">
  <img src="docs/images/tray-panel.png" width="320" alt="Panel de bandeja de TokenBar"/>
  <img src="docs/images/settings-providers.png" width="520" alt="Configuración de proveedores de TokenBar"/>
</p>

## Funciones

- Panel de bandeja compacto con tarjetas de uso y actualización.
- Configuración de credenciales, cookies de sesión, claves API, regiones y
  preferencias de visualización según las capacidades del proveedor.
- Lectura automática de cookies del navegador e importación manual como
  fuentes separadas; la aplicación no abre un navegador de inicio de sesión.
- CLI local para uso, costos, configuración y diagnósticos.
- Versiones instalable y portable para Windows con archivos SHA-256 cuando se
  publican.

## Instalación

Descarga el instalador o la versión portable desde
[TokenBar Releases](https://github.com/shawnqd/TokenBar/releases). TokenBar se
distribuye actualmente mediante GitHub Releases; todavía no hay paquete de
Winget.

## Primer uso

1. Inicia TokenBar desde el menú Inicio o el ejecutable portable.
2. Haz clic en el icono de la bandeja para abrir el panel.
3. Abre **Configuración → Proveedores** y activa los proveedores que uses.
4. Configura solo las fuentes de autenticación que admita cada proveedor:
   lectura automática del navegador, cookies guardadas/manuales, claves API o
   CLI/OAuth.

## Compilar desde el código fuente

Requisitos: Windows 10/11, Node.js con pnpm y Rust.

```powershell
git clone https://github.com/shawnqd/TokenBar.git
cd TokenBar
pnpm --dir apps/desktop-tauri install
pnpm --dir apps/desktop-tauri tauri:dev
```

Para una compilación de producción:

```powershell
pnpm --dir apps/desktop-tauri tauri:build
```

Consulta [docs/BUILDING.md](docs/BUILDING.md) para más comandos.

## Privacidad

Los datos de proveedores se leen desde la configuración local o las API que
configures. La extracción de cookies solo se ejecuta para proveedores activos
y los diagnósticos no incluyen credenciales.

## macOS y licencia

TokenBar se mantiene para Windows. En macOS, usa el
[CodexBar original](https://github.com/steipete/CodexBar). TokenBar se publica
bajo la licencia MIT.
