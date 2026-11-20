# Desktop-Abnahme der Schreibwerkstatt

Stand 20. November 2026. Die Arbeitspakete für den lokalen Schreibablauf und die Codex-Funktionen liegen in `main`. Für den manuellen Durchlauf dient eine Kopie des synthetischen Testbuchs. [Umsetzungsplan](superpowers/plans/2026-11-20-schreibwerkstatt.md) · [Testbuch](../tests/fixtures/imperium/)

## Lokale Prüfungen

| Umgebung | Ergebnis |
| --- | --- |
| macOS 27.2.1, Apple Silicon, Rust 1.100.0, Git 2.56.1 | Formatprüfung, Clippy, 55 automatisierte Tests und nativer Release-Build bestanden. Im nativen Fenster das Testbuch geöffnet, eine Zeitkante gewählt, die Rückblende geöffnet und Umlauttext ergänzt. Die Szenendatei enthält die Bearbeitung. „Speichern“ erzeugte genau einen lokalen Commit. Das über den nativen Dialog exportierte PDF enthält die Konzeptkennzeichnung, den ergänzten Text und die Erzählreihenfolge. |
| Windows und Linux | Native Builds und Fensterdurchläufe für diesen Stand stehen aus. GitHub-Prüfungen sind zurückgestellt. |

Die automatisierten Prüfungen laufen über Cargo. `desktop_journey` verbindet Timeline, Rückblende, Dateisicherung, Git und beide Exportformate. Der Dateikonflikt wird mit zwei getrennten Store-Instanzen erzeugt und durch die bestätigte Zusammenführung aufgelöst. `manuscript_export` prüft den Text im erzeugten PDF sowie den Dateierhalt bei einem Exportfehler. Der macOS-Linker meldet im Testprofil eine Unwind-Warnung. Tests und Release-Build wurden erfolgreich beendet.

Noch offen im praktischen macOS-Durchlauf sind der komplette Drag-and-drop-Ablauf, Tastaturbedienung und Eingabemethoden über alle Ansichten sowie die Konfliktauflösung im Fenster. Die zugehörigen Fachpfade sind automatisiert geprüft.

## Codex

- Die Protokoll-Fixture prüft lokalen Opt-in, Anmeldung, fragmentierte Antworten, Abbruch, Prozessende und ausbleibende Übernahmen bei verspäteten Ergebnissen.
- Die installierte Codex-CLI erreichte mit synthetischem Buch im nativen Fenster den Browser-Anmeldeschritt. Anmeldung abgebrochen, KI ausgeschaltet und den Aus-Zustand geprüft.
- Die persönliche ChatGPT-Anmeldung, echte Modellantworten und der anschließende vollständige Codex-Durchlauf stehen aus. Die Übernahme von Ideen, Entwürfen, Detailvorschlägen und Entscheidungen hat lokale Fachtests.

## Prüfkommandos

```text
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
cargo run --release --locked
```
