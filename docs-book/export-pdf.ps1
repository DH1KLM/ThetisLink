param(
    [string]$ProjectRoot = "C:\Users\chiron.vanderburgt\claude\sdr-remote",
    [string]$OutputDir = "",
    [switch]$SkipBuild,
    [switch]$IncludeCombined
)

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $PSScriptRoot 'book'
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-docs.ps1') -ProjectRoot $ProjectRoot
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$browsers = @(
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
)

$browser = $browsers | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($null -eq $browser) {
    throw 'No headless browser found (Edge/Chrome).'
}

$jobs = @(
    # Nederlands
    @{ Html = 'installatie.html'; Pdf = 'Installatie.pdf' },
    @{ Html = 'user-manual.html'; Pdf = 'User-Manual.pdf' },
    @{ Html = 'technische-referentie.html'; Pdf = 'Technische-Referentie.pdf' },
    # English
    @{ Html = 'installation.html'; Pdf = 'Installation.pdf' },
    @{ Html = 'user-manual-en.html'; Pdf = 'User-Manual-EN.pdf' },
    @{ Html = 'technical-reference.html'; Pdf = 'Technical-Reference.pdf' }
)

if ($IncludeCombined) {
    $jobs += @{ Html = 'print.html'; Pdf = 'ThetisLink-Documentatie.pdf' }
}

foreach ($job in $jobs) {
    $htmlPath = Join-Path $PSScriptRoot (Join-Path 'book' $job.Html)
    if (-not (Test-Path $htmlPath)) {
        throw "HTML pagina niet gevonden: $htmlPath"
    }

    $pdfPath = Join-Path $OutputDir $job.Pdf
    if (Test-Path $pdfPath) {
        Remove-Item -LiteralPath $pdfPath -Force
    }

    $uri = ([System.Uri]$htmlPath).AbsoluteUri

    & $browser "--headless=new" "--disable-gpu" "--print-to-pdf=$pdfPath" "--print-to-pdf-no-header" "--no-pdf-header-footer" $uri *> $null
    $browserExit = $LASTEXITCODE

    for ($i = 0; $i -lt 20; $i++) {
        if (Test-Path $pdfPath) {
            $size = (Get-Item $pdfPath).Length
            if ($size -gt 0) { break }
        }
        Start-Sleep -Milliseconds 250
    }

    if (-not (Test-Path $pdfPath)) {
        throw "PDF is niet gegenereerd: $pdfPath"
    }

    $finalSize = (Get-Item $pdfPath).Length
    if ($finalSize -le 0) {
        throw "PDF bestaat maar is leeg: $pdfPath"
    }

    if (($null -ne $browserExit) -and ($browserExit -ne 0)) {
        Write-Warning "Browser gaf exit code $browserExit, maar PDF is wel succesvol aangemaakt: $pdfPath"
    }

    Write-Host "PDF generated: $pdfPath"
}

