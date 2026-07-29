[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "PrintLatch\bin"),
    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "PrintLatch"),
    [string]$TaskName = "PrintLatch Agent",
    [switch]$PurgeData,
    [switch]$NoStartup
)

$ErrorActionPreference = "Stop"
$resolvedInstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$resolvedDataDir = [System.IO.Path]::GetFullPath($DataDir)
$destinationExe = Join-Path $resolvedInstallRoot "printlatch.exe"

Get-CimInstance Win32_Process -Filter "Name = 'printlatch.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -eq $destinationExe } |
    ForEach-Object {
        Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
    }

if (-not $NoStartup) {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
}

if (Test-Path -LiteralPath $resolvedInstallRoot) {
    if ($PSCmdlet.ShouldProcess($resolvedInstallRoot, "Remove PrintLatch program files")) {
        Remove-Item -LiteralPath $resolvedInstallRoot -Recurse -Force
    }
}

if ($PurgeData -and (Test-Path -LiteralPath $resolvedDataDir)) {
    $localAppData = [System.IO.Path]::GetFullPath($env:LOCALAPPDATA)
    if (-not $resolvedDataDir.StartsWith($localAppData, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to purge a data directory outside LOCALAPPDATA"
    }
    if ($PSCmdlet.ShouldProcess($resolvedDataDir, "Permanently remove PrintLatch data")) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}

Write-Host "PrintLatch program files removed."
if ($PurgeData) {
    Write-Host "PrintLatch data removed."
} else {
    Write-Host "PrintLatch data preserved at $resolvedDataDir"
}

