[CmdletBinding()]
param(
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "PrintLatch\bin"),
    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "PrintLatch"),
    [string]$TaskName = "PrintLatch Agent",
    [switch]$NoStartup
)

$ErrorActionPreference = "Stop"
$sourceRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$sourceExe = Join-Path $sourceRoot "printlatch.exe"
$sourceUninstaller = Join-Path $sourceRoot "uninstall.ps1"

if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    throw "printlatch.exe is missing beside install.ps1"
}

$resolvedInstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$resolvedDataDir = [System.IO.Path]::GetFullPath($DataDir)
New-Item -ItemType Directory -Force -Path $resolvedInstallRoot | Out-Null
New-Item -ItemType Directory -Force -Path $resolvedDataDir | Out-Null

$destinationExe = Join-Path $resolvedInstallRoot "printlatch.exe"
Copy-Item -LiteralPath $sourceExe -Destination $destinationExe -Force
if (Test-Path -LiteralPath $sourceUninstaller -PathType Leaf) {
    Copy-Item -LiteralPath $sourceUninstaller -Destination (Join-Path $resolvedInstallRoot "uninstall.ps1") -Force
}

$quotedDataDir = '"' + $resolvedDataDir.Replace('"', '""') + '"'
$arguments = "--data-dir $quotedDataDir serve"

if (-not $NoStartup) {
    $action = New-ScheduledTaskAction -Execute $destinationExe -Argument $arguments
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings -Description "Loopback-only PrintLatch PDF print agent" -Force | Out-Null
}

$process = Start-Process -FilePath $destinationExe -ArgumentList $arguments -WindowStyle Hidden -PassThru
$healthy = $false
for ($attempt = 0; $attempt -lt 30; $attempt++) {
    try {
        $health = Invoke-RestMethod -Uri "http://127.0.0.1:32191/health" -Headers @{ Host = "127.0.0.1:32191" } -TimeoutSec 2
        if ($health.status -eq "ok" -and $health.product -eq "PrintLatch") {
            $healthy = $true
            break
        }
    } catch {
        Start-Sleep -Milliseconds 250
    }
}

if (-not $healthy) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "PrintLatch did not become healthy on http://127.0.0.1:32191"
}

Write-Host "Installed PrintLatch at $destinationExe"
Write-Host "Data directory: $resolvedDataDir"
Write-Host "Health: ok"

