param(
    [string]$ProjectRoot = "C:\Users\chiron.vanderburgt\claude\sdr-remote"
)

$cargoBin = Join-Path $HOME '.cargo\bin'
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$mdbook = Join-Path $cargoBin 'mdbook.exe'
if (-not (Test-Path $mdbook)) {
    $cmd = Get-Command mdbook -ErrorAction SilentlyContinue
    if ($null -eq $cmd) {
        throw 'mdbook not found. Run: cargo install mdbook mdbook-mermaid'
    }
    $mdbook = $cmd.Source
}

& (Join-Path $PSScriptRoot 'sync-docs.ps1') -ProjectRoot $ProjectRoot
if (-not $?) { exit 1 }

& $mdbook build $PSScriptRoot
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Book generated at: $(Join-Path $PSScriptRoot 'book\index.html')"
Write-Host "Printable HTML: $(Join-Path $PSScriptRoot 'book\print.html')"
