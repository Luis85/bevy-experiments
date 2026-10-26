# Gestaltung der Oberfläche

Die Oberfläche unterstützt Aufstellen, Starten und Beobachten zweier Armeen. Anzahl, Kategorie und Zeitstatus müssen verständlich sein. Eine Schlacht ist direkt vom Hauptmenü erreichbar; große Szenarien erhalten erkennbare Vorbereitungszustände.

## Gestaltungsregeln

- Dunkler Hintergrund: #10191E; Panels: #1B282F; hervorgehobene Bedienelemente: #2A3C44.
- Heller Text: #F2EEE4; sekundärer Text: #B9C9CA; warmer Aktionsakzent: #E6AA64.
- Blaue Armee: #55B6D1; rote Armee: #DC765A. Namen und Zahlen ergänzen die Farbcodierung.
- Abstände orientieren sich an 4 / 8 / 12 / 16 / 24 / 32 / 48 logischen Pixeln.
- Mitgelieferte Barlow-Schriften, lesbare Hilfstexte und größere Überschriften.
- Zurückhaltende Rundungen und unterscheidbare Zustände für Hover, Auswahl, Tastaturfokus und deaktivierte Aktionen.
- Deutsche Anweisungen mit den sieben vom Produkt vorgegebenen englischen Kategorienamen.

## Ansichten

| Ansicht | Inhalt und Aufgabe |
|---|---|
| Hauptmenü | Titel und primäre Spielaktion, gerenderte Welt, Konfiguration, Steuerung und Beenden |
| Konfiguration | Fünf Vorlagen, Gelände, zwei Armee-Editoren, Kategorien und Profile, Formation, Anzahl und Schrittweite, Seed, Zeitlimit, Speichern/Laden und Start |
| Schlacht | Überlebende und Verluste beider Armeen, Spielzeit, Zeitsteuerung und Navigation; möglichst viel sichtbare Welt |
| Vorbereitung | Benannte Hintergrundarbeit; letzte fertige Welt darf sichtbar bleiben; Menü und Hilfe erreichbar |
| Pause | Laufenden Tick von vollständig angehaltener Simulation unterscheiden; Einzelschritt nur bereit |
| Ergebnis | Ergebnis, Dauer und Überlebende; Wiederholung und Szenariowechsel |
| Fehler | Verständlicher Status und erreichbarer Neustart beziehungsweise Szenariowechsel |
| Hilfe | Modaler Dialog mit gehaltenem Tastaturfokus und blockierten Welteingaben |

## Große Populationen verständlich machen

Die Konfiguration zeigt die Gesamtgrenze von einer Million und bietet Schrittweiten 1, 100, 1k, 10k und 100k. Die Grenze gilt gemeinsam für beide Armeen. Kategorie und Kampfprofil sind getrennte Entscheidungen; resultierende Werte stehen direkt bei der Armee. Unzureichende Reichweite bei verschiedenen Kategoriehöhen wird vor dem Start benannt.

Die Übersicht verwendet einen eingereichten Punkt pro lebender Einheit. Nahe Einheiten erhalten bis zu 6.000 zusätzliche Modelle. Die Hilfe erklärt diesen Wechsel; die Diagnose trennt Population, Punkte und Detailmodelle. Die Zähler sind keine Zahl sichtbarer Pixel und keine garantierte Bildrate.

Die Geschwindigkeitsanzeige benennt ein **Zieltempo**. CPU-Tickzeit, Simulations-/Render-Tick und nicht nachgeholte Zeit machen Überlast nachvollziehbar. Fachliche Details stehen in der optionalen F3-Diagnose, der normale Ablauf verwendet verständliche Statusmeldungen.

## Eingaben und Zugänglichkeit

Menüs sind per Tab, Umschalt+Tab und Enter bedienbar. Deaktivierte Aktionen sind nicht Teil der Tab-Reihenfolge. Schrittweiten bilden eine zusammenhängende Tastaturauswahl mit Pfeiltasten und Home/End. Auswahl und Fokus sind unterscheidbar.

UI-Tastaturfokus unterdrückt Kamerabefehle und normale Schlachtkürzel; Escape bleibt ein Weg zum Pausieren oder Schließen der Hilfe. Der HUD-Hinweis erklärt, dass ein Klick ins Schlachtfeld WASD und Leertaste wieder freigibt. Mausbewegung und Mausrad über UI-Flächen steuern die Kamera nicht.

Das Öffnen der Hilfe pausiert die Schlacht nach dem aktuellen Tick. Schließen lässt sie pausiert; der Hilfetext erklärt die getrennten Schritte Schließen und Fortsetzen. Der Dialog blockiert Tastatur- und Mauseingaben an die Welt. Zugängliche Namen unterscheiden gleich beschriftete Aktionen nach Armee und Funktion; Auswahl und deaktivierter Zustand werden zusätzlich semantisch bereitgestellt.

Mindestgröße ist 1024 × 768, Standard 1440 × 900. Die Prüfung umfasst lange Kategorienamen, eine Million Einheiten, große Seed-Werte, Statusmeldungen, Hilfe sowie Diagnose und Ergebnis bei beiden Größen. Ein vollständiger Screenreader- oder Kontrastnorm-Nachweis ist mit diesen Regeln allein nicht erbracht.

## Verifikation und Politur

Der native Smoke-Test aktiviert echte UI-Aktionen, wartet auf asynchrone Übergänge und schreibt Screenshots der wesentlichen Ansichten. Separate Renderprüfungen kontrollieren die ausgewählte Population und die GPU-Darstellung. Physische Maus-/Tastatureingaben und sichtbare Layoutprüfung ergänzen die Observer-Aktivierungen.

Der abschließende Review betrachtet Funktionsumfang, Spielverständnis, Zustände und Fehler, Bedienbarkeit und Hierarchie, Skalierung und Diagnose sowie Einrichtung und Dokumentation. Befunde werden behoben und gezielt erneut geprüft. Befunde und Korrekturen stehen in [REVIEW.md](REVIEW.md); tatsächliche Ergebnisse und verbleibende Prüfgrenzen in [TEST_RESULTS.md](TEST_RESULTS.md).
