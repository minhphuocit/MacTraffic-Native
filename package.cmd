@echo off
setlocal
cd /d "%~dp0"
call build.cmd
if errorlevel 1 exit /b 1
cargo test --locked --target x86_64-pc-windows-gnu
if errorlevel 1 exit /b 1
if not exist release mkdir release
powershell.exe -NoProfile -NonInteractive -Command "$ErrorActionPreference='Stop'; Compress-Archive -LiteralPath '.\dist\MacTraffic.exe','.\MacTraffic.ini','.\Spacer.html','.\README.md','.\EXPLORER-BLUR.md','.\CLEAN-INSTALL.md' -DestinationPath '.\release\MacTraffic-v1.0.2-win-x64.zip' -Force; $sha=[System.Security.Cryptography.SHA256]::Create(); try { $hash=[BitConverter]::ToString($sha.ComputeHash([IO.File]::ReadAllBytes((Resolve-Path -LiteralPath '.\release\MacTraffic-v1.0.2-win-x64.zip').ProviderPath))).Replace('-','') } finally { $sha.Dispose() }; ($hash + '  MacTraffic-v1.0.2-win-x64.zip') | Set-Content -Encoding ascii '.\release\SHA256SUMS.txt'"
if errorlevel 1 exit /b 1
echo Ready: release\MacTraffic-v1.0.2-win-x64.zip
exit /b 0
