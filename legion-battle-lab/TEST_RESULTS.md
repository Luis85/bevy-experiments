# Prüfbericht — Legion Battle Lab 0.1

Prüfdatum: **26. Oktober 2026**. Die Quelle besteht die dokumentierten Rust-Prüfungen und **alle zehn nativen Fälle**: zwei vollständige UI-Abläufe, die Millionenfront und sieben Kategorienansichten. Eine Million individuelle Creature-Zustände wurde zusätzlich über 300 Kampfticks geprüft. Die erreichte CPU-Geschwindigkeit liegt deutlich unter Echtzeit; Kapazität, Spielkorrektheit und Darstellung werden deshalb getrennt bewertet.

## Umgebung und Buildstand

| Bestandteil | Prüfkonfiguration |
|---|---|
| System | Linux x86_64, virtualisierte Umgebung |
| CPU und Speicher | Intel Xeon 8573C; neun sichtbare logische CPUs, CPU-Quota acht Kerne; 8 GiB Speicherlimit |
| Rust / Engine | Rust 1.99.1, Bevy 0.20.0, gesperrte Cargo-Abhängigkeiten |
| CPU-Benchmark | Release, Thin LTO, 16 Codegen-Units |
| Native Anwendung | Optimiertes Development-Profil; X11 über Xvfb |
| Grafik | Mesa 25.2.8, llvmpipe mit LLVM 20.1.2, Vulkan; Software-Renderer |

Die Grafikprüfung verwendet **keine dedizierte Hardware-GPU**. Ihre Gesamtdauer und angezeigte FPS sind keine Aussage über Spielleistung auf einer Gaming-GPU. Auch die CPU-Zeiten stammen aus einer virtualisierten Umgebung; Scheduling kann Ausreißer verursachen.

**Bestanden.** Der abschließende Stand einschließlich vollständiger Scene-Pipeline-Bereitschaft wurde mit `cargo build --locked --workspace` erfolgreich kompiliert. Die nachfolgenden nativen Läufe verwenden diesen Build; [Buildprotokoll](validation/build.log).

Die zehn nativen Läufe und der SHA-256-Wert ihrer ausführbaren Datei stehen im [nativen Manifest](validation/native/manifest.json). Die nativen Läufe wurden nach den funktionalen Korrekturen und der abschließenden Clippy-Politur wiederholt. Die Bereitschaftsprüfung umfasst nun Crowd- und Standard-Scene-Pipelines, damit keine Ansichten mit noch fehlendem Gelände als vollständig gelten. Befunde und Entscheidungen des Reviews stehen in [REVIEW.md](REVIEW.md).

## Rust-Prüfungen

**32 Tests bestanden, null fehlgeschlagen:** 20 für Kern und Benchmarkvalidierung sowie zwölf für Anwendung, Worker, Shader-Datenlayout und Persistenz. [Vollständiges Testprotokoll](validation/rust-tests.log)

Geprüft wurden unter anderem individuelle Millionen-Zustände, konstante Tickdauer, reproduzierbare Starts, serielle/parallele Gleichheit, gleichzeitiger tödlicher Schaden, stationäre Gebäude, Höhenreichweite, Flussrouten, strategische Bewegung und tatsächlicher Frontkontakt. Die Anwendungstests decken Generationstausch, unveränderliche Snapshots, Fehlerantworten, Größen-/UTF-8-Prüfung, begrenztes Lesen, paralleles Speichern und Erhalt einer bestehenden Datei bei Fehlern ab.

**Clippy mit `-D warnings` bestand; rustfmt meldete keine Abweichung.** Belege: [Clippy](validation/clippy.log), [rustfmt](validation/rustfmt.log).

Reproduktion im Projektordner:

```bash
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked -p legion
```

## Native UI-Abläufe und Eingaben

| Fall | Ergebnis | Bericht und Ansichten |
|---|---|---|
| 1440 × 900 | Bestanden; 71 Prüfeinträge, fünf erfolgreich geschriebene PNGs | [Bericht](validation/native/desktop/smoke-report.json), [Menü](validation/native/desktop/01-menu.png), [Konfiguration](validation/native/desktop/02-configuration.png), [Schlacht](validation/native/desktop/03-battle.png), [Hilfe](validation/native/desktop/04-help.png), [Ergebnis](validation/native/desktop/05-result.png) |
| 1024 × 768 | Bestanden; 71 Prüfeinträge, fünf erfolgreich geschriebene PNGs | [Bericht](validation/native/compact/smoke-report.json), [Menü](validation/native/compact/01-menu.png), [Konfiguration](validation/native/compact/02-configuration.png), [Schlacht](validation/native/compact/03-battle.png), [Hilfe](validation/native/compact/04-help.png), [Ergebnis](validation/native/compact/05-result.png) |

Die Prüfeinträge umfassen tatsächliche Button-Observer-Aktivierungen und daraus folgende Zustandsprüfungen; sie sind nicht 71 voneinander unabhängige Testfälle. Beide Läufe durchlaufen alle sieben Kategorien für beide Armeen, Einerschritte der Anzahl, Gelände-/Profil-/Seedänderung, Speichern/Laden, Start, Pause bis zum ruhenden Worker, exakt einen Einzelschritt, Hilfe, Schlachtende, neue Generation beim Neustart und Rückkehr ins Menü. Gezieltes Vorspulen beschleunigt den Ablauf. **Diese Suite ist kein Geschwindigkeitsbenchmark und ersetzt keine physischen Eingaben.**

### Physische Eingabeprüfung

**Bestanden für die geprüften Abläufe.** Reale Mausereignisse öffneten die Konfiguration, starteten die Schlacht und führten zurück ins Menü. Bei 1024 × 768 blieben 500.000 + 500.000 Einheiten, der maximale `u64`-Seed, Underground-/Space-Kategorien und der 35-m-Reichweitenhinweis vollständig lesbar. F1 öffnete die Hilfe; Enter aktivierte ihre fokussierte Schaltfläche, danach blieb die Schlacht pausiert. Ein Klick ins Feld gab WASD frei: Ein über mehrere Software-Renderframes gehaltener W-Tastendruck veränderte 126.701 Bildpunkte im Weltbereich bei weiterhin pausierter Simulation. [Eingabeprotokoll](validation/physical/physical-input-check.json), [Grenzwerteansicht](validation/physical/02-extreme-configuration.png), [Hilfedialog](validation/physical/03-help-while-running.png), [pausierte Welt](validation/physical/04-help-closed-paused.png). Die Länge eines physischen Quit-Klicks war kein Abnahmekriterium; der Testprozess wurde durch das Testsystem beendet. Diese Software-GPU-Prüfung ist keine Aussage über interaktive Bildraten auf echter Grafikhardware.

Reproduktion der automatischen Abläufe:

```bash
cargo run --locked -p legion -- --smoke-test --capture-dir captures/recheck-desktop
cargo run --locked -p legion -- --smoke-test --width 1024 --height 768 --capture-dir captures/recheck-compact
```

## Millionenfront: tatsächliche Renderkette

Der [native Millionenbericht](validation/native/million/render-check.json) bestätigt bei Tick **9**: **1.000.000 gespeicherte und lebende Einheiten**, je 500.000 pro Armee, **1.000.000 eingereichte und hochgeladene GPU-Punkte**, passende Generation und Ticknummer, gültige numerische Werte sowie bereite Pipelines. Aus der Übersicht wurden zwei Punktgruppen und keine nahen Detailmodelle eingereicht. Der Screenshot entstand nach zwei zusätzlichen Renderupdates.

Der [Screenshot](validation/native/million/render-check.png) zeigt beide Armeen. Die [Pixelregression](validation/native/million/pixel-regression.json) findet außerhalb von HUD und Steuerung 156.857 Pixel der blauen und 143.446 der roten Farbfamilie; beide liegen über der Mindestschwelle von 1.000. Zusätzlich sind 563.649 Gelände-Pixel sichtbar, bei einer Mindestschwelle von 50.000. Zwei getrennte visuelle Prüfungen bestätigten die Ausgabe. Damit ist die zuvor fehlende rote Armee nach der `NoAutomaticBatching`-Korrektur erneut überprüft.

Dieser Zustand liegt **vor dem Kontaktgefecht**: 0,45 Sekunden Spielzeit, keine Verluste. Er belegt die Verbindung realer Zustände mit GPU-Upload und sichtbarer Ausgabe. Er belegt weder eine Million unterscheidbare Pixel noch dauerhafte Bildrate oder Kampfleistung bei dieser Population.

```bash
cargo run --locked -p legion -- --render-check --scenario scenarios/millionenfront.json --capture-dir captures/recheck-million
```

## Sieben Kategorienansichten

Alle sieben nativen Fälle bei 1024 × 768 bestanden. Jeder begann mit 64 gegen 64; alle 128 Datensätze bleiben gespeichert. Die Space-Ansicht hatte bereits vier Verluste, deshalb stimmen dort 124 lebende Einheiten mit 124 Punkten und Detailmodellen überein. In den übrigen Ansichten sind es jeweils 128. Jede Kategorie besitzt einen eigenen erfolgreich geschriebenen Screenshot.

| Kategorie | Lebend / Punkte / Detailmodelle | Rohbericht und Screenshot |
|---|---:|---|
| Creature | 128 / 128 / 128 | [JSON](validation/native/categories/creature/render-check.json), [PNG](validation/native/categories/creature/render-check.png) |
| Vehicle | 128 / 128 / 128 | [JSON](validation/native/categories/vehicle/render-check.json), [PNG](validation/native/categories/vehicle/render-check.png) |
| Building | 128 / 128 / 128 | [JSON](validation/native/categories/building/render-check.json), [PNG](validation/native/categories/building/render-check.png) |
| Air Unit | 128 / 128 / 128 | [JSON](validation/native/categories/air_unit/render-check.json), [PNG](validation/native/categories/air_unit/render-check.png) |
| Sea Unit | 128 / 128 / 128 | [JSON](validation/native/categories/sea_unit/render-check.json), [PNG](validation/native/categories/sea_unit/render-check.png) |
| Underground Unit | 128 / 128 / 128 | [JSON](validation/native/categories/underground_unit/render-check.json), [PNG](validation/native/categories/underground_unit/render-check.png) |
| Space Unit | 124 / 124 / 124 | [JSON](validation/native/categories/space_unit/render-check.json), [PNG](validation/native/categories/space_unit/render-check.png) |

Reproduktion nach dem Build; POSIX-Shell:

```bash
for category in creature vehicle building air_unit sea_unit underground_unit space_unit; do
  target/debug/legion --render-check --scenario "scenarios/categories/$category.json" --width 1024 --height 768 --capture-dir "captures/recheck-$category"
done
```

### Bildregressionen

Die [acht Bildprüfungen](validation/image-regressions.json) bestanden: Millionenfront und alle sieben Kategorien zeigen jeweils mindestens 50.000 Gelände-Pixel im HUD-freien Ausschnitt; die Millionenfront zusätzlich mindestens 1.000 Pixel je Teamfarbe. Das schützt gezielt vor den behobenen Fehlern mit fehlendem Gelände und fehlender roter Armee. Farbflächen sind keine Zählung einzelner sichtbarer Einheiten. Die unabhängige visuelle Prüfung bestätigte beide Armeen in allen sieben Kategorien sowie getrennte Flug- und Raumschiffmodelle.

Die gespeicherten Bilder können mit Python und der optionalen Abhängigkeit Pillow erneut geprüft werden:

```bash
python3 tools/verify_captures.py validation/native
```

## CPU-Kontaktgefecht mit einer Million

Der [Kampfbericht](validation/core/million-combat.json) verwendet Creature gegen Creature, offene Ebene, Seed 2026 und 300 Ticks. Beide Armeen erreichen Kontakt und verursachen individuelle Verluste.

| Messgröße | Ergebnis |
|---|---:|
| Abgeschlossene Ticks / Spielzeit | 300 / 15,0 s |
| Berechnungsdauer | 76,614 s |
| Tickdurchsatz | 3,916 Ticks/s; etwa 0,196× Echtzeit |
| Tickdauer Median / 95. Perzentil | 205,800 / 596,611 ms |
| Verluste Blau / Rot | 2.911 / 3.578; insgesamt 6.489 |
| Lebend Blau / Rot | 497.089 / 496.422 |
| Persistente Kerndaten | 85.818.508 Bytes |
| Schlusszustand | `state_valid: true` |

Der [serielle Vergleichslauf](validation/core/million-serial-baseline.json) benötigte 108,631 Sekunden. Beide Profile waren Release mit Thin LTO; berichtete Endpopulation, Verluste und Nachbarschaftsarbeit stimmen überein. Das [Vergleichsprotokoll](validation/core/performance-comparison.json) enthält Befehle und Binärhashes. Ein einzelner Vergleich ist keine allgemeine Beschleunigungsgarantie. **20 Echtzeit-Ticks pro Sekunde sind am Millionenlimit hier nicht erreicht.**

```bash
mkdir -p captures/core
cargo run --locked --release -p battle_core --bin battle-bench -- --units 1000000 --ticks 300 --seed 2026 --map meadow --output captures/core/million.json
```

Die [Kapazitätsberichte](validation/core/category-capacity.json) prüfen die übrigen sechs Kategorien mit jeweils einer Million Einheiten über **zwei Ticks**; alle Schlusszustände sind gültig. Das sind kurze Kapazitätsprüfungen, keine langen Kontaktgefechte. Reproduktion der sechs kurzen Läufe:

```bash
cargo build --locked --release -p battle_core --bin battle-bench
mkdir -p captures/core
for category in vehicle building air_unit sea_unit underground_unit space_unit; do
  target/release/battle-bench --units 1000000 --ticks 2 --seed 2026 --map meadow --blue-category "$category" --red-category "$category" --output "captures/core/$category.json"
done
```

## Speichergrenzen und offene Nachweise

Persistente Kerndaten sind **nicht der gesamte Prozess- oder GPU-Speicher**. Bei Building erreichten sie 146.748.708 Bytes. Die native Creature-Millionenansicht weist zusätzlich 40.000.000 Bytes Snapshot-Vektoren und 48.000.000 Bytes Render-Nutzlast aus. Weitere gleichzeitig gehaltene Snapshots, Buckets, gerundete GPU-Kapazitäten, Engine-Daten und Worker-Stacks kommen hinzu; aus diesen Einzelwerten wird keine RAM-/VRAM-Obergrenze abgeleitet.

Nicht nachgewiesen sind native Windows-/macOS-Ausführung, echte Screenreader-Bedienung, Dauerlast über Stunden, beliebige dichte Millionen-Gefechte, plattformübergreifende Bitgleichheit oder 20 Millionen Einheiten. Luft-, See-, Untergrund- und Weltraumdomänen bleiben vereinfachte Proxys; gemischte Verbände sind noch nicht konfigurierbar. Diese Grenzen bestehen trotz erfolgreicher Prüfungen der dokumentierten ersten Iteration.
