[CmdletBinding()]
param(
    [string]$ArchiveRoot,
    [ValidateRange(0, 65535)]
    [int]$Port = 0
)

$ErrorActionPreference = "Stop"
if (-not $ArchiveRoot) {
    if (Test-Path -LiteralPath (Join-Path $PSScriptRoot "docs\assets\sample.pdf")) {
        $ArchiveRoot = $PSScriptRoot
    } else {
        $ArchiveRoot = Split-Path -Parent $PSScriptRoot
    }
}
$runId = [Guid]::NewGuid().ToString("N")
$installRoot = Join-Path $env:LOCALAPPDATA "PrintLatchSmoke-$runId\bin"
$dataDir = Join-Path $env:LOCALAPPDATA "PrintLatchSmoke-$runId\data"
$taskName = "PrintLatch Smoke $runId"
$exe = Join-Path $installRoot "printlatch.exe"
$sample = Join-Path $ArchiveRoot "docs\assets\sample.pdf"
if ($Port -eq 0) {
    $portProbe = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Loopback,
        0
    )
    $portProbe.Start()
    $Port = ([System.Net.IPEndPoint]$portProbe.LocalEndpoint).Port
    $portProbe.Stop()
}
$baseUri = "http://127.0.0.1:$Port"

function New-ApiClient {
    param([string]$Token)
    $client = [System.Net.Http.HttpClient]::new()
    $client.BaseAddress = [Uri]$baseUri
    if ($Token) {
        $client.DefaultRequestHeaders.Authorization =
            [System.Net.Http.Headers.AuthenticationHeaderValue]::new("Bearer", $Token)
    }
    return $client
}

function Submit-Job {
    param(
        [System.Net.Http.HttpClient]$Client,
        [string]$Mode,
        [string]$Mime,
        [string]$PrinterId
    )
    $form = [System.Net.Http.MultipartFormDataContent]::new()
    $form.Add([System.Net.Http.StringContent]::new($Mode), "mode")
    $form.Add([System.Net.Http.StringContent]::new("1"), "copies")
    $form.Add([System.Net.Http.StringContent]::new($PrinterId), "printer_id")
    $file = [System.Net.Http.ByteArrayContent]::new([System.IO.File]::ReadAllBytes($sample))
    $file.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::new($Mime)
    $form.Add($file, "file", "sample.pdf")
    return $Client.PostAsync("/v1/jobs", $form).GetAwaiter().GetResult()
}

try {
    if (-not (Test-Path -LiteralPath $sample -PathType Leaf)) {
        throw "Sample PDF not found at $sample"
    }

    & (Join-Path $ArchiveRoot "install.ps1") `
        -InstallRoot $installRoot `
        -DataDir $dataDir `
        -TaskName $taskName `
        -Port $Port `
        -NoStartup `
        -NoDashboard

    $health = Invoke-RestMethod "$baseUri/health"
    if ($health.status -ne "ok" -or $health.bind -ne "loopback-only") {
        throw "Unexpected health response"
    }

    $pairOutput = (& $exe --data-dir $dataDir pair --origin "https://smoke.printlatch.test" --name "Release smoke") -join "`n"
    $pairCode = [regex]::Match(
        $pairOutput,
        "(?m)^Pairing code: (?<code>PL-[A-F0-9]{8}-[A-F0-9]{8}-[A-F0-9]{8}-[A-F0-9]{8})\r?$"
    ).Groups["code"].Value
    if (-not $pairCode) {
        throw "Pairing command did not return a code"
    }

    $pairClient = New-ApiClient
    $pairRequest = [System.Net.Http.HttpRequestMessage]::new("POST", "/v1/pair")
    $pairRequest.Headers.TryAddWithoutValidation("Origin", "https://smoke.printlatch.test") | Out-Null
    $pairRequest.Content = [System.Net.Http.StringContent]::new(
        "{`"code`":`"$pairCode`"}",
        [System.Text.Encoding]::UTF8,
        "application/json"
    )
    $pairResponse = $pairClient.Send($pairRequest)
    if (-not $pairResponse.IsSuccessStatusCode) {
        throw "Pairing failed with HTTP $([int]$pairResponse.StatusCode)"
    }
    $browserToken = ($pairResponse.Content.ReadAsStringAsync().Result | ConvertFrom-Json).token
    if (-not $browserToken.StartsWith("pl_live_")) {
        throw "Pairing did not return a token"
    }

    $replayRequest = [System.Net.Http.HttpRequestMessage]::new("POST", "/v1/pair")
    $replayRequest.Headers.TryAddWithoutValidation("Origin", "https://smoke.printlatch.test") | Out-Null
    $replayRequest.Content = [System.Net.Http.StringContent]::new(
        "{`"code`":`"$pairCode`"}",
        [System.Text.Encoding]::UTF8,
        "application/json"
    )
    $replayResponse = $pairClient.Send($replayRequest)
    if ([int]$replayResponse.StatusCode -ne 401) {
        throw "One-time pairing replay returned HTTP $([int]$replayResponse.StatusCode)"
    }
    $pairClient.Dispose()

    $tokenOutput = (& $exe --data-dir $dataDir clients create --name "Release smoke local" --days 1) -join "`n"
    $token = [regex]::Match($tokenOutput, "pl_live_[A-Za-z0-9_-]+").Value
    if (-not $token) {
        throw "Local token command did not return a token"
    }
    $api = New-ApiClient -Token $token

    $printers = $api.GetStringAsync("/v1/printers").Result | ConvertFrom-Json
    if (-not ($printers.printers.id -contains "capture:pdf")) {
        throw "Capture target was not enumerated"
    }

    $previewResponse = Submit-Job -Client $api -Mode "preview" -Mime "application/pdf" -PrinterId "capture:pdf"
    if ([int]$previewResponse.StatusCode -ne 202) {
        throw "Preview job returned HTTP $([int]$previewResponse.StatusCode)"
    }
    $preview = $previewResponse.Content.ReadAsStringAsync().Result | ConvertFrom-Json
    if ($preview.job.state -ne "preview_ready") {
        throw "Preview did not enter preview_ready"
    }
    $document = $api.GetByteArrayAsync("/v1/jobs/$($preview.job.id)/document").Result
    $sourceHash = (Get-FileHash -LiteralPath $sample -Algorithm SHA256).Hash
    $documentHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($document))
    if ($sourceHash -ne $documentHash) {
        throw "Preview document hash mismatch"
    }

    $captureResponse = Submit-Job -Client $api -Mode "print" -Mime "application/pdf" -PrinterId "capture:pdf"
    if ([int]$captureResponse.StatusCode -ne 202) {
        throw "Capture job returned HTTP $([int]$captureResponse.StatusCode)"
    }
    $capture = $captureResponse.Content.ReadAsStringAsync().Result | ConvertFrom-Json
    $finalState = $null
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        $current = $api.GetStringAsync("/v1/jobs/$($capture.job.id)").Result | ConvertFrom-Json
        $finalState = $current.job.state
        if ($finalState -in @("succeeded", "failed")) {
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if ($finalState -ne "succeeded") {
        throw "Capture ended in state $finalState"
    }
    $capturePath = Join-Path $dataDir "captures\$($capture.job.id).pdf"
    if (-not (Test-Path -LiteralPath $capturePath)) {
        throw "Capture artifact was not written"
    }
    if ((Get-FileHash -LiteralPath $capturePath -Algorithm SHA256).Hash -ne $sourceHash) {
        throw "Capture artifact hash mismatch"
    }

    $invalidResponse = Submit-Job -Client $api -Mode "preview" -Mime "application/octet-stream" -PrinterId "capture:pdf"
    if ([int]$invalidResponse.StatusCode -ne 400) {
        throw "Invalid MIME returned HTTP $([int]$invalidResponse.StatusCode)"
    }
    $api.Dispose()

    & (Join-Path $ArchiveRoot "uninstall.ps1") `
        -InstallRoot $installRoot `
        -DataDir $dataDir `
        -TaskName $taskName `
        -NoStartup
    if (Test-Path -LiteralPath $installRoot) {
        throw "Uninstall left program files behind"
    }
    if (-not (Test-Path -LiteralPath $dataDir)) {
        throw "Default uninstall did not preserve data"
    }

    & (Join-Path $ArchiveRoot "uninstall.ps1") `
        -InstallRoot $installRoot `
        -DataDir $dataDir `
        -TaskName $taskName `
        -NoStartup `
        -PurgeData
    if (Test-Path -LiteralPath (Split-Path -Parent $dataDir)) {
        Remove-Item -LiteralPath (Split-Path -Parent $dataDir) -Recurse -Force
    }

    Write-Host "PASS install"
    Write-Host "PASS health"
    Write-Host "PASS exact-origin one-time pairing"
    Write-Host "PASS authenticated preview and document hash"
    Write-Host "PASS PDF capture and output hash"
    Write-Host "PASS invalid MIME rejection"
    Write-Host "PASS uninstall preserve and purge"
} finally {
    Get-CimInstance Win32_Process -Filter "Name = 'printlatch.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.ExecutablePath -eq $exe } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
