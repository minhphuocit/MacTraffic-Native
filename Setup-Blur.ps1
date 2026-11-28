param([ValidateSet('Enable','Disable')][string]$Action)
$ErrorActionPreference = 'Stop'
$delegated = $false
try {
    $manager = Join-Path $PSScriptRoot 'blur-provider\Manage-Blur.ps1'
    if (-not (Test-Path -LiteralPath $manager -PathType Leaf)) {
        throw 'Place Setup-Blur.ps1 beside MacTraffic.exe in the extracted Full package.'
    }
    if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit Windows PowerShell.' }
    if (-not $Action) {
        Write-Host '1. Enable Explorer Blur'
        Write-Host '2. Disable Explorer Blur'
        $choice = Read-Host 'Choose 1 or 2 (anything else cancels)'
        switch ($choice) {
            '1' { $Action = 'Enable' }
            '2' { $Action = 'Disable' }
            default { return }
        }
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        $powershell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $arguments = '-NoProfile -File "' + $PSCommandPath + '" -Action ' + $Action
        Start-Process -FilePath $powershell -ArgumentList $arguments -Verb RunAs -Wait -ErrorAction Stop
        $delegated = $true
        return
    }
    Write-Host 'MacTraffic - optional Explorer Blur setup'
    Write-Host 'Close MacTraffic with Ctrl+Alt+F12 before proceeding.'
    Write-Host "Selected action: $Action"
    & $manager -Action $Action
} catch {
    Write-Host $_.Exception.Message -ForegroundColor Red
} finally {
    if (-not $delegated) {
        [void](Read-Host 'Press Enter to close')
    }
}
