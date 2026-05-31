param(
    [string]$ProjectRoot = "C:\Users\chiron.vanderburgt\claude\sdr-remote"
)

$src = Join-Path $PSScriptRoot 'src'

Copy-Item -LiteralPath (Join-Path $ProjectRoot 'Installatie.md') -Destination (Join-Path $src 'installatie.md') -Force
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'User-Manual.md') -Destination (Join-Path $src 'user-manual.md') -Force
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'Technische-Referentie.md') -Destination (Join-Path $src 'technische-referentie.md') -Force

# English versions
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'Installation.md') -Destination (Join-Path $src 'installation.md') -Force
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'User-Manual-EN.md') -Destination (Join-Path $src 'user-manual-en.md') -Force
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'Technical-Reference.md') -Destination (Join-Path $src 'technical-reference.md') -Force

Write-Host 'Synced source markdown files into docs-book/src (NL + EN).'
