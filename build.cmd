@echo off
setlocal
cd /d "%~dp0"
cargo build --locked --release --target x86_64-pc-windows-gnu
if errorlevel 1 exit /b 1
if not exist dist mkdir dist
copy /y "target\x86_64-pc-windows-gnu\release\mactraffic.exe" "dist\MacTraffic.exe" >nul || exit /b 1
if not exist "dist\MacTraffic.ini" copy "MacTraffic.ini" "dist\MacTraffic.ini" >nul
if not exist "dist\MacTraffic.ini" exit /b 1
copy /y "Spacer.html" "dist\Spacer.html" >nul || exit /b 1
copy /y "README.md" "dist\README.md" >nul || exit /b 1
echo Ready: dist\MacTraffic.exe
exit /b 0
