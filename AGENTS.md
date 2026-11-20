# Arbeitsweise für die Schreibwerkstatt

## Umsetzungsaufträge

- Lege jeden Umsetzungsauftrag zu Beginn als Goal im aktuellen Chat an. Formuliere das angestrebte Ergebnis und markiere das Goal nach erfolgreicher Prüfung als abgeschlossen.

## Skills und Delegation

- Lade für jede Rust-Aufgabe die einschlägigen Rust-Skills. Nutze beispielsweise `idiomatic-rust` für neue Rust-Implementierungen und `rust-testing` für Rust-Tests.
- Nutze `delegate-work-efficiently` für Recherche, Analyse, Planung und Dokumentation sowie `delegate-coding-efficiently` für Implementierung, Fehlersuche, Refactoring und technische Prüfungen. Delegiere klar abgegrenzte, separat prüfbare Arbeitspakete, wenn der Nutzen den Koordinationsaufwand rechtfertigt. Der Hauptagent prüft die Ergebnisse und führt sie zusammen.

## Entwicklung und Versionen

- Verwende für Cargo-Abhängigkeiten explizite Caret-Versionsanforderungen wie `^1.2.3`, damit kompatible Aktualisierungen möglich bleiben.

- Nutze `test-driven-development` für Features, Fehlerbehebungen, Refactorings und Verhaltensänderungen. Schreibe zuerst einen Test und prüfe, dass er aus dem erwarteten Grund fehlschlägt. Implementiere die kleinste passende Änderung, prüfe den erfolgreichen Testlauf und räume bei Bedarf auf. Führe vor dem Abschluss die vollständige Testsuite aus und benenne etwaige Fehlschläge.
- Verwende die jeweils neuesten stabilen Versionen der Entwicklungswerkzeuge, der Rust-Toolchain und der Abhängigkeiten. Prüfe die verfügbaren Versionen beim Einrichten und bei Aktualisierungen anhand der offiziellen Veröffentlichungen oder Paketregister. Prüfe nach Aktualisierungen den Build und die Tests.

## Git

- Arbeite direkt auf `main`. Committe regelmäßig in kleinen, zusammenhängenden und geprüften Schritten und pushe diese Commits auf `origin/main`.
- Nenne den beteiligten Agenten in jedem von ihm erstellten Commit als Co-Autor. Verwende dafür den Git-Trailer `Co-authored-by: Name <E-Mail>`. Für Codex lautet er `Co-authored-by: Codex <codex@openai.com>`.
- Führe bei Pull Requests die beteiligten Agenten auch in der PR-Beschreibung als Co-Autoren auf.
