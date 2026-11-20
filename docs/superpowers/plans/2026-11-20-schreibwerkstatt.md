# Umsetzungsplan für die Rust-Schreibwerkstatt

> Für die Umsetzung stehen bei sinnvoll abgrenzbaren Paketen delegate-coding-efficiently und für begleitende Dokumentarbeit delegate-work-efficiently zur Verfügung. Bei ausdrücklich gewünschter Inline-Umsetzung gilt executing-plans. Die Kontrollkästchen halten den Fortschritt fest.

**Ziel** ist eine lokale Desktop-App, in der aus einem Ereignisnetz ein überarbeitbares und exportierbares Manuskript entsteht. Optional begleitet Codex den gesamten Schreibprozess.

**Architektur.** Ein Rust-Paket mit Bibliothekskern und GPUI-Oberfläche hält alle fachlichen Zustände im Buchordner. Die Oberfläche führt Benutzeraktionen über denselben Kern aus wie die Integrationstests. Ein separater Adapter verwaltet den lokalen Codex-Kindprozess. Die App besitzt die Manuskriptänderungen und übernimmt Vorschläge nur nach Auswahl.

**Technik.** Rust, GPUI, GPUI Kit, Markdown, lokales Git und Codex App Server mit ChatGPT-Abo. Als technische Entwurfsentscheidungen sieht der Plan YAML-Metadaten, persistente UUIDs, einen flüchtigen Quellenindex und Typst als PDF-Satzsystem vor. Diese Entscheidungen können bei nachgewiesenen Integrationsproblemen ersetzt werden, solange das Projektformat lesbar und der Export portabel bleibt.

**Grundlage.** [Freigegebener Entwurf](../specs/2026-11-20-schreibprogramm-design.md). Ausgangslage ist ein Repository mit Entwurfsdokument und ohne Anwendungscode.

## Gemeinsame Anforderungen

- macOS, Windows und Linux sind gleichwertige Ziele der ersten Version.
- Das vollständige Buch besteht aus lesbaren Text- und Markdown-Dateien. Projektwechsel und externe Bearbeitung sind normale Arbeitsabläufe.
- Automatisches Sichern schreibt Dateien. „Speichern“ erstellt nach dem Schreiben einen lokalen Git-Commit.
- „Konzept“ bleibt auf der betroffenen Passage bis zur ausdrücklichen Entfernung erhalten.
- KI ist pro Projekt optional und standardmäßig aus. Nur die bestätigte Übernahme verändert die geplante Handlung, Karteien oder den Manuskripttext.
- Der einzige KI-Zugang der ersten Version ist der lokale Codex App Server mit persönlicher ChatGPT-Anmeldung. Manuskript, Planung und Export müssen auch ohne installierten Codex funktionieren.

## Risiken für die Abnahme

- Externe Änderungen während des Schreibens sowie ein gescheiterter Commit dürfen keine Textfassung kosten (Pakete 1–2).
- Ungeordnete Ereignisse und Rückblenden dürfen keine falsche Auskunft über „zuletzt“ erzeugen (3 und 10).
- Umlaute, Unicode-Auswahl und Konzeptgrenzen müssen Teilersetzungen und Export überstehen (5–6 und 11).
- Späte KI-Antworten dürfen nach Szenenwechsel, Überarbeitung oder Opt-out keine veralteten Inhalte eintragen (8, 11 und 12).
- Ein verschobener Absatz muss seine Quellstelle und einen beabsichtigten Widerspruch behalten, eine tatsächlich geänderte Aussage wieder prüfbar werden (7 und 13).

## Dateistruktur, Schnittstellen und Etappen

Alle Modulpfade unten sind relativ zu `src/`; mit `tests/` und `docs/` beginnende Pfade liegen im Repository-Stamm. Paket 1 legt außerdem `Cargo.toml`, `Cargo.lock`, `src/main.rs` und `src/lib.rs` an. Paket 1 definiert gemeinsame Typen `Id` (UUID) und `Result<T>` (Fehler mit Quellpfad und Ursache). Weitere Fachtypen entstehen im zuerst genannten Modul ihres Pakets. Tests liegen je Paket in der angegebenen Datei; kurze Assertions beschreiben das Verhalten anhand lokaler Testhelfer. Testdaten bleiben synthetisch.

| Etappe | Pakete | Nutzbares Ergebnis |
|---|---|---|
| Planen | 1–4 | Ohne KI ein Buch anlegen, Ereignisse verbinden, Szenen und Kapitel anordnen und versionieren |
| Schreiben | 5–7 | Buch einschließlich Figurenwissen bearbeiten, durchlesen und exportieren |
| Mit Codex arbeiten | 8–10 | Anmelden, Ideen am Ereignispaar entwickeln und Quellen im Manuskript finden |
| Überarbeiten und abnehmen | 11–14 | Entwürfe übernehmen, Details und Konsistenz prüfen und die drei Desktopfassungen abnehmen |

Paket 8 kann nach dem Projektformat unabhängig vom PDF-Exporter erarbeitet werden. Die Reihenfolge unten ermöglicht einen vollständig benutzbaren Schreibablauf vor dem ersten KI-Aufruf. Die Quellen für die Integrationsarbeit sind die [GPUI-Kit-Beispiele](https://github.com/longbridge/gpui-kit), das [Codex-App-Server-Protokoll](https://developers.openai.com/codex/app-server) und [Typst](https://github.com/typst/typst).

## Arbeitspakete

### 1. Projekt mit lesbaren Dateien öffnen

**Dateien** `project.rs`, `format.rs`, `ui/project.rs` sowie `tests/project_roundtrip.rs`.

**Schnittstelle** `Project::open(root: &Path) -> Result<Project>; Project::create(root: &Path) -> Result<Project>`.

- [ ] Verhalten in `tests/project_roundtrip.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(reopened.scene(id).body(), "Eine Tür öffnet sich."); assert!(reopened.has_missing_ref(lost));`
- [ ] `cargo test --test project_roundtrip` ausführen. Der Test muss die fehlende Funktion erkennen (das minimale Cargo-Testziel dafür zuerst anlegen).
- [ ] Rust-Cargo-Paket writer, GPUI-Kit-Fenster und Projektwahl gemeinsam anlegen. Passende GPUI-/Kit-Revisionen anhand ihrer Beispiele gemeinsam bauen und in Cargo.lock fixieren. Frontmatter mit UUID, Art und optionalen Referenzen, danach Markdown-Text. Kapitelplan, Weltwissen und Planung über stabile IDs laden. Unbekannte Felder und unveränderte Dateien beim Schreiben erhalten. Fehler pro Datei anzeigen, doppelte IDs als mehrdeutig behandeln. Testordner extern umbenennen, Szene löschen, zweite ID duplizieren und intakte Inhalte weiter öffnen.
- [ ] `cargo test --test project_roundtrip` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions und der oben beschriebene praktische Nachweis.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 2. Automatisch sichern und Git-Version erstellen

**Dateien** `storage.rs`, `version.rs`, `ui/project.rs` sowie `tests/autosave_git.rs`.

**Schnittstelle** `Store::flush(&mut self) -> Result<()>; Store::checkpoint(&mut self, message: &str) -> Result<Checkpoint>`.

- [ ] Verhalten in `tests/autosave_git.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(disk_text(), edited); assert_eq!(head(), before); click_save(); assert_eq!(new_commits(), 1);`
- [ ] `cargo test --test autosave_git` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Git bei Projektanlage initialisieren. Änderungen je Datei mit Revision und kurzem Debounce atomar schreiben, bei Wechsel, Schließen und Commit ausstehende Schreibvorgänge abwarten. Bei paralleler externer Änderung beide Fassungen zur Auflösung zeigen. Dateistand vor Ersetzung prüfen, für App-Schreiber pro Projekt serialisieren. Git-CLI über Argumentlisten ansprechen. Versionsumfang auf Projektdateien begrenzen und bestehenden fremden Index respektieren. Fehlende Identität, Indexsperre, externe Staging-Konflikte und Schreibfehler sichtbar melden. Bei unverändertem Stand vorhandene Version anzeigen. Tests mit temporärem Repo und eigener Git-Identität, Doppelklick, simuliertem Fehler und erneutem Öffnen.
- [ ] `cargo test --test autosave_git` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 3. Ereignisnetz bearbeiten

**Dateien** `timeline.rs`, `ui/timeline.rs` sowie `tests/timeline_order.rs`.

**Schnittstelle** `Timeline::relate(&mut self, from: Id, to: Id, kind: Relation, note: String) -> Result<()>; Timeline::compare(&self, a: Id, b: Id) -> TemporalOrder`.

- [ ] Verhalten in `tests/timeline_order.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(order(contact, deadline), Before); assert_eq!(order(a, b), Unknown); assert!(cycle.is_err());`
- [ ] `cargo test --test timeline_order` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Ereignisse anlegen, auswählen, ändern und verknüpfen. Zwei Beziehungstypen modellieren, eine Ursache impliziert zeitlichen Vorrang. Optionale Erläuterung und Zeitangabe wie „100 Jahre später“ speichern. Stabile Darstellung auch für parallele oder isolierte Ereignisse. Zyklen bei Eingabe erklären, extern entstandene Zyklen diagnostizieren. Karten im GPUI-Fenster per Maus und Tastatur erreichbar machen.
- [ ] `cargo test --test timeline_order` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions und der oben beschriebene praktische Nachweis.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 4. Szenen ableiten und Kapitel ordnen

**Dateien** `outline.rs`, `ui/outline.rs`, `ui/timeline.rs` sowie `tests/scene_outline.rs`.

**Schnittstelle** `Outline::derive_scene(&mut self, event: Id, bullets: &str) -> Id; Outline::place(&mut self, scene: Id, chapter: Id, index: usize) -> Result<()>`.

- [ ] Verhalten in `tests/scene_outline.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(timeline_bounds(scene), (after, before)); assert_eq!(chapter_of(scene), chapter_two);`
- [ ] `cargo test --test scene_outline` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Eine oder mehrere Szenen mit Stichpunkten und Figurenbezügen aus einem Ereignis erzeugen. Zuordnung zum Ereignis, relative Grenzen und optionale präzisere Zeitangabe separat von der Kapitelposition speichern. Kapitel erstellen, benennen und verschieben, Szenen auch ohne Kapitel öffnen. Beim Schließen veränderte Dokumente über Store sichern. Nach Neuladen prüfen, dass eine verschobene Rückblende ihren Zeitpunkt behält.
- [ ] `cargo test --test scene_outline` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 5. Szenen in Markdown bearbeiten

**Dateien** `passage.rs`, `ui/editor.rs` sowie `tests/editor_passages.rs`.

**Schnittstelle** `SceneText::replace(&mut self, target: &Anchor, markdown: &str) -> Result<()>; SceneText::set_concept(&mut self, target: &Anchor, enabled: bool) -> Result<()>`.

- [ ] Verhalten in `tests/editor_passages.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert!(concept_after_replacement); assert!(!ordinary_list_is_concept); assert_eq!(undo_text(), original);`
- [ ] `cargo test --test editor_passages` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Kit-Editor mit ein-/ausblendbarer Markdown-Vorschau verbinden. Fett, kursiv, Überschrift und Liste als rückgängig machbare Auswahloperationen anbieten. Konzeptgrenzen in Markdown als gepaarte HTML-Kommentare mit stabiler Passagen-ID speichern und sichtbar beschriften. Konzeptstatus beim Ersetzen auf neuen Text übertragen. Anker tragen Dokument-ID, Revision, Zitat und lokalen Kontext. Vor Aktionen neu auflösen, bei Mehrdeutigkeit Auswahl anfordern. UTF-8, Absatzgrenzen, deutsche Anführungszeichen und kombinierende Zeichen prüfen.
- [ ] `cargo test --test editor_passages` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions und der oben beschriebene praktische Nachweis.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 6. Manuskript zusammenhängend überarbeiten und exportieren

**Dateien** `manuscript.rs`, `export.rs`, `ui/manuscript.rs` sowie `tests/manuscript_export.rs`.

**Schnittstelle** `Manuscript::from_project(project: &Project) -> Manuscript; export(book: &Manuscript, format: ExportFormat, path: &Path) -> Result<()>`.

- [ ] Verhalten in `tests/manuscript_export.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(changed_scene_ids(), [second]); assert!(markdown.contains("Konzept")); assert!(pdf_text.contains("Konzept"));`
- [ ] `cargo test --test manuscript_export` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Eine Dokumentreihenfolge für Endlosansicht und beide Exporte verwenden. Bearbeitbare Szenen mit stabilen Grenzen unter Kapitelüberschriften anzeigen, Auswahl über Szenengrenzen beim Ersetzen je betroffener Szene abbilden und Undo gemeinsam führen. „Konzept“ lesbar ausgeben. Markdown-Struktur für PDF über einen begrenzten Dokumentbaum an Typst übergeben, Nutztext als Daten behandeln. Schriften mit Umlauten einbetten, lange Kapitel über Seiten umbrechen. Export in temporäre Datei schreiben und erst bei Erfolg ersetzen. Test mit Rückblende, gemischtem Konzeptstatus und aus PDF extrahiertem Text; Layout mehrseitig visuell prüfen.
- [ ] `cargo test --test manuscript_export` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions und der oben beschriebene praktische Nachweis.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 7. Lokales Figuren- und Quellenverzeichnis aufbauen

**Dateien** `knowledge.rs`, `sources.rs`, `ui/world.rs` sowie `tests/source_knowledge.rs`.

**Schnittstelle** `Sources::build(project: &Project) -> Sources; Sources::resolve(&self, anchor: &Anchor) -> Resolution`.

- [ ] Verhalten in `tests/source_knowledge.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(resolve_after_move(), Unique(scene)); assert_eq!(resolve_duplicate_quote(), Ambiguous);`
- [ ] `cargo test --test source_knowledge` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Figuren, Orte, Konzepte, Ideen und zeitgebundene Wissens- oder Glaubensaussagen anlegen und bearbeiten. Aus allen Dateien zur Laufzeit Belege mit Textstelle, relativer Zeit, Konzeptstatus und Form (Stichpunkt oder Prosa) bilden. Weltwissen des Autors und Figurenwissen separat adressieren. Von einer Quelle zum Absatz springen. Externe Änderungen invalidieren betroffene Quellen. Kein Datenbankindex auf Platte.
- [ ] `cargo test --test source_knowledge` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 8. Codex App Server opt-in anbinden

**Dateien** `ai/mod.rs`, `ai/server.rs`, `ai/session.rs`, `ui/ai_settings.rs` sowie `tests/ai_transport.rs`.

**Schnittstelle** `AiSession::request(&mut self, request: AiRequest) -> Result<RequestId>; AiSession::disable(&mut self)`.

- [ ] Verhalten in `tests/ai_transport.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(disabled_process_starts(), 0); assert_eq!(late_result_after_disable(), None);`
- [ ] `cargo test --test ai_transport` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Projektbezogene Freigabe lokal an Benutzer und Projekt binden, damit das Öffnen eines fremden Projekts nie fremdes Opt-in erbt. Verfügbare Codex-CLI erkennen oder lokale Binärdatei wählen. app-server als Kindprozess über stdio starten. initialize/initialized, account/read, bei Bedarf account/login/start mit chatgpt und offiziellem Browserlogin; account/login/completed auswerten. thread/start bzw. thread/resume, turn/start, Text-Deltas und turn/completed zuordnen. Bei Deaktivierung laufende Turns unterbrechen, Kindprozess beenden und verspätete Ergebnisse verwerfen. Keine Schlüssel im Buchprojekt. Kontingent, fehlende CLI, abgebrochener Login und Prozessabbruch als nachvollziehbare Zustände. Beschränkten Lesekontext liefern, Werkzeugaktionen mit Seiteneffekten ausschließen. Protokoll-Fixture mit vertauschten IDs, Teilzeilen, Abbruch und vollständiger Anmeldung; zusätzlich echter Opt-in-Smoke-Test mit Testbuch.
- [ ] `cargo test --test ai_transport` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions und der oben beschriebene praktische Nachweis.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 9. Brainstorming am Ereignispaar

**Dateien** `ai/brainstorm.rs`, `ui/brainstorm.rs` sowie `tests/pair_chat.rs`.

**Schnittstelle** `Brainstorm::open(project: &Project, pair: [Id; 2]) -> Result<Brainstorm>; Brainstorm::promote(&mut self, choice: IdeaPart, links: &[Link]) -> Result<Id>`.

- [ ] Verhalten in `tests/pair_chat.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(open_pair(b,a).messages(), saved_messages); assert_eq!(events_before_accept(), initial_events);`
- [ ] `cargo test --test pair_chat` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Ungeordnetes Ereignispaar als Chat-Schlüssel; einzelne Ideen mit ID im Antworttext markieren. Nachrichten und Markierungen als Markdown speichern. Nach Neustart mit Verlauf weiterarbeiten, bei verlorener Server-Thread-ID lokalen Verlauf als Kontext übernehmen. Auswahl als neues Ereignis samt explizit gewählten Beziehungen einfügen; doppelte Übernahme beim Wiederholen verhindern. Quelle zur ursprünglichen Nachricht bewahren.
- [ ] `cargo test --test pair_chat` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 10. Fragen zur Geschichte mit Quellen beantworten

**Dateien** `ai/research.rs`, `ui/research.rs` sowie `tests/timeline_research.rs`.

**Schnittstelle** `research(project: &Project, at: Id, question: Question) -> Result<SourcedAnswer>`.

- [ ] Verhalten in `tests/timeline_research.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(hits(), [later_planned, earlier_written]); assert!(open_ideas().contains(&marked_idea));`
- [ ] `cargo test --test timeline_research` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Gesamtes Manuskript einschließlich unbestätigter Details in begrenzten Textpaketen durchsuchen; Quellenkandidaten über alle Pakete sammeln und anhand tatsächlicher Textstellen prüfen. Vor der Antwort Teilordnung, Zeitbezug und Konzeptstatus anwenden. Ungeordnete Begegnungen ausdrücklich offenlassen. Fragen nach Figurenwissen und Autorenplänen getrennt behandeln; markierte Ideen als „Noch offen“ mit Chatlink zeigen. Rückblende in späterem Kapitel, ausgeschriebenes Konzept und fehlender Beleg als Pflichtfälle. Bei unvollständiger Suche Reichweite kenntlich machen.
- [ ] `cargo test --test timeline_research` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 11. Auswahl im Dialog ausformulieren

**Dateien** `ai/draft.rs`, `ui/draft.rs` sowie `tests/draft_acceptance.rs`.

**Schnittstelle** `Draft::accept(&self, excerpt: &str, target: &Anchor, scene: &mut SceneText) -> Result<()>`.

- [ ] Verhalten in `tests/draft_acceptance.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(changed_passages(), [selected]); assert!(selected_is_concept()); assert_eq!(unaccepted_notes(), initial_notes);`
- [ ] `cargo test --test draft_acceptance` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Markierte Stichpunkte, ganze Szene, Figurenwissen zum Szenenzeitpunkt und Beispiele ausgeschriebener Szenen mit getrennten Rollen übergeben. Stilbeispiele als sprachliche Orientierung kennzeichnen. Entscheidende Wissenslücke als Rückfrage, unwichtige offenlassen. Antwort im Gespräch verwenden und als optionale zeitgebundene Notiz anbieten. Entwurf im gespeicherten Gespräch überarbeiten und nur gewählte Passage an bestätigter Zielstelle ersetzen. Bei zwischenzeitlicher Bearbeitung Quelle erneut zeigen. Modellqualität mit festem Beispiel auf Perspektive und vorweggenommene Enthüllung prüfen.
- [ ] `cargo test --test draft_acceptance` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 12. Detailvorschläge anbieten

**Dateien** `ai/details.rs`, `ui/details.rs` sowie `tests/detail_triggers.rs`.

**Schnittstelle** `suggest_details(trigger: Trigger, snapshot: &SceneSnapshot) -> Result<Vec<Suggestion>>`.

- [ ] Verhalten in `tests/detail_triggers.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(requests_after_pause(), 0); assert_eq!(requests_after_leaving_changed_scene(), 1); assert_eq!(writes_before_accept(), 0);`
- [ ] `cargo test --test detail_triggers` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Manuellen Button und Szenenwechsel an dieselbe opt-in-geschützte Anfrage anschließen. Automatische Anfragen je Inhaltsrevision bündeln, Ergebnisse ihrer Ursprungsszene zuordnen. Vorschlag zeigt Textstelle, Ziel und änderbaren Inhalt. Annahme aktualisiert Notiz, Verwerfen beendet Vorschlag. Nach Deaktivierung und während bloßer Schreibpause keine neue Anfrage. Stale Quellen bei Übernahme erneut prüfen.
- [ ] `cargo test --test detail_triggers` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 13. Beabsichtigte Widersprüche verwalten

**Dateien** `ai/consistency.rs`, `ui/checks.rs` sowie `tests/consistency_review.rs`.

**Schnittstelle** `check(project: &Project) -> Result<Vec<Finding>>; dismiss(finding: &Finding, reason: Option<String>) -> Decision`.

- [ ] Verhalten in `tests/consistency_review.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert!(hidden_after_scene_move()); assert!(visible_after_change_from_100_to_50());`
- [ ] `cargo test --test consistency_review` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Prüfung nur über „Konsistenz prüfen“. Aussagen aus Manuskript, Ereignissen und Weltwissen mit Fundstellen gegenüberstellen. Entscheidungen mit optionaler Begründung als Markdown speichern. Fund über Quellenidentitäten und relevante normalisierte Aussagen wiedererkennen. Verschiebung und kosmetische Korrektur testen; geänderte Jahreszahl oder Negation erneut melden. Unsichere Übereinstimmungen zur Prüfung zeigen.
- [ ] `cargo test --test consistency_review` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

### 14. Desktopfassung auf drei Systemen abnehmen

**Dateien** `tests/desktop_journey.rs`, `tests/fixtures/imperium/`, `docs/abnahme-desktop.md`.

**Schnittstelle** Öffentliche App-Einstiegspunkte und Buchordner aus den vorherigen Paketen.

- [ ] Verhalten in `tests/desktop_journey.rs` zunächst als Integrationstest festhalten. Kernassertionen: `assert_eq!(reopened_order(), exported_order()); assert_eq!(git_versions_added(), 1); assert!(works_without_codex());`
- [ ] `cargo test --test desktop_journey` ausführen. Der Test muss die fehlende Funktion erkennen.
- [ ] Einen kompletten Durchlauf mit Testbuch durchführen. Zusätzlich auf macOS, Windows und Linux Fenster, Eingabemethoden, Tastaturkürzel, lange Szene, Tastaturnavigation, Dateidialoge, externe Änderungen, Git und PDF visuell prüfen. Binaries je System im Release-Modus bauen und starten. Offline-Durchlauf mit deaktivierter KI, reproduzierbarer KI-Durchlauf mit Protokoll-Fixture, abschließend tatsächlichen Login, Brainstorming, Abbruch und Quellensprung mit aktiviertem Testprojekt prüfen. Pro Plattform Ergebnis und konkret ungeprüfte Punkte vermerken.
- [ ] `cargo test --test desktop_journey` ausführen und die beschriebenen Anwendungsfälle prüfen. Erwartet werden erfolgreiche Assertions und der oben beschriebene praktische Nachweis.
- [ ] Geprüfte Änderungen als eigenständigen Git-Commit mit Co-Autoren-Trailer festhalten.

## Abschlussnachweis

`cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` und `cargo build --release --locked` müssen auf allen drei Desktopplattformen erfolgreich sein. Die Endabnahme folgt dem Weg vom Erstkontakt über eine Rückblende und eine markierte Chatidee bis zu einem gemischten Kapitel aus Konzept und Prosa. Im exportierten PDF und Markdown müssen dieselbe Kapitelreihenfolge und dieselben Konzeptkennzeichnungen erkennbar sein. Ein nicht durchgeführter Plattform- oder Abo-Test bleibt im Abnahmeprotokoll offen.
