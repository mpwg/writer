# UX-Entwurf für die Schreibwerkstatt

Der [klickbare Prototyp](../../prototyp/index.html) zeigt das Desktop-Dokumentfenster mit „Der Herr der Ringe“ als Beispielbuch. Öffne `docs/prototyp/index.html` direkt im Browser. Der Umschalter **Prototyp-Ansicht** steht unter dem Fenster. Speichern, KI und Export zeigen Beispielzustände.

Die Schreibwerkstatt ist eine native Rust-Desktop-Anwendung mit GPUI. Ein Buchprojekt öffnet sich als Dokumentfenster für Planung, Entwurf und Überarbeitung. Die folgenden Funktionsskizzen verwenden „Das verlorene Imperium“. Das Erscheinungsbild ist im klickbaren Prototyp ausgearbeitet. [Freigegebene Produktspezifikation](2026-11-20-schreibprogramm-design.md) · [Umsetzungsplan](../plans/2026-11-20-schreibwerkstatt.md)

## Fenster und Gestaltung

Die Titelleiste trägt den Projektnamen und die aktive Ansicht. Darunter hat jeder Arbeitsbereich eine kompakte Werkzeugzeile für seine Befehle. Die Quellliste links bündelt **Timeline**, **Kapitel**, **Manuskript** und **Welt**. **Schreiben** führt zurück zur aktiven Szene. Konsistenz und Einstellungen stehen unter **Projektwerkzeuge**. Die Statusleiste bleibt am unteren Fensterrand. Sie zeigt die Dateisicherung, die letzte lokale Version und im Editor das Dokumentformat.

Die Quellliste startet bei etwa 200 px. Bei Bedarf öffnet sich rechts ein Inspektor für Eigenschaften, Quellen, Prüffunde oder ein Gespräch. Für die native Umsetzung sind verstellbare Trenner und eine einklappbare Quellliste vorgesehen. Bei wenig Fensterbreite teilen sich Editor und Inspektor die Arbeitsfläche über einen Ansichtswechsel. Der Fokus kehrt nach dem Schließen zum auslösenden Bedienelement zurück. Die Prototyp-Vorschau stapelt die Bereiche bei schmalen Fenstern, damit ihr Inhalt sichtbar bleibt.

Die Oberfläche verwendet kühles Grau für Fensterleisten und Quellliste, Weiß für die Arbeitsfläche und Schieferblau für Auswahl und Fokus. Dünne Trenner markieren die Arbeitsbereiche. Systemschrift gilt für Menüs, Listen und Inspektoren, Monospace für Markdown und Serifenschrift für den gesetzten Buchtext. Der Editor zentriert den Text auf etwa 70 Zeichen pro Zeile. Die Vorschau bleibt beim ersten Öffnen eingeklappt. Ein ockerfarbener Rand mit **Konzept** kennzeichnet Entwurfspassagen. Der Dunkelmodus folgt der Systemdarstellung.

| Ansicht | Aufbau im Desktopfenster |
| --- | --- |
| Projektwahl | Kompakte Liste zuletzt geöffneter Buchordner mit Öffnen- und Neu-Befehl. |
| Timeline | Zeichenfläche mit Raster und umschaltbarer Ereignisliste. Der Inspektor zeigt Eigenschaften des ausgewählten Ereignisses. |
| Brainstorming | Die beiden Ereignisse bleiben auf der Arbeitsfläche sichtbar. Das zugehörige Gespräch steht daneben. |
| Kapitelordnung | Eingerückte Szenen unter aufklappbaren Kapitelzeilen. Der Inspektor zeigt Erzählposition und Zeitbezüge. |
| Szeneneditor | Dokumentkopf mit Szenenname und Kapitelpfad, schmale Formatierleiste und zentrierter Markdown-Text. Vorschau und Szenendetails teilen sich den rechten Bereich. |
| Weltkartei | Quellenliste für Figuren, Orte und Ideen, Detailansicht für den gewählten Eintrag und Quellennachweise im Inspektor. |
| Gesamtmanuskript | Inhaltsverzeichnis neben einer weißen Lesefläche auf grauem Grund. Export steht in der Werkzeugzeile. |
| Konsistenzprüfung | Befundliste links. Rechts stehen die beiden Aussagen mit ihren Fundstellen und dem Vermerkbefehl. |
| Projekteinstellungen | Nach Thema gruppierte Felder für KI, Projektordner und Versionen. |

Die native App verwendet die Menü- und Tastaturkonventionen des jeweiligen Betriebssystems. **Speichern** erstellt eine lokale Git-Version und ist mit ⌘S auf macOS beziehungsweise Strg+S unter Windows und Linux erreichbar. Das automatische Schreiben des Texts zeigt die Statusleiste separat.

## 1. Projekt öffnen oder anlegen

```text
┌─────────────────────────────────────────────────────────────────┐
│ Schreibwerkstatt                                                │
│                                                                 │
│ Buchprojekt öffnen                                              │
│ [ Ordner öffnen … ]   [ Neues Buchprojekt … ]                   │
│                                                                 │
│ Zuletzt geöffnet                                                │
│ Das verlorene Imperium                               [ Öffnen ] │
│ ~/Bücher/Das verlorene Imperium                                 │
└─────────────────────────────────────────────────────────────────┘
```

Ein neues Buch braucht einen Titel und einen Zielordner. Anschließend öffnet sich die leere Timeline mit „Erstes Ereignis anlegen“. Ein bestehendes Buch kehrt zur letzten Ansicht zurück, sofern deren Inhalt noch erreichbar ist. Bei fehlenden Dateien oder mehrdeutigen Verweisen erscheint oberhalb der Arbeitsfläche „Projekthinweise anzeigen“. Die Liste zeigt Datei und betroffenen Verweis. Intakte Inhalte sind weiterhin zugänglich.

## 2. Timeline und Ereignisdetails

```text
┌────────────────────────────────────────────────────────────────────────────────┐
│ Das verlorene Imperium ▾                                        [ Speichern ]  │
├────────────┬────────────────────────────────────────────────┬──────────────────┤
│ ▸ Timeline │ Timeline                 [ Netz / Liste ]  [+] │ Ereignis         │
│   Kapitel  │                                                │ Erstkontakt      │
│ Manuskript │ [Erstkontakt] ── 100 Jahre ──▶ [Fristablauf]   │ Mara empfängt    │
│       Welt │                                    ╎           │ ein Signal.      │
│            │                                    ▼           │                  │
│            │                                [Aufbruch]      │ [Szene ableiten] │
│            │                                                │ [Verknüpfen …]   │
│            │ Noch ohne zeitliche Einordnung                 │                  │
│            │ [Fund der Sternenkarte]                        │ Szenen           │
│            │                                                │ ↗ Das Signal     │
│            │ ─▶ zeitlich davor   ···▶ verursacht            │                  │
└────────────┴────────────────────────────────────────────────┴──────────────────┘
```

Eine Ereigniskarte zeigt Titel und bei Platz einen Stichpunkt. Ein Klick auf den Pfeil öffnet Beziehungstyp, Erläuterung und relative Zeitangabe. „Verknüpfen …“ bietet eine Zielauswahl per Maus oder Tastatur. Würde eine Verbindung einen Zyklus erzeugen, nennt die App den betroffenen Pfad direkt im Dialog. Ursachenpfeile drücken ebenfalls zeitlichen Vorrang aus. Bei parallelen oder ungeordneten Pfaden ergibt sich die Reihenfolge aus den Verbindungen. Isolierte Karten stehen in einem eigenen Bereich ohne Zeitachse.

„Szene ableiten“ öffnet ein Formular für Titel, Stichpunkte, Figuren und optionale zeitliche Grenzen. Nach dem Anlegen führt „Szene öffnen“ direkt in den Editor. Ein Ereignis kann auf mehrere Szenen verweisen.

## 3. Brainstorming zwischen zwei Ereignissen

```text
┌──────────────────────────────────────┬───────────────────────────────────┐
│ Timeline · 2 Ereignisse ausgewählt   │ Gespräch                          │
│                                      │ Erstkontakt → Fristablauf         │
│ [Erstkontakt] ───────▶ [Fristablauf] │                                   │
│                                      │ Eine zweite Botschaft erreicht    │
│ [ Lücke erkunden ]                   │ die Station kurz vor Fristablauf. │
│                                      │ [ Idee markieren ]                │
│                                      │ [ Als Ereignis übernehmen … ]     │
│                                      │                                   │
│                                      │ [ Wie könnte Mara reagieren?   ]  │
└──────────────────────────────────────┴───────────────────────────────────┘
```

Die Auswahl desselben Paars öffnet den gespeicherten Verlauf auch nach erneutem Öffnen des Projekts. Eine markierte Textstelle wird als offene Idee referenzierbar. Beim Übernehmen zeigt ein Formular den ausgewählten Text als Vorschlag für ein neues Ereignis. Für jede gewünschte Verbindung werden Richtung und Beziehungstyp gewählt. Erst die Bestätigung ändert die Timeline.

## 4. Kapitel und Szenen ordnen

```text
┌───────────────────────────────────────┬───────────────────────────────────┐
│ Kapitel                   [+ Kapitel] │ Szene · Das Signal                │
│                                       │                                   │
│ ▾ Kapitel 1 · Die Rückkehr            │ In der Erzählung                  │
│   ≡  Startvorbereitungen              │ Kapitel 3 · Die verlorene Station │
│ ▾ Kapitel 2 · Der Anleger             │                                   │
│   ≡  Am Anleger                       │                                   │
│ ▾ Kapitel 3 · Die verlorene Station   │                                   │
│   ≡  Das Signal          [Konzept]    │ In der Timeline                   │
│   ≡  Die verlassene Station           │ ↗ Erstkontakt                     │
│ ▾ Ohne Kapitel                        │ Vor ↗ Fristablauf                 │
│   ≡  Ein falscher Name                │                                   │
│                                       │ [ Szene bearbeiten ]              │
└───────────────────────────────────────┴───────────────────────────────────┘
```

Kapitel und Szenen lassen sich ziehen oder über das Kontextmenü verschieben. Kapitellose Szenen sind sofort bearbeitbar. Eine Rückblende lässt sich später im Buch erzählen. Der Detailbereich zeigt dafür die Kapitelposition und die zeitlichen Bezüge in zwei getrennten Feldern. Chronologie wird über die Timeline-Bezüge bearbeitet. „Konzept“ an einer Szene weist auf enthaltene Konzeptpassagen hin.

## 5. Eine Szene schreiben

```text
┌───────────────────────────────────────────────────────────────────────┐
│ Kapitel 3 › Das Signal                 [ Vorschau ] [ Szenendetails ] │
├─────────────────────────────────────────────┬─────────────────────────┤
│ [ B ] [ I ] [ Überschrift ] [ Liste ]       │ Vorschau                │
│ [ Als Konzept markieren ]                   │                         │
│                                             │ Mara stellte das Radio  │
│ Mara stellte das Radio auf die Fensterbank. │ auf die Fensterbank.    │
│                                             │                         │
│ ▎ Konzept                                   │ ▎ Konzept               │
│ ▎ - Mara hört ihren Namen im Rauschen.      │ ▎ • Mara hört ihren     │
│ ▎ - Das Signal bricht plötzlich ab.         │ ▎   Namen im Rauschen.  │
│                                             │ ▎ • Das Signal bricht   │
│                                             │ ▎   plötzlich ab.       │
├─────────────────────────────────────────────┴─────────────────────────┤
│ Auf Festplatte gesichert · Änderungen seit letzter Version            │
└───────────────────────────────────────────────────────────────────────┘
```

Markdown ist die primäre Schreibfläche. Die optionale Vorschau teilt sich den Platz mit dem Editor. „Als Konzept markieren“ bezieht sich auf eine Passage. Der Rand und die Beschriftung bleiben auch bei ausformulierter Prosa sichtbar, bis „Konzeptkennzeichnung entfernen“ gewählt wird. Normale Markdown-Listen sehen wie der übrige Text aus.

Mit aktiver KI erscheint bei einer Stichpunktauswahl „Ausformulieren …“. Im Kontextbereich steht über dem Gespräch der Zeitpunkt der Szene. Eine entscheidende Wissenslücke erscheint als konkrete Rückfrage. „Als Figurennotiz speichern …“ zeigt bei Bedarf die vorgeschlagene Notiz mit Zeitbezug. Für die Übernahme eines Entwurfs zeigt der Bereich zuerst **Ersetzen** mit der Zielauswahl und **Einfügen** mit dem gewählten Textausschnitt. Bei einer inzwischen geänderten, mehrdeutigen Zielstelle wird die Auswahl erneut abgefragt.

„Vorschläge anzeigen“ öffnet Detailvorschläge mit Textbeleg, Zielnotiz und bearbeitbarem Inhalt. Bei aktivierter KI kann das Verlassen einer geänderten Szene die Suche automatisch anstoßen. Ein Hinweis an der Ursprungsszene hält fertige Vorschläge erreichbar. Die Notiz ändert sich mit „Übernehmen“. „Verwerfen“ schließt den Vorschlag.

## 6. Weltkartei und Fragen zur Geschichte

```text
┌─────────────────────┬──────────────────────────┬──────────────────────────┐
│ Welt                │ Mara                     │ Frage zur Geschichte     │
│ [ Einträge suchen ] │                          │ Zeitpunkt · Das Signal   │
│                     │ Figurennotiz             │                          │
│ ▾ Figuren           │ Ehemalige Funkerin       │ Wann traf sie Ivo        │
│   ▸ Mara            │                          │ zuletzt?                 │
│     Ivo             │ Wissen und Überzeugungen │                          │
│ ▸ Orte              │ Ab ↗ Erstkontakt         │ Geplant · Am Anleger     │
│ ▸ Konzepte          │ Kennt das Rufzeichen.    │ ↗ Kapitel 2 · Konzept    │
│ ▸ Ideen             │ ↗ Das Signal             │                          │
│                     │                          │ Erzählt · In der Werft   │
│ [+ Eintrag]         │                          │ ↗ Kapitel 4 · Rückblende │
└─────────────────────┴──────────────────────────┴──────────────────────────┘
```

Die Kartei zeigt allgemeine Figurennotizen und zeitgebundene Aussagen zum Wissen oder Glauben einer Figur. Ein Quellensprung öffnet die zugehörige Passage und setzt den Fokus dorthin. Der Kontextbereich „Frage zur Geschichte“ ist auch aus dem Szeneneditor erreichbar und übernimmt von dort den Szenenzeitpunkt. Außerhalb einer Szene ist der verwendete Zeitbezug sichtbar und auswählbar.

Antworten zeigen geplante und bereits erzählte Fundstellen mit ihrem Status. Eine ausformulierte Konzeptpassage behält ihre Kennzeichnung. Bei zeitlich ungeordneten Treffern bleibt die Abfolge als „Reihenfolge offen“ gekennzeichnet. Bei Fragen nach geplanten Entwicklungen führen Einträge unter „Noch offen“ zur markierten Chatstelle.

## 7. Gesamtmanuskript und Export

```text
┌─────────────────────────┬──────────────────────────────────────────────────┐
│ Kapitel                 │ Gesamtmanuskript                    [ Export … ] │
│ ▸ Die Rückkehr          │                                                  │
│   Der Anleger           │ Die Rückkehr                                     │
│   Die verlorene Station │                                                  │
│                         │ Die Station lag seit Tagen im Schatten.          │
│                         │                                                  │
│                         │ ▎ Konzept                                        │
│                         │ ▎ Mara findet die zweite Botschaft.              │
│                         │                                                  │
│                         │ Der Anleger                                      │
└─────────────────────────┴──────────────────────────────────────────────────┘
```

Das Manuskript ist ein durchgehend scroll- und bearbeitbarer Text in Kapitelreihenfolge. Das Inhaltsverzeichnis springt zu einem Kapitel. Beim Bearbeiten erscheint am Abschnittsrand der Szenenname als Rücksprung zur Einzelansicht. Die App ordnet Änderungen den betroffenen Szenendateien zu. Der Exportdialog zeigt „Markdown“ oder „PDF“, den Zielpfad und „Exportieren“. Beide Formate führen Kapitel und Konzeptpassagen an ihrer Manuskriptposition. Nach Erfolg zeigt die App den Dateinamen mit „Datei öffnen“.

## 8. Konsistenz prüfen

```text
┌───────────────────────────┬──────────────────────────────────┐
│ Konsistenzprüfung         │ Dauer der Rückkehrfrist          │
│ [ Konsistenz prüfen ]     │                                  │
│                           │ Timeline · 100 Jahre             │
│ ▸ Dauer der Rückkehrfrist │ ↗ Erstkontakt → Fristablauf      │
│   Rufzeichen der Station  │                                  │
│                           │ Manuskript · 50 Jahre            │
│ [ Vermerkte anzeigen ]    │ ↗ Kapitel 2 · Der Anleger        │
│                           │                                  │
│                           │ [ Als beabsichtigt markieren … ] │
└───────────────────────────┴──────────────────────────────────┘
```

Die Prüfung öffnet sich aus dem Projektmenü und startet über „Konsistenz prüfen“. Ein Prüffund stellt Aussagen mit Zitaten und Sprüngen gegenüber. „Als beabsichtigt markieren“ bietet eine optionale Begründung. Unter „Vermerkte anzeigen“ bleiben solche Entscheidungen einsehbar. Eine inhaltlich geänderte Aussage kann erneut als Prüffund auftauchen, auch wenn ihre Szene denselben Namen trägt.

## 9. Einstellungen und Zustände beim Speichern

```text
┌───────────────────────────────────────────────────────┐
│ Projekteinstellungen · Das verlorene Imperium         │
│                                                       │
│ KI für dieses Projekt                         [ Aus ] │
│ Mit ChatGPT über Codex verbinden                      │
│                                                       │
│ Beim Verlassen einer geänderten Szene kann die App    │
│ Detailvorschläge anfordern.                           │
│                                                       │
│                                        [ Schließen ]  │
└───────────────────────────────────────────────────────┘
```

KI ist pro Projekt und Benutzer zunächst ausgeschaltet. KI-Aktionen führen bei Bedarf zu dieser Einstellung. Beim Einschalten werden Anmeldung und Verbindungszustand sichtbar. Abbruch, fehlende Einrichtung oder erschöpftes Kontingent erscheinen im betroffenen Bereich mit passender nächster Aktion. Beim Ausschalten endet eine laufende Anfrage. Projektarbeit, Versionierung und Export bleiben zugänglich.

Die Fußzeile zeigt den Schreibzustand der Dateien. „Speichern“ trägt den erklärenden Tooltip „Lokale Git-Version erstellen“. Ein erfolgreicher Commit erscheint mit Uhrzeit als letzte Version. Während des Commits ist der Button vorübergehend inaktiv. Wenn nichts zu versionieren ist, zeigt die App die vorhandene Version.

| Zustand | Anzeige und Aktion |
| --- | --- |
| Automatisches Schreiben läuft | „Wird auf Festplatte gesichert …“ |
| Dateien geschrieben, Änderungen seit Commit | „Auf Festplatte gesichert · Änderungen seit letzter Version“ |
| Schreiben fehlgeschlagen | Sichtbarer Hinweis mit Dateiname, Fehlerdetails und „Erneut versuchen“ |
| Lokale und externe Fassungen derselben Datei | Vergleich mit „In der App“ und „Auf Festplatte“. Die gewählte oder manuell zusammengeführte Fassung wird nach Bestätigung geschrieben. |
| Git-Commit fehlgeschlagen | „Lokale Version konnte nicht erstellt werden“ mit Ursache und „Erneut versuchen“. Sind die Dateien bereits geschrieben, bleibt ihr Sicherungsstatus sichtbar. |

## Durchgang für die Abnahme

1. Ein Buch anlegen und Erstkontakt, Fristablauf sowie Aufbruch relativ verknüpfen. Eine isolierte Karte bleibt sichtbar und zeitlich offen.
2. Aus Erstkontakt „Das Signal“ ableiten, nach Kapitel 3 verschieben und den unveränderten Zeitbezug in den Szenendetails prüfen.
3. Eine Konzeptpassage neben Prosa bearbeiten, Vorschau öffnen, automatisch sichern lassen und eine lokale Version erstellen.
4. Das Gesamtmanuskript bearbeiten und als Markdown sowie PDF exportieren. Beide Ausgaben zeigen die Konzeptkennzeichnung.
5. KI einschalten, ein Ereignispaar wieder öffnen, die markierte Idee übernehmen und aus einer Szenenfrage zur Quelle springen. Einen Detailvorschlag und einen beabsichtigten Widerspruch bestätigen.
6. Dieselbe Datei intern und extern ändern, beide Fassungen vergleichen und die gewählte Fassung explizit übernehmen.
