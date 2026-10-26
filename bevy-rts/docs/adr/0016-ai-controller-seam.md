---
status: "accepted"
date: 2026-10-24
decision-makers: "controller (accepted at the M1 Task 1 review, rulings R69, R84), lead"
consulted: "M1 planner (planner-m1-report.md, spec-gap resolutions accepted provisionally as R69)"
informed: "M1 implementers (Tasks 2, 20)"
---

# AI controller seam: a client-hosted `AiController` on a client-built `PlayerView` (stub scope)

## Context and Problem Statement

Spec §5.5 and §8 promise that `Controller::Ai` exists from day one, backed by a do-nothing AI used in tests, and that
an AI runs outside the sim, reads a per-player `PlayerView` from `fl_view` and emits ordinary `PlayerCommand`s through
the delayed path. §8 also says the sim's `Publish` phase builds `PlayerView`. M0 has `Controller::Ai(id)` in a
scenario's players (`fl_protocol::Controller`), but nothing reads it: no `PlayerView` type, no controller trait, and
nothing that hosts one. Visibility doesn't exist until M3, so a sim-built view would be the snapshot filtered by
ownership, with nothing to hide. Where AI runs in multiplayer (which peer, which thread) belongs to the first AI
faction (§9.3).

## Decision Drivers

* The seam must exist and be exercised from M1, so AI work later starts on a tested path.
* An AI gets no privileged access: it reads only its player's view and issues the same commands a human does.
* No sim-side state or snapshot schema before vision defines what a player can see.
* `fl_view` stays Bevy-free (ADR-0002); the sim never depends on an AI.
* Don't pre-decide the AI placement questions that belong to the first AI faction.

## Considered Options

* A controller trait in `fl_view`, on a `PlayerView` the client builds from the snapshot, hosted by `fl_client`
* Build `PlayerView` in the sim's `Publish` phase now
* Run AI as sim systems
* No seam until the first AI faction

## Decision Outcome

Chosen option: "A controller trait in `fl_view`, on a client-built `PlayerView`, hosted by `fl_client`", because it
gives `Controller::Ai` a real, tested path in M1 without adding sim state that M3 would replace.

- **Types.** `fl_view::ai::{PlayerView, AiController, NullAi, ai_controller}`.
  - `PlayerView { tick, player, team, own, visible }`: `own` holds the records whose owner is the player; `visible`
    holds other players' records whose `visible_to` bit for the team is set; both in `SimId` order.
  - `AiController: Send + Sync + 'static` has `id()` and `think(&mut self, &PlayerView, &StringTables) ->
    Vec<CommandDraft>`. It may use floats; nothing it does is hashed.
  - `NullAi` (id `"null"`) issues nothing. `ai_controller(id)` maps `"null"` to `NullAi` and anything else to `None`.
- **Building the view.** `PlayerView::from_snapshot(snapshot, player, team)` builds it on the client side until
  vision exists. M3 moves it into the sim's `Publish` phase, where `visible` comes from the visibility grid; the type
  stays in `fl_view`.
- **Hosting.** Players whose controller is `Controller::Ai(id)` are hosted by `fl_client::ai::AiHost`, filled by
  `fl_game`'s `GameBuilder` from the scenario's players; an unknown id fails `prepare()` with
  `"unknown AI controller '<id>' for player <n>"`. The host thinks at most once per frame, in a frame where the
  newest snapshot's tick changed (several ticks in one frame give one think, on the newest snapshot; a load that
  restarts at an earlier tick counts as a change, R91). Its drafts go
  through `LocalDriver` as that player, so they are stamped, delayed, sequenced and recorded like a human's. A draft
  for another player is dropped with a warning.
- **Scope.** This is a stub seam. The full AI placement ADR (which peer hosts AI in a lockstep match, threads and
  async planning, an open AI registry, budgets) stays deferred to the first AI faction (spec §9.3).

### Consequences

* Good, because `Controller::Ai` works end to end in M1 (`testbed_ai` runs with player 1 idle), and its commands go
  through the normal path, replays included.
* Good, because no sim state or snapshot schema is added before M3 defines visibility.
* Neutral, because the trait signature may change when the first real AI needs more (the known map, events); it is a
  contract item, so that change goes through an ADR.
* Bad, because until M3 the client-built view trusts the snapshot's visibility bits, and the client has the whole
  snapshot; a cheating AI is possible in principle. That is acceptable for a do-nothing AI.
* Bad, because the view is built on the client thread; a heavy AI would need its own thread later.

### Confirmation

* Types (Task 2, `fl_view/tests/view.rs`): `player_view_splits_own_and_visible`, `null_ai_issues_nothing`,
  `ai_controller_by_id`.
* Hosting (Task 20, `fl_client/tests/ai.rs`): `null_ai_issues_nothing`, `ai_thinks_once_per_new_tick`,
  `draft_for_another_player_is_dropped`; (`fl_game/tests/ai.rs`): `ai_scenario_builds_host`, `unknown_ai_is_error`.
* `cargo xtask arch-check`: `fl_view` gains no Bevy dependency, and no client crate depends on `fl_sim`.
* The M1 demo checklist row "`testbed_ai` runs with player 1 idle" (Task 21).

## Pros and Cons of the Options

### Trait in `fl_view`, client-built view, client host

* Good, because it is small, exercised, and matches §8's "outside the sim".
* Bad, because the view's builder moves once (client to `Publish`) in M3.

### `PlayerView` built in `Publish` now

* Good, because the builder is in its final place from the start.
* Bad, because without vision it is the snapshot filtered by owner: new per-player state and schema with no
  information in it, to be reworked in M3.

### AI as sim systems

* Good, because the AI would see sim state directly and need no view.
* Bad, because spec §8 rules it out: it must not desync, may use floats and threads, and must be swappable without
  changing the rules hash.

### No seam until the first AI faction

* Good, because there is no code now.
* Bad, because §8 requires `Controller::Ai` backed by a do-nothing AI from day one, and scenarios already carry
  `Controller::Ai(id)` that nothing reads.

## More Information

* Spec §5.5 (controllers), §8 (AI-driven faction seam), §9.3 (AI controllers: trigger "the first AI faction").
* Related: ADR-0002 (`fl_view`, drivers), ADR-0007 (the command envelope and `LocalDriver`).
* Controller ruling R69 (the client-side `PlayerView` stub until M3).
* Revisit: in M3 (move the view into `Publish`), and with the first AI faction (the full placement ADR).
