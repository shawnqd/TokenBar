' Double-click entry point for Windows hot-reload development.
' WScript style 0 keeps PowerShell and the pnpm/Tauri chain hidden.
Option Explicit

Dim shell, fso, scriptDir, repoRoot, command, arg, arguments
Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

scriptDir = fso.GetParentFolderName(WScript.ScriptFullName)
repoRoot = fso.GetParentFolderName(scriptDir)
arguments = ""
For Each arg In WScript.Arguments
  arguments = arguments & " " & Chr(34) & Replace(arg, Chr(34), Chr(34) & Chr(34)) & Chr(34)
Next

command = "powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File " & _
  Chr(34) & scriptDir & "\dev-windows.ps1" & Chr(34) & arguments
shell.CurrentDirectory = repoRoot
shell.Run command, 0, True
