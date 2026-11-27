param([Parameter(Mandatory=$true)][ValidateSet('Enable','Disable')][string]$Action)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit Windows PowerShell.' }
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Open Windows PowerShell as Administrator, then run this script explicitly.' }
$dll = Join-Path $PSScriptRoot 'ExplorerBlurMica.dll'
if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) { throw 'Missing compiled blur provider. Use the full Windows CI package.' }
if ($dll.Length -ge 260) { throw 'Use a shorter permanent folder path for the legacy extension.' }
$key = [Microsoft.Win32.Registry]::ClassesRoot.OpenSubKey('CLSID\{B44BD3C8-E597-4E08-AE43-246CE24698E7}\InProcServer32')
$registered = $null
if ($key) { try { $registered = [string]$key.GetValue('') } finally { $key.Dispose() } }
if ($registered -and ($registered -ine $dll)) { throw "Another ExplorerBlurMica DLL is registered: $registered. Uninstall it using its own uninstaller before proceeding." }
if ($Action -eq 'Disable' -and -not $registered) { Write-Host 'No matching registered provider.'; return }
if ($Action -eq 'Enable' -and $registered) { Write-Host 'This provider is already registered. Restart Windows if needed.'; return }
if (Get-Process -Name MacTraffic,MacTrafficCaptionLab -ErrorAction SilentlyContinue) { throw 'Exit MacTraffic normally (Ctrl+Alt+F12) first.' }
Write-Host "$Action ExplorerBlurMica COM shell extension: $dll"
Write-Host 'This explicitly changes COM registration and loads the provider into Explorer after a Windows restart. Keep this folder until the provider has been unregistered.'
if ((Read-Host 'Type YES to continue') -cne 'YES') { return }
$regsvr = Join-Path $env:WINDIR 'System32\regsvr32.exe'
$arguments = if ($Action -eq 'Disable') { '/u "' + $dll + '"' } else { '"' + $dll + '"' }
$proc = Start-Process -FilePath $regsvr -ArgumentList $arguments -Wait -PassThru
if ($proc.ExitCode -ne 0) { throw "Registration command failed: $($proc.ExitCode)" }
Write-Host 'Completed. Save your work and restart Windows manually. Do not delete the folder while the provider is registered.'
