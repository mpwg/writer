# Eine Schreibwerkstatt für lange Geschichten

Entwurf vom 20. November 2026 zur inhaltlichen Freigabe.

## Ziel und Arbeitsweise

Die Autorin oder der Autor entwickelt eine Geschichte zunächst als Gefüge von Ereignissen. Erst daraus entstehen Szenen, Kapitel und schließlich Prosa. Die App begleitet diesen gesamten Weg auf macOS, Windows und Linux. Ein späterer mobiler Client bleibt eine Möglichkeit. Die Arbeitsdaten liegen als lesbare Text- und Markdown-Dateien vor. Git versioniert das Buchprojekt. Die KI hilft beim Nachdenken, Wiederfinden und Schreiben. Die Entscheidung über den Inhalt bleibt beim Menschen.

## Von Ereignissen zu Kapiteln

Ein Ereignis kann zunächst aus einem einzigen Stichpunkt bestehen. Eine gerichtete Verbindung drückt entweder eine zeitliche Reihenfolge oder eine Ursache aus. Bei „B wird durch A ausgelöst“ gilt zugleich, dass A vor B liegt. Jede Verbindung kann eine kurze Erläuterung tragen. Weder ein Kalenderdatum noch eine vollständige Reihenfolge aller Ereignisse ist erforderlich. Im Beispiel des verlorenen Imperiums liegen hundert Jahre zwischen Erstkontakt und vereinbarter Rückkehr. Für den späteren Aufbruch kann zunächst „nach Ablauf der Frist“ genügen.

Aus einem Ereignis lassen sich eine oder mehrere Szenen ableiten. Ihre Stichpunkte halten beispielsweise beteiligte Figuren und den beabsichtigten Verlauf fest. Eine Szene lässt sich zwischen zwei Ereignissen einordnen und später zeitlich präzisieren. Die Szenen werden zu Kapiteln zusammengestellt und in Erzählreihenfolge angeordnet. Ein Kapitelwechsel oder das Verschieben einer Rückblende im Manuskript bewahrt ihre zeitliche Einordnung und ihre Ereignisbezüge.

Wer zwei Ereignisse auswählt, kann mit der KI mehrere Ideen für die Lücke dazwischen entwickeln. Beim nächsten Auswählen desselben Paars lässt sich das Gespräch fortsetzen. Einzelne Ideen innerhalb einer Antwort können markiert werden. Über die Aufnahme von Ideen oder Teilen davon in die Timeline entscheidet der Autor ausdrücklich, einschließlich der Art ihrer Verbindungen.

## Schreiben und Überarbeiten

Die primäre Schreibfläche zeigt Markdown-Quelltext. Formatierhilfen wenden beispielsweise Hervorhebungen auf eine Auswahl oder Überschriften auf eine Zeile an. Eine zuschaltbare formatierte Vorschau zeigt das Ergebnis. Die Szenenstichpunkte werden im Manuskript nach und nach durch Prosa ersetzt. Offene Stichpunkte und ausgearbeitete Absätze können nebeneinander bestehen.

„Konzept“ ist eine explizite Kennzeichnung auf Passagenebene und gehört in die Projektdatei. Eine Liste kann daher auch gewöhnlicher Bestandteil der Geschichte sein. Die Kennzeichnung bleibt bei manueller Überarbeitung und beim Ersetzen durch KI-Prosa erhalten, bis sie ausdrücklich entfernt wird.

Für einen KI-Entwurf markiert der Autor Stichpunkte. Die KI berücksichtigt die ganze Szene, die beteiligten Figuren und deren Wissen oder Überzeugungen zum Zeitpunkt der Szene. Bereits ausgeschriebene Szenen dienen als Stilvorlagen. Spätere Enthüllungen in einer Stilvorlage dürfen einer Figur kein vorzeitiges Wissen verleihen. Ist unklar, ob eine Figur etwas weiß, fragt die KI kurz nach, sofern die Antwort den Entwurf beeinflusst. Andernfalls lässt sie die Frage offen. Eine Klärung gilt für das Gespräch. Die KI kann anbieten, sie zusätzlich als Figurennotiz mit Zeitbezug zu speichern. Der Autor entscheidet über die Übernahme.

Der Entwurf lässt sich im Gespräch weiterentwickeln. Zur Übernahme wählt der Autor die gewünschte Passage und die zu ersetzenden Stichpunkte. Andere Stichpunkte bleiben für spätere Bearbeitung stehen.

## Figurenwissen, Recherche und Konsistenz

Das Projekt enthält Figuren, Konzepte, Ideen und weiteres Weltwissen. Beim Schreiben erkennt die App mögliche neue Details. Beim Verlassen der Szene oder über „Vorschläge anzeigen“ bietet sie die Übernahme an. Eine Schreibpause löst die Anzeige nicht aus. Ein Vorschlag zeigt die Quellstelle und den vorgesehenen Eintrag. Erst eine bestätigte Übernahme ändert die zugehörige Notiz.

Die KI kann das gesamte Manuskript durchsuchen, auch wenn eine Information nie in eine Kartei übernommen wurde. Antworten verweisen auf die konkrete Passage und erlauben den Sprung dorthin. Bei „Wann haben sich die beiden zuletzt gesehen und was haben sie gemacht?“ gilt standardmäßig der Zeitpunkt der aktuellen Szene in der Timeline. Auch eine Rückblende in einem späteren Kapitel kommt als Fundstelle infrage. Ist die jüngere Begegnung nur geplant und eine frühere bereits erzählt, zeigt die App beide mit ihrem jeweiligen Status. Eine ausgeschriebene Konzeptpassage bleibt als solche erkennbar. Bei zeitlich ungeordneten Treffern benennt die Antwort die Ungewissheit.

Figurenbezogene Fragen können den damaligen Wissensstand oder die Absichten des Autors betreffen. Für „Was habe ich mit dieser Figur vor?“ werden Figurenotizen, Ereignisse und Szenen herangezogen. Markierte, bisher nicht übernommene Chatideen erscheinen gesondert unter „Noch offen“ mit Verweis auf die ursprüngliche Chatstelle.

„Konsistenz prüfen“ startet eine Prüfung auf mögliche Widersprüche zwischen Manuskript, Ereignissen und Notizen. Ein Fund zeigt die betroffenen Aussagen mit ihren Fundstellen. Beabsichtigte Abweichungen lassen sich markieren und optional begründen. Derselbe Fund bleibt bei späteren Prüfungen ausgeblendet. Als Entwurfsentscheidung soll diese Markierung beim Verschieben einer Szene bestehen bleiben und bei einer inhaltlich veränderten Aussage eine erneute Prüfung zulassen.

## Projektordner und Git

Der Ordner ist das vollständige Buchprojekt. Manuskript, Planung, Weltwissen, KI-Gespräche, Ideenmarkierungen, Konzeptstatus und Entscheidungen zu Prüffunden sind in sinnvoll gegliederten Textdateien nachvollziehbar. Eine Datenbank ist weder als Projektformat noch als Voraussetzung für die Nutzung vorgesehen. Ein möglicher Zuschnitt ist:

    Planung/         Ereignisse und Beziehungen
    Manuskript/      Szenendateien und Kapitelplan
    Welt/            Figuren, Orte und Konzepte
    Brainstorming/   Gespräche und markierte Ideen

Externe Änderungen an Markdown sind zulässig. Die App muss auch einen teilweise inkonsistenten Ordner öffnen und die intakten Inhalte zugänglich halten. Vorgeschlagen sind stabile Kennungen für verknüpfte Inhalte und verständliche Hinweise auf fehlende Ziele. Konkurrierende externe und interne Änderungen dürfen beim Speichern nicht still überschrieben werden.

**Beim Schreiben aktualisiert die App die Dateien automatisch im Git-Arbeitsverzeichnis.** Auch ohne manuellen Klick ist der automatisch gesicherte Text beim nächsten Öffnen vorhanden. Das automatische Schreiben erzeugt keinen Commit. Ein Fehler beim Schreiben muss erkennbar sein.

**„Speichern“ bedeutet einen Git-Commit erstellen.** Die App legt ein neues Buchprojekt als Git-Repository an. Vor dem Commit schreibt sie alle ausstehenden Änderungen in die Projektdateien. Ein bewusster Klick hält dann den aktuellen Projektstand in einem lokalen Commit fest. Dazu zählen etwa der geänderte Szenentext und bestätigte Figurennotizen. Erst ein erfolgreicher Commit wird als neue Version bestätigt. Schlägt der Commit fehl, bleiben die bereits geschriebenen Dateien erhalten. Ein Remote-Dienst ist für die lokale Versionierung entbehrlich. Commit-Nachrichten und technische Behandlung von Wiederholungsversuchen bleiben der Umsetzung überlassen.

## Gesamtmanuskript und Export

Das ganze Manuskript lässt sich in Kapitelreihenfolge als durchgehender Text lesen und bearbeiten. Änderungen in dieser Ansicht werden den jeweiligen Szenendateien zugeordnet. Der Export erzeugt wahlweise eine einzelne Markdown-Datei oder ein formatiertes PDF. Beide enthalten die Kapitelüberschriften und sämtliche Manuskriptpassagen einschließlich der Konzepte an ihrer jeweiligen Stelle. Die Kennzeichnung „Konzept“ ist in beiden Ausgaben sichtbar.

## Technische Ansätze und Entscheidung

Gewählt wurde eine Rust-Desktop-App mit **GPUI und GPUI Kit**. Das Kit liefert Bausteine und Beispiele für Texteditor, Markdown-Darstellung und geteilte Ansichten. Die Timeline benötigt eine eigene Interaktion für Ereigniskarten und Verbindungen. Die aktive Weiterentwicklung des Frameworks ist akzeptiert. Das Schreibgefühl auf allen drei Desktopplattformen wurde für dieses Projekt noch nicht praktisch erprobt. [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [GPUI Kit](https://github.com/longbridge/gpui-kit)

Iced war die engste Alternative. Sein Canvas passt zur individuellen Timeline, und TextEditor sowie Markdown-Widgets decken die Grundlagen des Schreibfensters ab. Für GPUI Kit gaben die vorhandenen Editor-, Vorschau- und Layoutbeispiele den Ausschlag. egui wurde als weitere Rust-Option für eine stark grafische Arbeitsfläche erwogen. [Iced](https://iced.rs/) · [egui](https://github.com/emilk/egui)

Dem Bauplan bleiben die konkrete Dateisyntax, das Timeline-Layout, Editorbefehle, PDF-Erzeugung und die technische KI-Anbindung überlassen. Codex und ChatGPT wurden als Möglichkeiten genannt. Ein Anbieter ist noch nicht festgelegt. Das Projektformat soll die Wahl offenhalten. Automatische Übernahme von KI-Vorschlägen, eine Festlegung auf vollständige Kalenderdaten und ein mobiler Client gehören nicht zum Umfang der ersten Desktopfassung.
