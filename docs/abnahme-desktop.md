# Desktop-Abnahme der Schreibwerkstatt

Stand 20. November 2026. Die Arbeitspakete für den lokalen Schreibablauf und die Codex-Funktionen liegen in `main`. Für den manuellen Durchlauf dient eine Kopie des synthetischen Testbuchs. [Umsetzungsplan](superpowers/plans/2026-11-20-schreibwerkstatt.md) · [Testbuch](../tests/fixtures/imperium/)

## Lokale Prüfungen

| Umgebung | Ergebnis |
| --- | --- |
| macOS 27.2.1, Apple Silicon, Rust 1.100.0, Git 2.56.1 | Formatprüfung, Clippy, 82 automatisierte Tests und nativer Release-Build bestanden. Im nativen Fenster das Testbuch geöffnet, eine Zeitkante gewählt, die Rückblende geöffnet und Umlauttext ergänzt. Die Szenendatei enthält die Bearbeitung. „Speichern“ erzeugte genau einen lokalen Commit. Das über den nativen Dialog exportierte PDF enthält die Konzeptkennzeichnung, den ergänzten Text und die Erzählreihenfolge. |
| Windows und Linux | Native Builds und Fensterdurchläufe für diesen Stand stehen aus. GitHub-Prüfungen sind zurückgestellt. |

Die automatisierten Prüfungen laufen über Cargo. `desktop_journey` verbindet Timeline, Rückblende, Dateisicherung, Git und beide Exportformate. `autosave_git` erzeugt einen Dateikonflikt mit zwei Store-Instanzen und löst ihn durch die bestätigte Zusammenführung auf. Eine abweichende Dokumentkennung wird abgewiesen und beide Fassungen bleiben erhalten. `manuscript_export` prüft den Text im erzeugten PDF sowie den Dateierhalt bei einem Exportfehler.

Für `22c27be` bestanden Formatprüfung, Clippy, alle 119 lokalen Tests und der native macOS-Release-Build. Der macOS-Linker meldete im Testprofil weiterhin eine Unwind-Warnung.

Im Release-Build von `373da59` wurde „Das Signal“ per Ziehen nach „Aufbruch“ verschoben. Die Szenendatei behielt den Zeitbezug „Erstkontakt“. Anschließend wurde auch das Kapitel „Rückblende“ per Ziehen umgeordnet. Beide Änderungen waren in den Kapiteldateien nachvollziehbar.

Bei einer externen Änderung an der geöffneten Szene zeigte die App die beiden Fassungen an. „Auf Festplatte übernehmen“ lud die externe Fassung in den Editor. `⌘S` erstellte genau einen lokalen Git-Commit mit dieser Fassung. Der Text im Git-Commit entsprach der Datei bytegenau.

## GPUI-Komponententests

Die 61 GPUI-Komponententests rendern die Produktionsansichten in GPUI-Testfenstern und simulieren Klicks, Tasten, Fokus, Ziehen, Scrollen und die Antworten der Dateidialoge. Ein zusätzlicher GPUI-Test prüft die helle und dunkle Anwendungspalette. Sie laufen über Cargo ohne Betriebssystemfenster und ohne Computer Use.

- Projektwahl, Neuanlage und erstes Tab-Ziel sowie Timeline-Netz, Ereignisliste, Verkettung, Zyklusdiagnose und Kapitelwechsel per Tastatur oder Ziehen. Ereignisentwürfe bleiben nach dem Wechsel zu Kapitel oder Szene erhalten.
- Die Kürzel für „Buchprojekt öffnen“ und „Neues Buchprojekt“ starten Ordnerdialog und Anlageformular. Vor einem Buchwechsel wird der Text gesichert. Bei einem Konflikt bleiben beide Fassungen zur Auflösung offen. Ein Abbruch der Neuanlage kehrt mit dem bisherigen Fokus zur Szene zurück. Die native Auslösung der Befehle über das macOS-Menü „Ablage“ ist noch offen.
- Beim Scrollen des Szenenbereichs behalten Projektleiste, Navigation und „Speichern“ ihre Position. Die Komponententests prüfen die Positionen bei 1080 × 520 und 680 × 520 px.
- Ab 1120 px Fensterbreite erscheinen Markdown-Vorschau oder Szenendetails im verstellbaren Inspektor rechts neben dem Editor. Darunter wechseln sie die Ansicht. Der GPUI-Test bei 740 px prüft, dass beim Schließen der Vorschau der Fokus zur Vorschau-Schaltfläche zurückkehrt und der Editor seine Textauswahl behält.
- Beim Ersetzen von „Tür“ durch Text mit kombinierendem Trema folgt die Markierung im Editor dem Konzeptbereich und behält dessen ID. Das Markdown für die Vorschau beschriftet die Konzeptpassage. Die benachbarte Liste bleibt unmarkiert. Nach Rückgängig öffnet ein neues GPUI-Fenster die ursprüngliche Fassung mit ihrer Markierung.
- Szeneneditor mit 900 Zeilen Umlauttext, Rückgängig-Kürzel und Autosave. Bei Git-Indexsperre bleibt der Text gesichert. Nach Freigabe erzeugt das Speicherkürzel genau einen Commit und lässt eine fremde Datei im Index. Ein frisch aufgebautes GPUI-Fenster öffnet die gewählte Konfliktfassung erneut. Der Konflikteditor lehnt leere Zusammenführungen ab. Die simulierte IME-Komposition prüft, dass erst der bestätigte Text gespeichert wird.
- Der GPUI-Abschaltvorgang schreibt eine ausstehende Szenenänderung. Bei einem Dateikonflikt halten das Beenden-Kürzel und der App-Befehl ohne Editorfokus die Szene zur Auflösung offen. Die Öffnungsroutine erstellt nach Schließen des letzten Fensters wieder die Projektwahl und überspringt das Öffnen, solange bereits ein Fenster vorhanden ist. Die native Menü- und Dock-Auslösung steht für diese Änderungen noch aus. Bei bereits eingeleitetem Beenden durch das Betriebssystem kann GPUI den Vorgang nicht mehr abbrechen. Es versucht dann, ausstehende Änderungen zu sichern.
- Szenenübergreifende Änderung mit Rückgängig, Markdown und PDF nach einem simulierten Dialog sowie Weltkartei mit Quellensprung und persistiertem Figurenwissen.
- Codex-Ansichten mit Testdaten für Opt-out, Gespräch, Entwurf, Recherche, Detailvorschläge und Konsistenzentscheidungen. Der Wechsel von der Timeline öffnet den gewählten Gesprächskontext. Ein Paar kann in umgekehrter Reihenfolge wieder geöffnet werden. Eine laufende Anfrage speichert ihre Antwort beim ursprünglichen Paar.

Im nativen macOS-Release-Build von `405cc98` zeigte die österreichische Tastaturbelegung nach `⌥U` das unbestätigte Trema im Editor. Es blieb auch nach Ablauf des Autosave-Intervalls außerhalb der Szenendatei. Nach `E` erschien `ë` im Editor und wurde als zusammengesetztes Zeichen automatisch gespeichert. Die nativen Windows- und Linux-Durchläufe stehen aus.

## Desktop-Dokumentfenster

Der [Desktop-UX-Entwurf](superpowers/specs/2026-11-20-schreibwerkstatt-ux-design.md) ist in den nativen GPUI-Ansichten umgesetzt. Die Komponententests prüfen unter anderem:

- Einklappbare Quellliste, feste Statusleiste, getrennte Anzeigen für Dateisicherung und Git-Stand sowie Wiederöffnen eines Buchs in der letzten Szene mit der letzten lokalen Version.
- Timeline-Netz und Ereignisliste mit verstellbarem Inspektor, Szenenableitung, Kapitelaufklappung, Kontextmenü und Ziehen. Erzählposition und Zeitbezug bleiben getrennt.
- Szenenansicht mit verstellbarer Vorschau und Gesamtmanuskript mit Inhaltsverzeichnis, szenenübergreifender Bearbeitung und Exportformat-Auswahl. Die Exporttests prüfen die geschriebenen PDF- und Markdown-Dateien.
- Durchsuchbare Weltkartei. Ein Quellensprung verwendet den aktuellen Dateistand. Bei doppelten Zitaten stehen die aktuellen Fundstellen mit Zeile und Spalte zur Auswahl.
- Gespräch zum Ereignispaar, Konsistenzbefunde mit Quellensprüngen und Projekteinstellungen mit Rückkehr zur bisherigen Ansicht. Die KI-Anmeldung und Antworten laufen hier über die lokale Protokoll-Fixture.

Der native macOS-Release-Build von `a804324` öffnete die synthetische Kopie des Buchs über ⌘O und den Systemordnerdialog. Der Fenstertitel wechselte zwischen Timeline, Kapitel und Schreiben. Die Timeline zeigte die Quellliste, das Netz und das zeitlich offene Ereignis. Im schmalen Dokumentfenster zeigte die Vorschau „Eine Tür öffnet sich“ als beschriftetes Konzept neben gewöhnlicher Prosa. „Ablage → Speichern“ erzeugte genau den ersten Git-Commit des Testbuchs. Der dokumentweite Ablauf in den übrigen Ansichten, die nativen Windows- und Linux-Durchläufe und Codex mit persönlicher Anmeldung bleiben offen.

Die Sichtprüfung zeigte noch die neutrale Standardpalette. Die danach eingebundene helle und dunkle Schreibwerkstatt-Palette wird über den GPUI-Theme-Wechsel auf Schieferblau, Quelllistenfarbe und Textfarbe geprüft. Die visuelle Prüfung der neuen Palette, einschließlich der Abgrenzung von Konzepten in der formatierten Vorschau, steht aus.

## Codex

- Die Protokoll-Fixture prüft lokalen Opt-in, Anmeldung, fragmentierte Antworten, Abbruch, Prozessende und ausbleibende Übernahmen bei verspäteten Ergebnissen.
- Der GPUI-Test für Detailvorschläge prüft, dass die Schreibpause ohne Anfrage bleibt und erst beim Szenenwechsel der bearbeitete Text an den lokalen Testserver geht. Die Antwort trifft in der zweiten Szene ein und wird der Ursprungsszene zugeordnet. Verwerfen und Übernehmen ändern die Weltkartei erst bei Bestätigung. Die gespeicherte Zielnotiz enthält ausschließlich die bestätigte, bearbeitete Ergänzung.
- Die GPUI-Tests für Szenenentwürfe prüfen die Teilübernahme aus einem Entwurf für zwei Konzeptstichpunkte. Benachbarte Prosa und der Konzeptstatus bleiben erhalten. Ohne Bestätigung wird keine Weltkarte angelegt. Bei einer verspäteten Antwort auf eine inzwischen doppelte Zielpassage übernimmt die App den Ausschnitt erst nach einer neuen Textauswahl.
- Ein durchgehender GPUI-Test verbindet die Codex-Ansicht mit derselben lokalen Protokoll-Fixture. Er durchläuft Opt-in, simulierte Anmeldung und eine Frage zum Ereignispaar. Danach markiert er einen Teil der Antwort als Idee, schaltet Codex aus und öffnet das Buch mit umgekehrter Ereignisauswahl. Gespräch und Markierung bleiben erhalten. Der Abbruch lässt Ereignisse und Beziehungen unverändert. Nach zweimaliger Bestätigung liegen genau ein neues Ereignis und die gewählte Beziehung vor.
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
