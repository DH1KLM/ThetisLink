# Scripts

CI en support-scripts voor het `sdr-remote` project.

## UI coverage-gate

`ui-coverage-expected.json` is de hand-onderhouden baseline voor de
UI-controls-coverage-matrix (zie `sdr-remote-client/src/ui/controls/coverage.rs`).

Formaat: JSON-array, stable-sorted op `control × surface × channel × density`,
één entry per render-site van een control-helper:

```json
{ "control": "band_selector", "surface": "PopoutSeparate", "channel": "Rx1", "density": "Extended", "guarded": true }
```

### Gebruik (CI-gate — pending implementation)

Runtime (debug-build of `feature = "ui-coverage"`): de client dumpt bij exit
de actueel-geregistreerde sites naar `target/ui-coverage.json` via
`controls::coverage::dump_if_enabled()`.

CI vergelijkt die dump met de expected-matrix:

```bash
diff <(jq -S . target/ui-coverage.json) \
     <(jq -S . scripts/ui-coverage-expected.json)
```

Empty diff = pass. Non-empty = regressie (nieuwe unregistered control-site of
verdwenen site).

### Toevoegen aan deze folder

- `check-ui-coverage.sh` — CI-gate script (pending)
- Andere CI of devops-scripts naar eigen inzicht
