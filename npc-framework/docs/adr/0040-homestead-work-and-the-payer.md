# 0040: Homestead work is physical, and every site has a payer

Date: 2026-11-12. Status: accepted for the living world (`living_world`).

## Context

A played character's homestead upgrades (`economy.live.homestead`) were
private building sites kept in `LiveState`. Their orders were filled the moment
a resident offered the good, anywhere on the island; a hired builder took a
stage off the work every `build_stage_ticks` without moving; a friend at a
helping tier took a stage off, or handed over a material, from wherever it
stood. In a game about watching an NPC economy that is invisible and wrong.

Village buildings already go through a physical pipeline (`village::civic`):
`ConstructionSite` entities whose materials residents carry from a store
(`life.haul materials`) and whose builder walks to the door and works there
in stages (`life.build`), paid by the civic fund.

## Decision

**Two payers, one set of resident steps.** A site is paid either by the
civic fund (village sites, unchanged) or by a played character's escrow (a
homestead upgrade). The resident leaves are shared: `life.haul materials` and
`life.build` try the village's own sites first and, only when none needs them,
a homestead upgrade of the resident's settlement (`live::jobs`). Friends help
through a new leisure leaf, `life.help`. Village sites keep their exact
behaviour; the civic planner never sees a homestead site, so it can neither
prioritise, fund nor cancel one.

**The homestead site stays in `LiveState`; it does not become a
`ConstructionSite` entity.** A homestead tier is not a catalogue building: it
has no plot of its own (it grows the existing cottage, or adds a chest, beds,
plots or a tool tier), its completion writes the cottage's household and
blocker, and its refunds go to the owner's pack and wallet, not to a civic
store. Making it a `ConstructionSite` would put it into `CivicState`'s index,
the fund hold, the planner's open-site allowance, integrity checks, the world
export and the world-visuals drawing of sites, and each of those would need a
"but not private" exception. Keeping the state where it was and sharing only
the physical steps gives the same behaviour with no exceptions in civic code.
`view::upgrade_site` stays the accessor for the site.

**What is physical.**

- An order (`LiveAct order`) reserves the goods' quote of the day plus
  `haul_fee_percent` in escrow. A resident of the homestead's settlement sells
  its own spare stock to the order, or buys the goods at a store of the
  settlement (only when the order pays at least the store's quote), walks to
  the cottage door and hands them over there. Only then is it paid from the
  escrow. Its claim (`Upgrade.incoming`) lapses after
  `haul_reservation_ticks`.
- A hired builder (`LiveAct hire`) walks to the door in its work hours and
  works there in stages of `build_stage_ticks` (`UpgradeWork`, a hashed
  sparse component). Each finished stage is paid `builder_wage` from the
  escrow; a stage it is called away from (a critical need, the end of the
  upgrade) is not paid. A builder who dies is replaced by the next one.
- A friend (`life.help`, at most once a day each, every need above
  `help_needs_above`, in its leisure block) walks over and brings one
  material the upgrade misses (from its pack, or bought at a store on the
  way; paid from the order if one covers it, else a gift), works one stage
  of `help_work_ticks` at the door, or harvests a ripe bed of the owner's
  plot into the chest or the owner's pack.
- The owner's own deliveries (`LiveAct deliver`, and everything the pack
  holds when it stands at the door) and its own work at the door are
  unchanged.

**Cancellation.** `cancel_upgrade` returns the delivered goods and the whole
escrow (unspent order coins and wages). Workers at the door stop, unpaid for
the unfinished stage; friends drop their errand. **Goods in a hauler's hands
stay the hauler's**: it bought them with its own coins at a store, or they
were its own stock, and the escrow had paid nothing for them, so the purchase
stands and nothing is created or lost. (Returning them to the seller would
need the hauler to walk back and a reversal of the store's split payment to
its lot owners; paying for and delivering goods to a cancelled site would
leave a live site after cancellation.)

## Consequences

- The living world's content hash changes: new tree leaf `life.help`, new
  hashed component `regional_upgrade_work_v1`, new `LiveState` fields.
- Homestead orders now take the time a resident needs to fit them into its
  work hours and walk; tests wait up to two short days.
- `view::upgrade_site` reports `workers` (who is on the way or working, with
  a sentence such as "Oak is bringing 2 log.").
- Legacy profiles do not register any of it.
