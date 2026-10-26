# Benchmarks

`baseline.json` is the reference output of `npcctl bench`: ticks per second
and per-mechanic p50/p95/max (µs per tick) at 1, 100 and 500 villagers
(`village_n` scenario, seed 1, 200 warm-up ticks, then 2000 measured ticks
with the profiler on). Its `git_rev` and `profile` fields record what it was
measured on.

## Compare against the baseline

```sh
cargo run --release -p npcctl -- bench --villagers 1,100,500 --ticks 2000 \
  --baseline bench/baseline.json --pretty --out bench/current.json
```

Exit code 1 means at least one mechanic's p95 is more than 25% above the
baseline (`--threshold-pct`) **and** at least 5 µs above it (`--min-us`), so
OS scheduling and timer noise on the small mechanics does not count.
Mechanics with fewer than 100 samples in either run (`--min-samples`, e.g.
`think.start` at 1 villager, whose p95 is just its max) are skipped. A run on
a busy machine can still flag a mechanic: re-run before trusting a single
regression. The regressions are listed in `comparison.regressions` and on
stderr; `comparison.throughput` shows the ticks/s change per village size.
Compare release against release: the `profile` of both files should be
`"release"`.

`--metric p50` or `--metric mean` compare the median or the whole-run mean
instead of p95. p95 is unstable for a mechanic that is expensive on only a few
percent of ticks (`think` with one villager re-thinks every 40 ticks, so its
p95 sits on the edge between cheap and expensive ticks); the mean is smooth on
an idle machine but moved by every preempted tick on a busy one.

## Regenerate the baseline

Only after an intended performance change, on an otherwise idle machine:

```sh
cargo run --release -p npcctl -- bench --villagers 1,100,500 --ticks 2000 --pretty --out bench/baseline.json
```

Numbers depend on the machine; regenerate the baseline when it moves to a
different one rather than comparing across machines.
