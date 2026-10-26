# Kategorienbeispiele

Diese sieben kleinen Szenarien zeigen jeweils **64 gegen 64 Einheiten** derselben Kategorie auf der offenen Ebene. Das Waffenprofil `infantry` und die Linienformation bleiben gleich, damit die Kategorie den Vergleich bestimmt. Die Kategorie verändert Lebenspunkte, Geschwindigkeit, Reichweite, Schaden, Radius und gegebenenfalls die Kampfhöhe.

Aus dem Projektverzeichnis starten:

```sh
cargo run --release -p legion -- --quick --scenario scenarios/categories/vehicle.json
```

Für einen automatischen nativen Rendercheck:

```sh
cargo run --release -p legion -- --render-check --scenario scenarios/categories/vehicle.json --capture-dir captures/vehicle
```

Der Rendercheck führt mindestens zwei Simulationsschritte aus, pausiert und prüft die tatsächlichen Einheiten- und Renderdaten. Er schreibt `render-check.json` und `render-check.png`. Dies prüft den gewählten Startzustand und seine Darstellung; ein vollständiger Gefechts- oder Leistungstest ist separat nötig.

| Datei | Kategorie | Verhalten in dieser Iteration |
|---|---|---|
| `creature.json` | Creature | Bewegliche Bodeneinheiten; humanoides Modell. |
| `vehicle.json` | Vehicle | Bewegliche Bodeneinheiten; Fahrzeugmodell. |
| `building.json` | Building | Stationäre Einheiten; Gebäudemodell. |
| `air_unit.json` | Air Unit | Bewegung auf der Ebene bei fester Kampfhöhe von 8 m; Flugzeugmodell. |
| `sea_unit.json` | Sea Unit | Beweglicher Marine-Platzhalter mit Bootsmodell; freie Bewegung auch über Land. |
| `underground_unit.json` | Underground Unit | Feste Kampfhöhe von −3 m; ein sichtbarer Oberflächenmarker kennzeichnet die Position. |
| `space_unit.json` | Space Unit | Bewegung auf der Ebene bei fester Kampfhöhe von 32 m; Raumschiffmodell. |

Air, Sea, Underground und Space ignorieren die Hindernisse des Bodenpfads. Flug-, Wasser-, Tunnel- und Orbitalphysik sind noch nicht implementiert. Höhenunterschiede werden bei der Angriffsreichweite berücksichtigt. Die Szenarien zeigen bewusst die Sandbox-Darstellung auf derselben Karte.
