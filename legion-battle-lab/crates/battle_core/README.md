# Simulationskern

`battle_core` ist ein reines Rust-Backend für bis zu **1.000.000 einzeln
gespeicherte Einheiten**. Es enthält weder Bevy noch
Grafik-, Eingabe- oder Fensterabhängigkeiten. `Battle::step()` berechnet genau
einen Schritt von 50 ms. Die Anwendung entscheidet nur, wann ein Schritt
angefordert wird; Kamera und Bildrate gehen nicht in die Kampfregeln ein.
Die logische Tickdauer ist eine Spielregel, keine Zusage, dass jede CPU einen
Millionen-Tick in 50 ms berechnet.

## Datenfluss

1. Lebende Einheiten werden in ein gleichmäßiges Raster mit getrennten
   Zellbereichen für beide Armeen eingetragen.
2. Zielwahl, Angriffe und Bewegungswünsche lesen denselben bisherigen Zustand.
   Große Schlachten verteilen diese Arbeit auf mehrere CPU-Threads.
3. Bewegungen werden in wiederverwendbaren Arbeitsarrays gesammelt. Gelände-
   und Brückenregeln begrenzen die resultierenden Positionen.
4. Alle Treffer werden pro Opfer addiert und anschließend gemeinsam angewendet.
5. Positionen, Lebenszustände, Verluste und Simulationszeit werden veröffentlicht.

Ein in diesem Tick getöteter Angreifer darf seinen bereits ausgelösten Angriff
noch ausführen. Beidseitige tödliche Treffer können daher ein Unentschieden
ergeben. Der letzte abgeschlossene Tick bleibt nach einem Ergebnis unverändert.

### Deterministische parallele Auswertung

Ab 65.536 Einheiten nutzt der Kern bei verfügbarer CPU-Parallelität mehrere
Worker, höchstens acht. Kleine Batches bleiben auf dem aufrufenden Thread.
Jeder Worker liest dieselben unveränderten Positionen, Geschwindigkeiten und
Einheiteneigenschaften und beschreibt ausschließlich seine eigenen
Ausgabebereiche. Blöcke von 4.096 IDs werden abwechselnd auf die Worker
verteilt, damit die aktiven Frontreihen beider Armeen mehrere Threads nutzen.
Rasteraufbau, räumliche Armeegrenzen und die abschließende Zustandsübernahme
bleiben seriell.

Jeder Angreifer speichert höchstens einen ausstehenden Treffer. Anschließend
addiert der Hauptthread diese Treffer immer nach aufsteigender Angreifer-ID.
Damit bleiben selbst die Reihenfolge der Fließkommaadditionen und simultane
tödliche Treffer unabhängig von der Threadplanung. Die Parallelisierung
ändert weder die LOD-Regeln noch Bewegungs- oder Schadensgleichungen. Ein Test
vergleicht den vollständigen beweglichen Zustand und die Kampfstatistik bei
einem und vier Workern mit gemeinsam beschossenen Zielen.

## Speicher und räumliche Abfragen

`UnitStore` speichert Eigenschaften in parallelen Arrays. Der Index ist für die
Dauer der Schlacht die stabile Einheiten-ID. Tote Datensätze bleiben für
Darstellung und Inspektion erhalten; sie werden nicht wiederverwendet. Im Raster
stehen nur lebende Einheiten. Rasterzählungen, Zellbereiche, Einträge,
Bewegungsarbeitsdaten und Schadensakkumulatoren behalten ihre Kapazität.
Raster-IDs, Zähler und Offsets sind 32-Bit-Werte. Kategorien sind zusätzliche
Eigenschaften derselben realen Einheiten; keine Kategorie wird durch einen
Sammelzähler oder einen Stellvertreter ersetzt.

`BattleStats::estimated_state_bytes` beziehungsweise `Battle::allocated_bytes()`
summiert die reservierten Nutzdatenkapazitäten der persistenten Arrays und
Raster. Der Wert enthält **keine** Speicherverwaltungskosten, temporären
Worker-Stacks und Auftragslisten, Render-Snapshots, Grafikpuffer oder den übrigen
Prozessspeicher. Der persistente Puffer ausstehender individueller Treffer
benötigt zusätzlich 8 Bytes pro angelegter Einheit.

Die Zielsuche untersucht höchstens 64 Kandidaten je Einheit und Tick. Die
Separation untersucht höchstens 24 Kandidaten je Armee. Auch eine einzelne
überfüllte Zelle wird nur begrenzt abgetastet, statt vor der Auswahl die gesamte
Zelle zu durchsuchen. `neighbor_checks` und `max_cell_occupancy` zeigen die
tatsächlich aufgetretene Arbeit und Dichte.

Eine Dichteabfrage vor der Einheit und die freie Strecke zu benachbarten Körpern
begrenzen die Vorwärtsgeschwindigkeit. Dadurch bildet sich vor der Brücke ein
Rückstau; lokale Separation allein würde die nachrückenden Reihen durch die
Front schieben und unrealistisch große Zellbelegungen erzeugen.

Das ist ein bewusst vereinfachtes Crowd-Modell: Es garantiert keine exakte
Kollisionsfreiheit, keine mathematisch vollständige Nächster-Nachbar-Suche und
keine physikalische Impulserhaltung. Dichtes Gedränge darf leicht überlappen.

## Simulations-LOD für große Szenarien

Oberhalb von 20.000 Einheiten kommt zusätzlich ein grobes 16-m-Raster zum
Einsatz. Eine Summenfeldtabelle pro Armee beantwortet die Frage nach Gegnern
in einem Rechteck mit vier Speicherzugriffen. Das Raster wird aus allen
lebenden Einheiten pro Tick erneuert; es hängt nie von der Kamera ab.
Zusätzlich verwirft die konservative räumliche Ausdehnung der Gegner eindeutig
ferne Einheiten, damit die groben Zellen nicht ganze Hinterreihen unnötig in
den teuren Bereich versetzen.

- **Frontbereich:** Sobald die konservative Rasterabfrage einen Gegner innerhalb
  der eigenen Reichweite plus 4 m findet, erhält die Einheit die volle lokale
  Zielsuche, Einzelangriffe und lokale Separation. Rasterzellen können auch
  Einheiten etwas früher aktivieren; echte Treffer prüfen weiterhin den
  individuellen räumlichen Abstand.
- **Strategischer Bereich:** Die Einheit behält Position, Geschwindigkeit,
  Gesundheit und ID. Ihre Position wird **jeden Tick** integriert. Die teurere
  Steuerungsrichtung wird nach stabiler ID gestaffelt nur alle acht Ticks
  erneuert. Breite Marschbahnen zielen auf den nächstgelegenen Punkt der
  gegnerischen Ausdehnung; die Brückenroute gilt weiterhin. Zellbelegung bremst
  Gedränge. In diesem Bereich entfällt individuelle Separation.

Das feine 3-m-Raster enthält weiterhin alle lebenden Einzelzustände. Deshalb
bleiben Ziele, Treffer und Todesfälle individuell und simultan. Einheiten
werden nicht anhand eines sichtbaren Ausschnitts erzeugt oder pausiert.
`full_detail_units` und `strategic_units` zählen die zu Tickbeginn lebenden,
in diesem Tick in den jeweiligen Stufen bearbeiteten Einheiten.

Die Grundarbeit bleibt linear in der Einheitenzahl und den Rasterzellen.
Teure Kandidatenbesuche fallen vor allem an der Front an. Der ungünstigste
Fall — etwa eine Million eng vermischte Feinde — kann alle Einheiten in den
Vollbereich versetzen. Auch dann bleibt die Kandidatenzahl je Einheit begrenzt;
der Lauf kann jedoch deutlich langsamer als Echtzeit sein. Diese LOD ist eine
präzise beschriebene Verhaltensvereinfachung, keine Gleichheitsgarantie gegenüber
einer Million Einheiten mit permanent vollständiger Nahbereichsphysik.

## Gelände und Waffen

- Offene Ebene: freie Bewegung, kleine Höhenvariationen.
- Hügeltal: höhenabhängige Darstellung und vereinfachte Verlangsamung.
- Flussübergang: Wasser bei `|x| < 5.2`, passierbare Brücke bei `|z| <= 5.5`.
  Eine kleine Portalroute führt die Armeen zum Übergang; das ist noch kein
  allgemeiner Navigationmesh- oder Flow-Field-Algorithmus.

Infanterie, schwere Garde und Fernkämpfer besitzen gemeinsame Typdefinitionen.
Fernkampfschaden wird nach Reichweitenprüfung direkt angewendet. Es existieren
in diesem Backend noch keine physischen Projektile, ballistischen Flugbahnen,
Rüstungszonen oder Sichtlinienprüfungen. `attack_flash` ist ein Darstellungsimpuls,
kein zusätzlicher Gameplayzustand, der die Schadensberechnung beeinflusst.

## Sieben Kategorien

`ArmyConfig.category` bestimmt die Kategorie. `ArmyConfig.kind` bleibt das
kompatible Profil Infanterie, schwere Garde oder Fernkämpfer. Alte JSON-Dateien
ohne `category` laden als `Creature`. Kombinierte Werte liefert
`UnitCategory::definition(UnitKind)`; die ursprünglichen
`UnitKind::definition()`-Werte beschreiben weiterhin Creature-Einheiten.

| Kategorie | Höhe | HP mit Infantry | Geschwindigkeit | Reichweite | Bewegung |
|---|---:|---:|---:|---:|---|
| Creature | 0 m | 100 | 3,8 m/s | 1,65 m | Boden / Brücke |
| Vehicle | 0 m | 400 | 5,7 m/s | 12 m | Boden / Brücke |
| Building | 0 m | 1.000 | 0 | 20 m | Stationäres Geschütz |
| Air Unit | 8 m | 150 | 9,12 m/s | 16 m | Freie Ebene |
| Sea Unit | 0 m | 600 | 3,23 m/s | 18 m | Vereinfachter ebener Proxy |
| Underground Unit | −3 m | 200 | 2,85 m/s | 5 m | Freie Ebene |
| Space Unit | 32 m | 300 | 11,4 m/s | 40 m | Freie Ebene |

Die Höhe liefert `category.elevation()`. Angriffsabstände berücksichtigen
Grundebenenabstand und Höhendifferenz: Boden-Nahkampf trifft kein 32 m hohes
Raumschiff. Der Raumangriff besitzt genügend Reichweite für Bodenziele.
Einheiten auf verschiedenen Höhen verdrängen sich nicht durch Nahseparation.
Gebäude bleiben auch bei Kontakt exakt stationär.

Diese erste Iteration simuliert gemeinsame 2.5D-Sandbox-Domänen: Sea Unit ist
noch **nicht auf Wasserflächen beschränkt**, Space Unit fliegt keine Umlaufbahn,
Air Unit hat keine Aerodynamik und Underground Unit gräbt keine Tunnel.
Freie Kategorien umgehen die Bodensperre des Flusses. Treffer prüfen keine
Sichtlinien, Abschirmung durch Erde oder separate Anti-Air-/Anti-Space-Waffen.

## Szenarien und Wiederholbarkeit

`ScenarioConfig::from_json()` validiert die Konfiguration. Beide Armeen müssen
mindestens eine Einheit enthalten, insgesamt sind höchstens 1.000.000 erlaubt.
Konfigurationen lassen sich als JSON speichern. Das JSON ist kein Snapshot
einer laufenden Schlacht.

Ein fester Seed erzeugt dieselben Startpositionen und Zufallswerte in derselben
freigegebenen Build-/Plattformkonfiguration. Der Zufall wird aus Seed, ID und
Tick abgeleitet. `step_ms` misst Wandzeit und hat keinen Einfluss auf den
Spielzustand. Plattformübergreifende bitgleiche Fließkommaergebnisse werden
nicht zugesichert.

## Prüfung und Messung

Im Projektverzeichnis:

```sh
cargo test -p battle_core
cargo run --release -p battle_core --bin battle-bench -- \
  --units 1200 --ticks 1200 --seed 42 --map meadow
cargo run --release -p battle_core --bin battle-bench -- \
  --units 10000 --ticks 1200 --map crossing --output benchmark.json
cargo run --release -p battle_core --bin battle-bench -- \
  --units 1000000 --ticks 300 --seed 2026 --map meadow --output million.json
cargo run --release -p battle_core --bin battle-bench -- \
  --units 2000 --ticks 300 --blue-category vehicle --red-category air_unit
```

`--units` ist die Gesamtzahl beider Armeen. Das Programm teilt diese Zahl nahezu
gleichmäßig auf. Es beendet die Simulation bei einem Schlachtergebnis oder nach
der angeforderten Tickzahl. Der Bericht nennt tatsächlich berechnete Ticks,
Verluste, p50-/p95-/Maximalzeiten, höchste Zellbelegung, Aktivitätsstufen,
Initialisierungszeit, Buildprofil und geschätzte persistente Backend-Bytes.

Nach der Zeitmessung prüft das Programm den vollständigen Endzustand auf
endliche Zahlen, konsistente Arraylängen und Lebenszustände sowie die
Erhaltung des Anfangsbestands aus Überlebenden und Verlusten pro Armee.
Bei einem Fehler beendet es sich mit einer konkreten Diagnose; ein erfolgreicher
JSON-Bericht enthält `state_valid: true`. Diese Abschlussprüfung verändert die
gemessenen Tickzeiten nicht.

Diese Messung enthält keine Darstellung. Die Millionengrenze ist ein
Kapazitätslimit des Backends und keine garantierte Bildrate. Die spätere
20-Millionen-Anforderung benötigt einen eigenen GPU-Simulationspfad samt
Hardware-, Dichte- und Darstellungsbudget.
