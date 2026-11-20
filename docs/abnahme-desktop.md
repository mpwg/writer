# Desktop-Abnahme der Schreibwerkstatt

Stand 20. November 2026. Die Arbeitspakete für den lokalen Schreibablauf und die Codex-Funktionen liegen in `main`. Für den manuellen Durchlauf dient eine Kopie des synthetischen Testbuchs. [Umsetzungsplan](superpowers/plans/2026-11-20-schreibwerkstatt.md) · [Testbuch](../tests/fixtures/imperium/)

## Lokale Prüfungen

| Umgebung | Ergebnis |
| --- | --- |
| macOS 27.2.1, Apple Silicon, Rust 1.100.0, Git 2.56.1 | Formatprüfung, Clippy, 82 automatisierte Tests und nativer Release-Build bestanden. Im nativen Fenster das Testbuch geöffnet, eine Zeitkante gewählt, die Rückblende geöffnet und Umlauttext ergänzt. Die Szenendatei enthält die Bearbeitung. „Speichern“ erzeugte genau einen lokalen Commit. Das über den nativen Dialog exportierte PDF enthält die Konzeptkennzeichnung, den ergänzten Text und die Erzählreihenfolge. |
| Windows | Native Build- und Fensterabnahme offen. Im aktuellen Arbeitsumfeld steht kein Windows-Testsystem bereit. |
| Linux | Native Build- und Fensterabnahme offen. Im aktuellen Arbeitsumfeld steht kein Linux-Testsystem bereit. |

Die automatisierten Prüfungen laufen über Cargo. `desktop_journey` verbindet Timeline, Rückblende, Dateisicherung, Git und beide Exportformate. In einer separaten Kopie des Repositorys kehrte ein absichtlich geänderter Markdown-Export die Kapitelreihenfolge um. Der Test scheiterte am Vergleich der beiden Szenenpositionen und bestand anschließend im unveränderten Repository. `autosave_git` erzeugt einen Dateikonflikt mit zwei Store-Instanzen und löst ihn durch die bestätigte Zusammenführung auf. Eine abweichende Dokumentkennung wird abgewiesen und beide Fassungen bleiben erhalten. `manuscript_export` prüft den Text im erzeugten PDF sowie den Dateierhalt bei einem Exportfehler.

Für `3babdcd` bestanden Formatprüfung, Clippy, alle 124 lokalen Tests und der native macOS-Release-Build. Cargo lief mit `--offline --locked`. Der macOS-Linker meldete im Testprofil weiterhin eine Unwind-Warnung.

Im Release-Build von `373da59` wurde „Das Signal“ per Ziehen nach „Aufbruch“ verschoben. Die Szenendatei behielt den Zeitbezug „Erstkontakt“. Anschließend wurde auch das Kapitel „Rückblende“ per Ziehen umgeordnet. Beide Änderungen waren in den Kapiteldateien nachvollziehbar.

Bei einer externen Änderung an der geöffneten Szene zeigte die App die beiden Fassungen an. „Auf Festplatte übernehmen“ lud die externe Fassung in den Editor. `⌘S` erstellte genau einen lokalen Git-Commit mit dieser Fassung. Der Text im Git-Commit entsprach der Datei bytegenau.

## GPUI-Komponententests

Die 66 GPUI-Komponententests rendern die Produktionsansichten in GPUI-Testfenstern und simulieren Klicks, Tasten, Fokus, Ziehen, Scrollen und die Antworten der Dateidialoge. Ein zusätzlicher GPUI-Test prüft die helle und dunkle Anwendungspalette. Sie laufen über Cargo ohne Betriebssystemfenster und ohne Computer Use.

- Projektwahl, Neuanlage und erstes Tab-Ziel sowie Timeline-Netz, Ereignisliste, Verkettung, Zyklusdiagnose und Kapitelwechsel per Tastatur oder Ziehen. Ereignisentwürfe bleiben nach dem Wechsel zu Kapitel oder Szene erhalten.
- Die Auswahl „Aufbruch“ bleibt beim Wechsel vom Netz zur Liste erhalten. Das vierte, zeitlich offene Ereignis ist per Tab und Enter erreichbar. Der Versuch einer zyklischen Verbindung öffnet den Dialog mit dem vollständigen Pfad „Aufbruch → Erstkontakt → Fristablauf → Aufbruch“. Escape gibt den Fokus an „Liegt vor“ zurück. Auf der Festplatte bleiben die beiden gültigen Kanten erhalten.
- Die Kürzel für „Buchprojekt öffnen“ und „Neues Buchprojekt“ starten Ordnerdialog und Anlageformular. Vor einem Buchwechsel wird der Text gesichert. Bei einem Konflikt bleiben beide Fassungen zur Auflösung offen. Ein Abbruch der Neuanlage kehrt mit dem bisherigen Fokus zur Szene zurück. Im nativen macOS-Release-Build vor `3babdcd` wurden beide Befehle über „Ablage“ ausgelöst. Ein Abbruch kehrte jeweils zur offenen Szenenvorschau zurück.
- Beim Scrollen des Szenenbereichs behalten Projektleiste, Navigation und „Speichern“ ihre Position. Die Komponententests prüfen die Positionen bei 1080 × 520 und 680 × 520 px.
- Ab 1120 px Fensterbreite erscheinen Markdown-Vorschau oder Szenendetails im verstellbaren Inspektor rechts neben dem Editor. Darunter wechseln sie die Ansicht. Der GPUI-Test bei 740 px prüft, dass beim Schließen der Vorschau der Fokus zur Vorschau-Schaltfläche zurückkehrt und der Editor seine Textauswahl behält.
- Der Konzepttest ersetzt „Tür“ durch „Luke“ und in einem zweiten Buch „Tu“ mit kombinierendem Trema durch „Tü“. Die Auswahl im zweiten Lauf endet direkt nach dem kombinierenden Zeichen. Der Editor behält die Konzept-ID, die gerenderte Vorschau zeigt „Konzept“, und die benachbarte Liste bleibt unmarkiert. Nach Rückgängig öffnet ein neues GPUI-Fenster die ursprüngliche Fassung mit ihrer Markierung und Vorschau.
- Nach sofortigem Szenenwechsel und Wiederöffnen in einem frischen GPUI-Testfenster steht die Bearbeitung im Editor und in der Datei. Git HEAD enthält bis zum Doppelklick auf „Speichern“ die vorige Fassung. Der Doppelklick erzeugt genau einen Commit. Der Sperrtest vergleicht sowohl den vorgemerkten Inhalt einer fremden Datei als auch ihre abweichende Arbeitskopie.
- Szeneneditor mit 900 Zeilen Umlauttext, Rückgängig-Kürzel und Autosave. Bei Git-Indexsperre bleibt der Text gesichert. Nach Freigabe erzeugt das Speicherkürzel genau einen Commit und lässt eine fremde Datei im Index. Ein frisch aufgebautes GPUI-Fenster öffnet die gewählte Konfliktfassung erneut. Der Konflikteditor lehnt leere Zusammenführungen ab. Die simulierte IME-Komposition prüft, dass erst der bestätigte Text gespeichert wird.
- Der GPUI-Abschaltvorgang schreibt eine ausstehende Szenenänderung. Bei einem Dateikonflikt halten das Beenden-Kürzel und der App-Befehl ohne Editorfokus die Szene zur Auflösung offen. Die Öffnungsroutine erstellt nach Schließen des letzten Fensters wieder die Projektwahl und überspringt das Öffnen, solange bereits ein Fenster vorhanden ist. Die native Menü- und Dock-Auslösung steht für diese Änderungen noch aus. Bei bereits eingeleitetem Beenden durch das Betriebssystem kann GPUI den Vorgang nicht mehr abbrechen. Es versucht dann, ausstehende Änderungen zu sichern.
- Der native Test zeigte, dass `⌘W` im bisherigen Release-Build nichts auslöste. Seit `3babdcd` ist „Fenster schließen“ in „Ablage“ mit `⌘W` verknüpft. Der GPUI-Test prüft, dass ein Dateikonflikt das Fenster offen hält und ein erneutes `⌘W` nach der Auflösung ausstehenden Szenentext vor dem Schließen sichert. Die native Wiederholung am neuen Build steht aus.
- Szenenübergreifende Änderung mit Rückgängig, Markdown und PDF nach einem simulierten Dialog sowie Weltkartei mit Quellensprung und persistiertem Figurenwissen.
- Die aus „Erstkontakt“ abgeleitete Szene „Das Signal“ lässt sich ohne Kapitel öffnen und per Kontextmenü in Kapitel 3 verschieben. Nach dem Öffnen über den simulierten Ordnerdialog in einem frischen GPUI-Fenster zeigen die beiden Felder in der Kapitelansicht die Position und das ursprüngliche Ereignis. Ziehen aus „Szenen ohne Kapitel“ stellt dieselbe gespeicherte Position her.
- Codex-Ansichten mit Testdaten für Opt-out, Gespräch, Entwurf, Recherche, Detailvorschläge und Konsistenzentscheidungen. Der Wechsel von der Timeline öffnet den gewählten Gesprächskontext. Ein Paar kann in umgekehrter Reihenfolge wieder geöffnet werden. Eine laufende Anfrage speichert ihre Antwort beim ursprünglichen Paar.

Im nativen macOS-Release-Build von `405cc98` zeigte die österreichische Tastaturbelegung nach `⌥U` das unbestätigte Trema im Editor. Es blieb auch nach Ablauf des Autosave-Intervalls außerhalb der Szenendatei. Nach `E` erschien `ë` im Editor und wurde als zusammengesetztes Zeichen automatisch gespeichert. Die nativen Windows- und Linux-Durchläufe stehen aus.

Der GPUI-Manuskripttest liest beide Szenendateien nach einer gemeinsamen Ersetzung und nach einem einzigen Rückgängig. Der Exporttest prüft Kapitelreihenfolge, Umlaute und Konzeptbeschriftung in Markdown und PDF. Er zählt 100 Absätze der Rückblende und prüft das Schlusswort auf der letzten Seite. Unter macOS wurden die drei PDF-Seiten mit `pdftoppm` gerendert und auf Lesbarkeit, Umbruch und sichtbare Seitenzahlen geprüft. Beim provozierten Exportfehler blieb die zuvor erzeugte PDF-Datei bytegleich.

## Desktop-Dokumentfenster

Der [Desktop-UX-Entwurf](superpowers/specs/2026-11-20-schreibwerkstatt-ux-design.md) ist in den nativen GPUI-Ansichten umgesetzt. Die Komponententests prüfen unter anderem:

- Einklappbare Quellliste, feste Statusleiste, getrennte Anzeigen für Dateisicherung und Git-Stand sowie Wiederöffnen eines Buchs in der letzten Szene mit der letzten lokalen Version.
- Timeline-Netz und Ereignisliste mit verstellbarem Inspektor, Szenenableitung, Kapitelaufklappung, Kontextmenü und Ziehen. Erzählposition und Zeitbezug bleiben getrennt.
- Szenenansicht mit verstellbarer Vorschau und Gesamtmanuskript mit Inhaltsverzeichnis, szenenübergreifender Bearbeitung und Exportformat-Auswahl. Die Exporttests prüfen die geschriebenen PDF- und Markdown-Dateien.
- Durchsuchbare Weltkartei. Ein Quellensprung verwendet den aktuellen Dateistand. Bei doppelten Zitaten stehen die aktuellen Fundstellen mit Zeile und Spalte zur Auswahl.
- Nach der Quellensuche führen Tab und Enter zur extern verschobenen Passage und setzen den Editorfokus. Bei identischem Zitat samt Kontext bietet der Dialog beide Fundstellen an. Ein Rust-Integrationstest belegt anhand gespeicherter Szenen und Ereignisse, dass dieselbe Erkenntnis in der früheren Szene als Autorenwissen vorliegt und der Figur erst ab dem späteren Ereignis zugeordnet ist.
- Gespräch zum Ereignispaar, Konsistenzbefunde mit Quellensprüngen und Projekteinstellungen mit Rückkehr zur bisherigen Ansicht. Die KI-Anmeldung und Antworten laufen hier über die lokale Protokoll-Fixture.

Der native macOS-Release-Build von `a804324` öffnete die synthetische Kopie des Buchs über ⌘O und den Systemordnerdialog. Der Fenstertitel wechselte zwischen Timeline, Kapitel und Schreiben. Die Timeline zeigte die Quellliste, das Netz und das zeitlich offene Ereignis. Im schmalen Dokumentfenster zeigte die Vorschau „Eine Tür öffnet sich“ als beschriftetes Konzept neben gewöhnlicher Prosa. „Ablage → Speichern“ erzeugte genau den ersten Git-Commit des Testbuchs. Der dokumentweite Ablauf in den übrigen Ansichten, die nativen Windows- und Linux-Durchläufe und Codex mit persönlicher Anmeldung bleiben offen.

Im nativen macOS-Release-Build von `ad8013d` wurde das Testbuch in einem eigenen Profil über den Systemordnerdialog geöffnet. Nach einer externen Dateiänderung und einer davon abweichenden Eingabe im Editor zeigte der Konfliktbereich beide Fassungen. „Auf Festplatte übernehmen“ übernahm den externen Absatz. `⌘S` erzeugte genau einen Commit, dessen Szenendatei bytegenau dem Arbeitsstand entsprach. Nach `⌘Q` und einem neuen App-Prozess öffnete der Eintrag in „Zuletzt geöffnet“ wieder „Das Signal“ mit der gewählten Fassung. In der hellen Palette stand die Konzeptpassage als beschrifteter Block neben der normalen Prosa. Die dunkle Palette ist über den GPUI-Theme-Wechsel geprüft, ihre native Sichtprüfung steht aus.

## Codex

- Die Protokoll-Fixture prüft lokalen Opt-in, Anmeldung, fragmentierte Antworten, Abbruch, Prozessende und ausbleibende Übernahmen bei verspäteten Ergebnissen.
- Ein GPUI-Durchlauf aktiviert KI für das Testbuch mit einem bewusst fehlenden Pfad zur Codex-Binärdatei. Bei aktiver Freigabe meldet die App die fehlende CLI und sperrt Modellanfragen. Der Editor speichert die Änderung, und beide Exporte enthalten den geänderten Umlauttext und die Konzeptbeschriftung. Anschließend lässt sich die Freigabe widerrufen.
- Ein GPUI-Test stellt die Recherchefrage aus dem Szeneneditor. Der an die lokale Protokoll-Fixture übergebene Kontext unterscheidet Prosa und Planung, enthält die zeitliche Beziehung der Rückblende zur geplanten Begegnung und lässt die Reihenfolge zweier Treffen offen. Jeder der vier Belege aus der simulierten Antwort öffnet die richtige Szene und markiert die zitierte Passage.
- Zwei GPUI-Tests führen die Konsistenzprüfung mit der lokalen Protokoll-Fixture durch. Eine begründete Entscheidung gilt nach Verschieben des Absatzes und erneuter Prüfung weiter. Eine veränderte Jahreszahl und eine Negation erscheinen jeweils wieder als offener Befund. Beide Quellensprünge führen zur aktuellen Szene beziehungsweise zur Dauerangabe der ursprünglichen Timeline-Kante.
- Der GPUI-Test für Detailvorschläge prüft, dass die Schreibpause ohne Anfrage bleibt und erst beim Szenenwechsel der bearbeitete Text an den lokalen Testserver geht. Die Antwort trifft in der zweiten Szene ein und wird der Ursprungsszene zugeordnet. Verwerfen und Übernehmen ändern die Weltkartei erst bei Bestätigung. Die gespeicherte Zielnotiz enthält ausschließlich die bestätigte, bearbeitete Ergänzung.
- Die GPUI-Tests für Szenenentwürfe prüfen die Teilübernahme aus einem Entwurf für zwei Konzeptstichpunkte. Benachbarte Prosa und der Konzeptstatus bleiben erhalten. Ohne Bestätigung wird keine Weltkarte angelegt. Bei einer verspäteten Antwort auf eine inzwischen doppelte Zielpassage übernimmt die App den Ausschnitt erst nach einer neuen Textauswahl.
- Ein durchgehender GPUI-Test verbindet die Codex-Ansicht mit derselben lokalen Protokoll-Fixture. Er durchläuft Opt-in, simulierte Anmeldung und eine Frage zum Ereignispaar. Danach markiert er einen Teil der Antwort als Idee, schaltet Codex aus und öffnet das Buch mit umgekehrter Ereignisauswahl. Gespräch und Markierung bleiben erhalten. Der Abbruch lässt Ereignisse und Beziehungen unverändert. Nach zweimaliger Bestätigung liegen genau ein neues Ereignis und die gewählte Beziehung vor.
- Im nativen macOS-Release-Build wurde der Testserver aus `tests/support/app_server.rs` als lokales `codex`-Programm im `PATH` eines isolierten Testprofils gestartet. Die App lief mit gesperrtem Netzwerk. Nach der simulierten Anmeldung lieferte der Server „Die Küste.“ auf die Frage zum Paar Erstkontakt/Aufbruch. Die App speicherte Beitrag, Antwort und die markierte Idee in einer gemeinsamen Gesprächsdatei. Nach Opt-out und vollständigem Neustart erschienen dieselben Beiträge und „Noch offen · Die Küste.“ für Aufbruch/Erstkontakt. Die Sendeaktion war gesperrt und im Buch lagen weiterhin genau drei Ereignisse. Dieser Lauf prüft den lokalen Protokollweg ohne echte Kontoanmeldung.
- Die installierte Codex-CLI erreichte mit synthetischem Buch im nativen Fenster den Browser-Anmeldeschritt. Anmeldung abgebrochen, KI ausgeschaltet und den Aus-Zustand geprüft.
- Nativer Offline-Durchlauf auf macOS mit Release-Build, eigenem Profil und Kopie des Testbuchs. Für diesen Prozess sperrte ein beim Start geladener Sandbox-Filter das Netzwerk und den Zugriff auf die im ChatGPT-Bundle enthaltene Codex-CLI. Der Prozess-`PATH` enthielt nur `/usr/bin` und `/bin`. Ohne manuell gewählten Codex-Pfad meldete die App „Die Codex-CLI wurde nicht gefunden“, und die Modellaktionen waren gesperrt. Über den Systemordnerdialog, den Szeneneditor, `⌘S` und beide systemeigenen Exportdialoge wurden die Änderung „Die Fähre überquert den Fluss.“, eine lokale Git-Version, Markdown und PDF erzeugt. Dateitext und Git-Version waren bytegleich. Im Markdown und im mit PDFKit extrahierten PDF-Text stimmten Erzählreihenfolge, Umlauttext und Konzeptbeschriftung. Die Codex-Installation des Hosts blieb bestehen.
- Die persönliche ChatGPT-Anmeldung, echte Modellantworten und der anschließende vollständige Codex-Durchlauf stehen aus. Die Übernahme von Ideen, Entwürfen, Detailvorschlägen und Entscheidungen hat lokale Fachtests.

## Prüfkommandos

```text
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
cargo run --release --locked
```
