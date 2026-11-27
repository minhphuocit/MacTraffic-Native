$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
& cmd.exe /d /c package.cmd
if ($LASTEXITCODE -ne 0) { throw 'Rust core package failed' }
$msbuild = Get-Command msbuild.exe -ErrorAction SilentlyContinue
if (-not $msbuild) { throw 'MSBuild unavailable. Use GitHub Actions or a VS 2022 Developer PowerShell for the full package.' }
& $msbuild.Source 'blur-provider\ExplorerBlurMica.sln' /m /p:Configuration=Release /p:Platform=x64
if ($LASTEXITCODE -ne 0) { throw 'Blur provider compilation failed' }
$dll = Join-Path $PWD 'blur-provider\build\x64\Release\ExplorerBlurMica.dll'
if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) { throw 'Compiled blur provider missing' }
$stage = Join-Path $PWD 'dist-full'
if (Test-Path -LiteralPath $stage) { throw 'dist-full already exists. Move it aside before rebuilding to preserve its configuration.' }
New-Item -ItemType Directory -Path $stage | Out-Null
# Copy clean templates rather than a potentially user-edited dist INI.
Copy-Item -LiteralPath 'dist\MacTraffic.exe','MacTraffic.ini','Spacer.html','README.md','EXPLORER-BLUR.md','CLEAN-INSTALL.md' -Destination $stage
$provider = Join-Path $stage 'blur-provider'
New-Item -ItemType Directory -Path $provider | Out-Null
Copy-Item -LiteralPath $dll,'blur-provider\config.ini','blur-provider\Manage-Blur.ps1','blur-provider\LICENSE','blur-provider\COPYING.LESSER','blur-provider\README.md','blur-provider\UPSTREAM-README.txt' -Destination $provider
$archive = Join-Path $PWD 'release\MacTraffic-v1.0.2-full-win-x64.zip'
Compress-Archive -Path "$stage\*" -DestinationPath $archive -Force
$names = @('MacTraffic-v1.0.2-win-x64.zip','MacTraffic-v1.0.2-full-win-x64.zip')
$names | ForEach-Object { $hash = (Get-FileHash -LiteralPath (Join-Path $PWD "release\$_") -Algorithm SHA256).Hash; "$hash  $_" } | Set-Content -Encoding ascii 'release\SHA256SUMS.txt'
Write-Host "Ready: $archive"
