# Desktop-Abnahme der Schreibwerkstatt

Stand 20. November 2026. Die Arbeitspakete für den lokalen Schreibablauf und die Codex-Funktionen liegen in `main`. Für den manuellen Durchlauf dient eine Kopie des synthetischen Testbuchs. [Umsetzungsplan](superpowers/plans/2026-11-20-schreibwerkstatt.md) · [Testbuch](../tests/fixtures/imperium/)

## Lokale Prüfungen

| Umgebung | Ergebnis |
| --- | --- |
| macOS 27.2.1, Apple Silicon, Rust 1.100.0, Git 2.56.1 | Formatprüfung, Clippy, 82 automatisierte Tests und nativer Release-Build bestanden. Im nativen Fenster das Testbuch geöffnet, eine Zeitkante gewählt, die Rückblende geöffnet und Umlauttext ergänzt. Die Szenendatei enthält die Bearbeitung. „Speichern“ erzeugte genau einen lokalen Commit. Das über den nativen Dialog exportierte PDF enthält die Konzeptkennzeichnung, den ergänzten Text und die Erzählreihenfolge. |
| Windows und Linux | Native Builds und Fensterdurchläufe für diesen Stand stehen aus. GitHub-Prüfungen sind zurückgestellt. |

Die automatisierten Prüfungen laufen über Cargo. `desktop_journey` verbindet Timeline, Rückblende, Dateisicherung, Git und beide Exportformate. `autosave_git` erzeugt einen Dateikonflikt mit zwei Store-Instanzen und löst ihn durch die bestätigte Zusammenführung auf. Eine abweichende Dokumentkennung wird abgewiesen und beide Fassungen bleiben erhalten. `manuscript_export` prüft den Text im erzeugten PDF sowie den Dateierhalt bei einem Exportfehler.

Für `49f8173` bestanden Formatprüfung, Clippy, alle 93 lokalen Tests und der native macOS-Release-Build. Der macOS-Linker meldete im Testprofil weiterhin eine Unwind-Warnung.

Im Release-Build von `373da59` wurde „Das Signal“ per Ziehen nach „Aufbruch“ verschoben. Die Szenendatei behielt den Zeitbezug „Erstkontakt“. Anschließend wurde auch das Kapitel „Rückblende“ per Ziehen umgeordnet. Beide Änderungen waren in den Kapiteldateien nachvollziehbar.

Bei einer externen Änderung an der geöffneten Szene zeigte die App die beiden Fassungen an. „Auf Festplatte übernehmen“ lud die externe Fassung in den Editor. `⌘S` erstellte genau einen lokalen Git-Commit mit dieser Fassung. Der Text im Git-Commit entsprach der Datei bytegenau.

## GPUI-Komponententests

Die 36 GPUI-Komponententests rendern die Produktionsansichten in GPUI-Testfenstern und simulieren Klicks, Tasten, Fokus, Ziehen, Scrollen und die Antworten der Dateidialoge. Sie laufen über Cargo ohne Betriebssystemfenster und ohne Computer Use.

- Projektwahl, Neuanlage und erstes Tab-Ziel sowie Timeline-Netz, Ereignisliste, Verkettung, Zyklusdiagnose und Kapitelwechsel per Tastatur oder Ziehen. Ereignisentwürfe bleiben nach dem Wechsel zu Kapitel oder Szene erhalten.
- Beim Scrollen des Szenenbereichs behalten Projektleiste, Navigation und „Speichern“ ihre Position. Die Komponententests prüfen die Positionen bei 1080 × 520 und 680 × 520 px.
- Szeneneditor mit 900 Zeilen Umlauttext, Rückgängig-Kürzel und Autosave. Bei Git-Indexsperre bleibt der Text gesichert. Nach Freigabe erzeugt das Speicherkürzel genau einen Commit und lässt eine fremde Datei im Index. Ein frisch aufgebautes GPUI-Fenster öffnet die gewählte Konfliktfassung erneut. Der Konflikteditor lehnt leere Zusammenführungen ab. Die simulierte IME-Komposition prüft, dass erst der bestätigte Text gespeichert wird.
- Der GPUI-Abschaltvorgang schreibt eine ausstehende Szenenänderung. Bei einem Dateikonflikt halten das Beenden-Kürzel und der App-Befehl ohne Editorfokus die Szene zur Auflösung offen. Die Öffnungsroutine erstellt nach Schließen des letzten Fensters wieder die Projektwahl und überspringt das Öffnen, solange bereits ein Fenster vorhanden ist. Die native Menü- und Dock-Auslösung steht für diese Änderungen noch aus. Bei bereits eingeleitetem Beenden durch das Betriebssystem kann GPUI den Vorgang nicht mehr abbrechen. Es versucht dann, ausstehende Änderungen zu sichern.
- Szenenübergreifende Änderung mit Rückgängig, Markdown und PDF nach einem simulierten Dialog sowie Weltkartei mit Quellensprung und persistiertem Figurenwissen.
- Codex-Ansichten mit Testdaten für Opt-out, Gespräch, Entwurf, Recherche, Detailvorschläge und Konsistenzentscheidungen. Der Wechsel von der Timeline öffnet den gewählten Gesprächskontext. Ein Paar kann in umgekehrter Reihenfolge wieder geöffnet werden. Eine laufende Anfrage speichert ihre Antwort beim ursprünglichen Paar.

Im nativen macOS-Release-Build von `405cc98` zeigte die österreichische Tastaturbelegung nach `⌥U` das unbestätigte Trema im Editor. Es blieb auch nach Ablauf des Autosave-Intervalls außerhalb der Szenendatei. Nach `E` erschien `ë` im Editor und wurde als zusammengesetztes Zeichen automatisch gespeichert. Die nativen Windows- und Linux-Durchläufe stehen aus.

## Codex

- Die Protokoll-Fixture prüft lokalen Opt-in, Anmeldung, fragmentierte Antworten, Abbruch, Prozessende und ausbleibende Übernahmen bei verspäteten Ergebnissen.
- Ein durchgehender Test verbindet die GPUI-Codex-Ansicht mit derselben lokalen Protokoll-Fixture. Er durchläuft Opt-in und simulierte Anmeldung, sendet eine Frage zum Ereignispaar und prüft Frage und Antwort nach dem Wiederöffnen des Markdown-Gesprächs. Erst eine weitere ausdrückliche Übernahme kann ein Ereignis anlegen. Der Test schaltet Codex anschließend wieder aus.
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
