# Schreibwerkstatt

Die Schreibwerkstatt ist eine native GPUI-App zum Planen und Schreiben von Büchern. Ein Buchprojekt liegt in einem lokalen Ordner mit `project.yaml` und Markdown-Dateien. Die App hält die Erzählreihenfolge der Kapitel und Szenen unabhängig von den Zeitbeziehungen der Ereignisse.

## Starten

Mit Rust und Git auf dem Rechner startet die App aus diesem Repository mit `cargo run --release --locked`. Lege im Startfenster ein Buchprojekt in einem leeren Ordner an oder öffne eine Kopie des [synthetischen Testbuchs](tests/fixtures/imperium/).

- Der Editor sichert Änderungen automatisch in den Szenendateien. „Speichern“ legt eine lokale Git-Version an. Konzeptpassagen sind im Editor hervorgehoben und in der Markdown-Vorschau beschriftet.
- In der Timeline stellst du Ereignisse als Netz oder Liste dar und leitest daraus Szenen ab. In der Kapitelansicht platzierst du Szenen über das Kontextmenü oder per Ziehen. Beim Versuch, eine zyklische Zeitbeziehung anzulegen, zeigt ein Dialog die vollständige Ereigniskette. Mit Escape kehrst du zur Timeline zurück.
- In der Weltkartei kannst du Textstellen suchen, per Tastatur öffnen und bei mehreren Fundstellen anhand von Zeile und Spalte wählen. Für Figurenwissen legst du das Ereignis fest, ab dem die Figur die Aussage kennt oder glaubt.
- Das Manuskript lässt sich in Erzählreihenfolge als Markdown oder PDF exportieren. Beide Formate beschriften Konzeptpassagen. Schreiben, Planung und Export funktionieren ohne Codex. Die KI-Funktionen benötigen die Codex-CLI, eine Anmeldung und die Freigabe für das jeweilige Buchprojekt.
- In der Konsistenzansicht kannst du Widersprüche mit beiden Quellen prüfen und beabsichtigte Abweichungen mit einer Begründung festhalten. Ändert sich die Aussage, kann eine erneute Prüfung den Befund wieder anzeigen.

## Lokal prüfen

```text
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

Die [Desktop-Abnahme](docs/abnahme-desktop.md) dokumentiert die GPUI-Komponententests, die nativen Plattformprüfungen und die noch offenen Durchläufe.
