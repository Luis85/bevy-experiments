# Produktrahmen

## Ziel

**Legion — Battle Lab** ist eine lokale native Schlachtensandbox in Rust und Bevy. Nutzer stellen zwei Armeen auf, starten eine beobachtbare Schlacht und vergleichen Ergebnisse unter wiederholbaren Startbedingungen. Die erste Iteration umfasst Menü, Szenariokonfiguration, simulierte Welt, Kamera und Ergebnisanzeige.

Das Kapazitätsziel sind **bis zu 1.000.000 individuelle Einheiten insgesamt**. Das langfristige Ziel sind 20 Millionen. Die erste Grenze wird im Szenariomodell und in der UI durchgesetzt; sie ist keine Echtzeit- oder Bildratenzusage. Tatsächliche Nachweise stehen in [TEST_RESULTS.md](TEST_RESULTS.md).

## Funktionsumfang

- Native Desktopanwendung mit Maus und Tastatur; deutschsprachige Oberfläche und Dokumentation.
- Sieben Kategorien mit kanonischen Namen: Creature, Vehicle, Building, Air Unit, Sea Unit, Underground Unit, Space Unit.
- Eine Kategorie, ein Kampfprofil und eine Startformation pro Armee; noch keine gemischten Verbände.
- Fünf Vorlagen einschließlich 500.000 gegen 500.000, drei Gelände, konfigurierbare Anzahl, Seed und Zeitlimit.
- Eigene Position, Gesundheit, Kategorie und Lebenszustand jeder Einheit; individuelle Angriffe und Verluste an der Kontaktfront.
- Orbitkamera, Pause, Einzelschritt, vier Zielgeschwindigkeiten, Neustart und Ergebnis.
- Speichern/Laden von Startkonfigurationen als JSON; kein Savegame einer laufenden Schlacht.
- Sichtbare Zustände für Vorbereitung, laufende Berechnung, Pause, Ergebnis und abfangbare Simulationsfehler.
- Diagnoseanzeige, Headless-Benchmark und native UI-/Renderprüfungen für nachvollziehbare Aussagen.

## Entscheidungen für die erste Iteration

Die maßgebliche Simulation bleibt ein überprüfbarer CPU-Kern mit kompakten parallelen Arrays. Ein eigener Worker und unveränderliche Snapshots trennen den Anwendungsablauf von langen Ticks. Ferne Einheiten verwenden vereinfachte Aktivität, ihre individuellen Positionen werden weiterhin jeden Tick integriert.

Die GPU erhält einen Punkt je lebender Einheit und zusätzliche Kategorienmodelle für höchstens 6.000 nahe Einheiten. Die Übersicht wird so vollständig eingereicht, während Modellkosten begrenzt bleiben. Der Kameraausschnitt beeinflusst keine Kampfregeln. Prozedurale Modelle, mitgelieferte Fonts und Shader vermeiden zusätzliche Asset-Downloads.

Die Kategorien sind zunächst gemeinsame 2.5D-Sandbox-Domänen. Sea Units bewegen sich auf der Ebene frei; Air und Space verwenden feste Kategoriehöhen; Underground Units erhalten sichtbare Oberflächenmarker. Gebäude sind stationär. Echte Wassernavigation, Flug-/Orbitalphysik, Tunnel und Sichtlinien sind noch keine Produktzusagen.

## Qualitätsziele und Review

Der Ablauf muss vom ersten Start über Konfiguration, Vorbereitung, Schlacht und Ergebnis bis zum Neustart verständlich bleiben. Pause und Einzelschritt müssen zwischen angeforderter und abgeschlossener Arbeit unterscheiden. Große Szenarien dürfen die UI nicht durch synchron ausgeführte Kampfticks blockieren.

Die Produktprüfung betrachtet Spielverhalten und Größenlimit, Verständlichkeit und Bedienbarkeit, Tastaturfokus und Fenstergrößen, Fehlererholung und Persistenz, Rendering und Messgrenzen sowie Einrichtung und Weitergabe. Der abschließende Review verbindet konkrete Befunde mit Korrekturen und anschließender Verifikation; [REVIEW.md](REVIEW.md) hält diese Entscheidungen fest. Testergebnisse sind keine Leistungsfreigabe für jede Hardware.

## Weitere Entscheidungen

Zielhardware, garantierte Leistung, gemischte Armeen, vollständige Bewegungsdomänen, Karteneditor, Multiplayer, Replays und endgültige Art Direction bleiben Folgeentscheidungen. Vor dem Ausbau auf 20 Millionen müssen Simulationsqualität, sichtbarer Detailgrad, Transferbudget und Mindestleistung festgelegt werden.
