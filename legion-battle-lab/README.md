# Legion — Battle Lab

Eine native Schlachtensimulation in **Rust und Bevy 0.20.0** mit Spielmenü, Szenariokonfiguration, prozeduraler 3D-Welt und zwei tatsächlich kämpfenden Armeen.

**Die erste Iteration erlaubt bis zu 1.000.000 Einheiten insgesamt.** Jede Einheit besitzt einen eigenen Zustand und eine stabile ID. Sieben Kategorien sind konfigurierbar: Creature, Vehicle, Building, Air Unit, Sea Unit, Underground Unit und Space Unit. Das Standardszenario beginnt mit 600 gegen 600; die Vorlage **1 Million** stellt 500.000 gegen 500.000 auf.

Das CPU-Backend verwendet abgestufte Aktivität und eine getrennte GPU-Darstellung. **Eine Million gespeicherte und dargestellte Einheiten ist kein Versprechen von 20 Echtzeit-Ticks oder 60 FPS.** Die logische Tickdauer beträgt immer 50 ms; bei hoher Last läuft die Spielzeit langsamer als die Echtzeit. Ausgeführte Prüfungen, Messwerte und Grenzen stehen in [TEST_RESULTS.md](TEST_RESULTS.md); Befunde und Korrekturen des abschließenden Produktreviews in [REVIEW.md](REVIEW.md). Das langfristige Ziel von 20 Millionen ist ein späterer Entwicklungsschritt.

## Starten

Voraussetzungen sind Rust, ein Desktop mit Maus und Tastatur sowie ein geeigneter Grafiktreiber. `rust-toolchain.toml` pinnt **Rust 1.99.1** einschließlich rustfmt und Clippy. Bei einer rustup-Installation wird diese Toolchain beim ersten Build bereitgestellt. Das Projekt nutzt Rust Edition 2024 und eine gesperrte Abhängigkeitsliste in `Cargo.lock`.

```bash
# Im entpackten Ordner legion-battle-lab:
cargo run --locked --release -p legion
```

Alternativ startet `run.bat` unter Windows beziehungsweise `./run.sh` unter Linux/macOS den Release-Build im richtigen Projektordner. Der erste Bevy-Build lädt viele Abhängigkeiten und benötigt Zeit sowie freien Speicher. Für Änderungen am Code kann `cargo run --locked -p legion` verwendet werden; Leistungsmessungen sollten mit `--release` erfolgen. Die Anwendung benötigt nach dem Build weder einen Server noch heruntergeladene Spielinhalte.

Unter Windows werden die MSVC-C++-Buildtools benötigt; unter macOS die Xcode Command Line Tools. Unter Linux verwendet diese Konfiguration X11 und benötigt xkbcommon sowie einen funktionierenden Vulkan-Treiber. Beispiel für Entwicklungsbibliotheken unter Ubuntu/Debian:

```bash
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev libxkbcommon-x11-0 libvulkan1
```

Der Grafiktreiber muss separat zur Hardware passen. Das Projekt aktiviert keine Audio- oder Gamepad-Abhängigkeiten. Die tatsächlich geprüfte Plattform steht in [TEST_RESULTS.md](TEST_RESULTS.md); ungeprüfte Betriebssysteme sind dadurch nicht als freigegeben anzusehen. Die offiziellen [Bevy-Setup-Hinweise](https://bevy.org/learn/quick-start/getting-started/setup/) und die [Rust-Installation](https://www.rust-lang.org/tools/install) enthalten weitere Plattformdetails.

## Spielablauf

1. **Sofort spielen** startet das aktuell gewählte Szenario.
2. **Szenario konfigurieren** bietet fünf Vorlagen, drei Gelände, beide Armeen, Kategorien, Kampfprofile, Formationen, Anzahl, Seed und Zeitlimit.
3. Die Anzahl lässt sich mit den Schrittweiten **1 / 100 / 1k / 10k / 100k** verändern. Die Gesamtgrenze von einer Million gilt für beide Armeen zusammen.
4. **Schlacht starten** legt die Welt im Hintergrund an. Vorbereitung und Aufbau der Darstellung werden angezeigt; Menü und Hilfe bleiben erreichbar.
5. **Pause**, **Einzelschritt**, vier Zielgeschwindigkeiten und **Neustart** erlauben die Beobachtung. Eine Pause beendet einen bereits laufenden Tick und hält danach an. Einzelschritt berechnet genau einen weiteren Tick.
6. Nach Sieg, gegenseitiger Vernichtung oder Zeitablauf erscheint das Ergebnis. Ein Neustart verwendet dieselbe Startkonfiguration und denselben Seed.

Die Konfiguration legt derzeit **eine Kategorie und ein Profil pro Armee** fest. Gemischte Verbände innerhalb derselben Armee benötigen eine spätere Erweiterung des Szenariomodells.

### Sieben Kategorien

| Kategorie | Verhalten dieser Iteration | Darstellung |
|---|---|---|
| Creature | Bodenbewegung und Brückenroute | Humanoide Figur |
| Vehicle | Bewegliche Bodeneinheit mit höheren Basiswerten | Fahrzeug |
| Building | Stationäre Einheit mit Fernangriff | Geschützgebäude |
| Air Unit | Freie Bewegung auf einer Ebene 8 m über dem Gelände | Fluggerät |
| Sea Unit | Vereinfachte ebene Bewegung, nicht auf Wasser beschränkt | Boot |
| Underground Unit | Freie Bewegung mit logischer Kategoriehöhe −3 m | Sichtbarer Marker an der Oberfläche |
| Space Unit | Freie Bewegung auf einer Ebene 32 m über dem Gelände | Raumschiff |

Die Kategorie verändert Lebenspunkte, Geschwindigkeit, Reichweite, Schaden und Körperradius. **Ausgewogen**, **Schwer** und **Fernkampf** sind zusätzliche Kampfprofile. Die Konfiguration zeigt die kombinierten Werte und weist darauf hin, wenn eine Armee die Höhe des Gegners mit ihrem Profil nicht erreichen kann. Angriffsreichweiten berücksichtigen die Höhendifferenz der Kategorien.

Die Kategorien bilden eine gemeinsame 2.5D-Sandbox. Es gibt noch keine Wasserbeschränkung, Aerodynamik, Tunnel, Erdabschirmung oder Umlaufbahnen. Air Unit, Sea Unit, Underground Unit und Space Unit umgehen die Bodensperre des Flusses. Fernkampfschaden wird nach der Reichweitenprüfung direkt angewendet; physische Geschosse und Sichtlinienprüfungen sind noch nicht enthalten.

### Gelände und Aufstellung

| Gelände | Verhalten |
|---|---|
| Offene Ebene | Freie Bewegung und breite Fronten |
| Hügeltal | Höhenvariationen und vereinfachte Verlangsamung |
| Flussübergang | Creature und Vehicle verwenden die mittlere Brücke |

Linie, Keil und Block bestimmen die **Startaufstellung**. Eine anhaltende Formationssteuerung ist noch nicht implementiert. Stationäre Gebäude oder gegenseitig unerreichbare Profile können bis zum Zeitlimit ohne Entscheidung bleiben.

## Steuerung

| Eingabe | Aktion |
|---|---|
| W/A/S/D oder Pfeiltasten | Kamera verschieben |
| Rechte Maustaste ziehen | Kamera drehen und neigen |
| Mausrad | Zoomen |
| Q / E | Kamera horizontal drehen |
| Shift | Schnellere Kamerabewegung |
| R oder Home | Kamera zurücksetzen |
| Leertaste | Pause/Fortsetzen bei freiem Tastaturfokus |
| Punkt | Ein Tick bei pausierter, bereiter Schlacht |
| 1 / 2 / 3 / 4 | Zielgeschwindigkeit 0,5× / 1× / 2× / 4× |
| Escape | Hilfe schließen; in der Schlacht Pause umschalten; Konfiguration verlassen |
| F1 | Hilfe öffnen/schließen; Öffnen pausiert die Schlacht |
| F3 | Diagnoseanzeige ein-/ausblenden |
| Tab / Umschalt+Tab / Enter | Menüs per Tastatur bedienen |
| Pfeiltasten / Home / End auf der Schrittweite | Schrittweite der Anzahl auswählen |

Die unterstützte Fenstergröße beginnt bei **1024 × 768**; Standard ist **1440 × 900**. Fokussierte UI-Elemente erhalten Tastatureingaben statt der Kamera. **Ein Klick ins freie Schlachtfeld gibt WASD und Leertaste wieder frei.** Mausrad und Kameraziehen wirken über UI-Flächen nicht auf die Welt. Escape bleibt zum Pausieren auch bei fokussierter UI verfügbar.

**Das Öffnen der Hilfe pausiert die Schlacht nach dem gerade laufenden Tick.** Der Dialog hält den Fokus und blockiert Welteingaben. Schließen setzt die Schlacht nicht automatisch fort: Escape schließt zunächst die Hilfe; ein weiteres Escape oder **Fortsetzen** startet die Simulation wieder.

## Szenarien speichern und laden

Die Schaltflächen speichern und laden standardmäßig `user-data/scenario.json`. Fehler erscheinen in der Konfiguration. Geladen werden reguläre Dateien bis **64 KiB**; auch die tatsächlich gelesene Datenmenge ist begrenzt. Wertebereiche, UTF-8 und unbekannte JSON-Felder werden geprüft.

Speichern erzeugt exklusiv eine eindeutig benannte temporäre Datei im selben Verzeichnis, schreibt und synchronisiert sie, schließt sie und ersetzt anschließend die Zieldatei durch Umbenennen. Bei einem Fehler wird die bisherige Zieldatei nicht als Ausweichlösung gelöscht; nur die eigene temporäre Datei wird aufgeräumt.

**Ein Szenario speichert die Startkonfiguration, keinen laufenden Spielstand.** Aktuelle Positionen, Verluste und verstrichene Zeit werden nicht gespeichert. Alte JSON-Dateien ohne `category` laden als Creature. In JSON heißen die Kategorien `creature`, `vehicle`, `building`, `air_unit`, `sea_unit`, `underground_unit` und `space_unit`.

Fünf Vorlagen liegen unter `scenarios/`. Die Millionenfront direkt starten:

```bash
cargo run --locked --release -p legion -- --scenario scenarios/millionenfront.json --quick
```

Weitere Startoptionen:

```bash
cargo run --locked -p legion -- --help
cargo run --locked -p legion -- --width 1280 --height 800
cargo run --locked -p legion -- --data-dir ./meine-szenarien
```

Relative Pfade beziehen sich auf das Arbeitsverzeichnis. Fonts und Shader befinden sich unter `assets`. Beim Start prüft die Anwendung, ob `shaders/crowd.wesl` sowie **beide** Barlow-Schriften vorhanden sind, und nennt fehlende Assets verständlich. Beim Weitergeben einer kompilierten ausführbaren Datei muss der vollständige Ordner `assets` neben ihr liegen oder vom Arbeitsverzeichnis aus erreichbar sein.

## Systemdesign

| Bereich | Ort | Verantwortung |
|---|---|---|
| Fachmodell und Simulation | `crates/battle_core` | Validierung, Einzelzustände, Raster, Bewegung, Kampf und Ergebnis |
| Simulationsworker | `crates/legion/src/runtime.rs` | Eigentümer der Battle-Instanz, Tickaufträge und unveränderliche Snapshots |
| Anwendungsablauf | `crates/legion/src/app.rs` | Bildschirme, Befehle, Zeittakt, Status und Szenario-IO |
| Benutzeroberfläche | `crates/legion/src/ui.rs` | Native Bevy-Widgets, Konfiguration, HUD, Hilfe und Diagnose |
| Welt und Darstellungsbrücke | `crates/legion/src/world.rs`, `world/` | Terrain, sieben Modelle, Hintergrundaufbereitung und Detailauswahl |
| Crowd-Renderer | `crates/legion/src/crowd.rs`, `assets/shaders/` | GPU-Instanzen, Interpolation, Animation und persistente Buffer |
| Kamera | `crates/legion/src/camera.rs` | Darstellung und Eingaben ohne Einfluss auf Kampfregeln |
| Native Prüfungen | `crates/legion/src/smoke.rs` | UI-Aktivierungen, Zustandsprüfungen, Renderprüfung und Screenshots |

Der reine Rust-Kern verwendet kompakte parallele Arrays mit stabilen IDs. Ab großen Populationen verteilt er lokale Arbeit auf CPU-Threads; Treffer werden in stabiler Reihenfolge gesammelt und gemeinsam angewendet. Oberhalb von 20.000 Einheiten erhält die Kontaktfront volle lokale Suche, während entfernte Einheiten ihre teurere Steuerung seltener aktualisieren. Ihre Positionen werden weiterhin jeden Tick integriert. Die Aktivitätsstufe hängt von Gegnernähe ab, niemals von der Kamera.

Ein eigener Worker berechnet die Simulation. Die Bevy-Anwendung erhält unveränderliche Snapshots und bleibt während eines langsamen Ticks bedienbar. Generationsnummern verhindern, dass alte Ergebnisse ein neu gestartetes Szenario ersetzen. Tickaufträge und die Aufbereitung der Darstellung werden begrenzt, sodass keine unbegrenzte Warteschlange großer Snapshots entsteht.

Die Darstellung erzeugt **keine Bevy-Entity pro Einheit**. Jede lebende Einheit wird als eigener GPU-Punkt eingereicht. Bis zu **6.000** nahe Einheiten erhalten zusätzlich ein Kategorienmodell. Die Punkte bleiben darunter bestehen; diese doppelte Darstellung zählt in der Diagnose nicht als zusätzliche Einheit. Sichtbarkeit hängt weiterhin von Bildausschnitt, Tiefe und Pixelauflösung ab. GPU-Interpolation, Geländeauswertung und einfache Animation benötigen pro Bild nur eine kleine Uniform-Aktualisierung. Vollständige Instanzdaten werden bei einem neuen aufbereiteten Snapshot übertragen.

Diese Trennung ermöglicht die erste Million als Kapazitätsziel. CPU-Tickkosten, vollständige Snapshot-Kopien und Transfers bleiben die nächsten Engpässe. Details und der Entwicklungsweg zu 20 Millionen: [ARCHITECTURE.md](ARCHITECTURE.md).

## Prüfen

```bash
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Der native Smoke-Test benötigt einen Desktop oder virtuellen X11-Bildschirm mit Grafiktreiber:

```bash
cargo run --locked -p legion -- --smoke-test --capture-dir captures/desktop
cargo run --locked -p legion -- --smoke-test --width 1024 --height 768 --capture-dir captures/compact
```

Er aktiviert die echten UI-Button-Observer, prüft Kategorienwahl, Anzahländerung, Speichern/Laden, Pause, Einzelschritt, Ergebnis, Neustart und Navigation. Er wartet auf asynchrone Zustände und erzeugt fünf Screenshots sowie `smoke-report.json`. Szenariodateien liegen isoliert im Capture-Verzeichnis. Dieser Test verwendet bewusst kleine Armeen und ersetzt weder eine physische Maus-/Tastaturprüfung noch eine Leistungsmessung.

Die separate Renderprüfung übernimmt die unveränderte Größe eines ausgewählten Szenarios:

```bash
cargo run --locked --release -p legion -- --render-check --scenario scenarios/millionenfront.json --capture-dir captures/million
```

Sie wartet auf abgeschlossene Ticks und passende Renderdaten, prüft Einzelzustände, Populationszahlen, GPU-Upload und Pipelinebereitschaft und speichert `render-check.png` sowie `render-check.json`. `--smoke-test` und `--render-check` werden getrennt ausgeführt. Ein erfolgreicher Rendercheck belegt diese Zustands- und Darstellungskette; er ist kein Echtzeitbenchmark.

## Ohne Grafik benchmarken

```bash
cargo run --locked --release -p battle_core --bin battle-bench -- --help
cargo run --locked --release -p battle_core --bin battle-bench -- --units 1200 --ticks 1200 --seed 42 --map meadow
cargo run --locked --release -p battle_core --bin battle-bench -- --units 1000000 --ticks 300 --seed 2026 --map meadow --output million.json
cargo run --locked --release -p battle_core --bin battle-bench -- --units 2000 --ticks 600 --blue-category vehicle --red-category air_unit --output categories.json
```

`--units` ist die Gesamtzahl beider Armeen. Der Bericht enthält Tickdurchsatz, Median und 95. Perzentil der Tickdauer, Verluste, Nachbarschaftsarbeit, Aktivitätsstufen und reservierte persistente Kerndaten. Der Schlusszustand wird unabhängig auf konsistente Längen, endliche Zahlen und korrekte Lebend-/Verlustzahlen geprüft. Details stehen in [battle_core/README.md](crates/battle_core/README.md).

### Gemessener Stand der Millionenfront

Ein kontrollierter Linux-Release-Lauf mit Thin LTO berechnete **300 Ticks für 1.000.000 Creature-Einheiten in 76,614 Sekunden**: rund **3,916 Ticks pro Sekunde** beziehungsweise **0,196× Echtzeit**. Das entspricht 15 Sekunden Spielzeit, mit 6.489 individuellen Verlusten und konsistentem Schlusszustand. Die Median-Tickdauer betrug 205,800 ms, das 95. Perzentil 596,611 ms. Diese CPU-Messung enthält kein Rendering und belegt keine 20-Hz-Echtzeitleistung.

Für die übrigen sechs Kategorien wurden jeweils eine Million Einheiten angelegt und **zwei Ticks** mit gültigem Zustand geprüft. Diese kurzen Kapazitätsprüfungen sind kein langes Kontaktgefecht. Persistente Kerndaten lagen bei 85.818.508 Bytes für die Creature-Millionenfront und bis zu 146.748.708 Bytes für Building; der gesamte Anwendungs- und Grafikspeicher kommt hinzu. Bedingungen und die maßgeblichen Berichte stehen in [TEST_RESULTS.md](TEST_RESULTS.md).

**CPU-Tickzeit, Grafikbildrate und Speicherwerte sind getrennte Messgrößen.** `estimated_state_bytes` enthält keine Render-Snapshots, CPU-Instanzlisten, GPU-Buffer, Worker-Stacks oder den restlichen Prozessspeicher. F3 zeigt unter anderem Simulationstick, Render-Tick, Hintergrundarbeit, Punkte, Detailmodelle und nicht nachgeholte Zeit.

## Grenzen und nächste Schritte

- Das erste Kapazitätslimit beträgt eine Million; 20 Millionen sind noch nicht implementiert oder nachgewiesen.
- Bei überlasteter CPU wird der Zeitrückstand begrenzt. Vollständige Kampfticks bleiben erhalten, die Simulation fällt gegenüber der Echtzeit zurück.
- Dichte Gefechte mit vielen gleichzeitig aktiven Gegnern sind teurer als breite, getrennte Armeen. Simulations-LOD vereinfacht das Verhalten ferner Einheiten.
- Nahkontakte verwenden begrenzte Kandidatensuche und weiche Separation. Es gibt keinen exakten Rigid-Body-Solver, allgemeines Navmesh, individuelle Skelette oder Ragdolls.
- Die Kategorien verwenden vereinfachte Bewegungsdomänen und eine Kategorie pro Armee. Gemischte Verbände, Wassernavigation, Flug-/Weltraumphysik und Tunnel benötigen weitere Systeme.
- Es gibt noch keine GPU-Kampfsimulation, GPU-Sichtbarkeitslisten oder indirekten GPU-Draw-Argumente, keine Mehrspielerfunktion und keine vollständigen Replays.

Der nächste große Meilenstein ist ein GPU-Simulationsbackend mit dauerhaft residenten Zuständen, eigenem Speicher-/Bandbreitenbudget und regelbasierten Vergleichstests gegen diesen CPU-Kern. Zielhardware und Bildratenanforderungen müssen vor dem Ausbau auf 20 Millionen festgelegt werden.

## Quellen und Lizenzen

- [Bevy 0.20](https://bevy.org/news/bevy-0-20/)
- [Offizielles Instancing-Beispiel](https://bevy.org/examples/shaders/custom-shader-instancing/)
- [Bevy-Setup](https://bevy.org/learn/quick-start/getting-started/setup/)
- [Rust installieren](https://www.rust-lang.org/tools/install)

Der eigene Quellcode steht unter MIT. Drittanbieterhinweise stehen in [THIRD_PARTY.md](THIRD_PARTY.md). Die Barlow-Fonts stehen unter SIL Open Font License, siehe `assets/fonts/OFL.txt`.
