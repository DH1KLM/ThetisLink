# docs-book

Deze map bevat de mdBook-build van de hoofdhandleidingen.

## Brondocumenten
- `../Installatie.md`
- `../User-Manual.md`
- `../Technische-Referentie.md`

## Eenmalig
```powershell
cargo install mdbook mdbook-mermaid
mdbook-mermaid install .\docs-book
```

## Builden
```powershell
.\docs-book\build-docs.ps1
```

Dit doet:
1. Brondocumenten synchroniseren naar `docs-book/src/`
2. mdBook genereren naar `docs-book/book/`

Output:
- `docs-book/book/index.html`
- `docs-book/book/print.html`

## PDF export (gesplitst in 3 documenten)
```powershell
.\docs-book\export-pdf.ps1
```

Dit genereert standaard:
- `docs-book/book/ThetisLink-Installatie.pdf`
- `docs-book/book/ThetisLink-User-Manual.pdf`
- `docs-book/book/ThetisLink-Technische-Referentie.pdf`

Optioneel ook gecombineerde PDF erbij:
```powershell
.\docs-book\export-pdf.ps1 -IncludeCombined
```

Met eigen outputmap:
```powershell
.\docs-book\export-pdf.ps1 -OutputDir "C:\Users\chiron.vanderburgt\Desktop\ThetisLink-PDF"
```

Sneller (als je al net gebouwd hebt):
```powershell
.\docs-book\export-pdf.ps1 -SkipBuild
```
