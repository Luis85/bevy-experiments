# Your Living Island homestead

Create a fresh **Living Island Homestead** world (`living_homestead`) from the game menu's explorer flow. Its larger island has a market/food district, workshop row, northern fields and six prepared sites: four house plots by the southern lane and two northern gardens. Existing saved compact worlds keep their original shoreline and rules.

The new preset is installed additively. It remains available when an existing worlds folder contains an older `living_island` file; the host never overwrites a user-owned world preset to expand it. New default Living Village scenarios also use the expanded profile.

Play directly in the native 3D game using **Home & farm [F2]**, or use the ordinary shared player/AI action API. The browser remains an observer. No provider starts automatically.

## Play in the native game

Open **Home & farm** and choose **Play as hero**. This records human control of your current character and pauses host AI nudges. If another client controls the hero, stop/release that client first; the native panel never silently takes it over. Fresh and resumed explorer worlds start without an attached controller. Standalone scenarios may already have a visitor controller attached.

- **Land & crops:** select a discovered house/garden plot, then **Claim plot**. **Build cottage** shows your actual material counts. Select an individual bed to plant, water or harvest it. Work buttons walk to the correct door/bed before acting and check the rules again on arrival.
- **Supplies:** gather from discovered resource patches and make goods at known workshops. Gather six logs, turn two into eight planks at the carpenter, and keep four logs for the cottage. Select **Quantity - / +** to change gathering and trading amounts; recipes make one advertised batch.
- **Food & trade:** inspect your pack, eat, drink, rest at home or on the ground, buy/sell nearby goods and offer surplus for an actual NPC to purchase. Grain needs one real seed and one clean water per bed.

Right-click the ground to walk your selected hero; WASD/arrows pan the existing camera. The panel also offers relative exploration walks. Unknown sources and plots appear as the hero discovers them. **Stop** cancels pending approach/work and wakes resting characters; cancelled cottage work releases its materials. **Release controls** returns the hero to the ordinary control boundary without starting a provider.

F2 or **Close** collapses the panel. Tab/Shift+Tab navigate controls; Enter/Space activate, and Escape returns focus to the world. Scroll over the panel to reach lower bed/work actions in smaller windows. World clicks and camera input are shielded while interacting with the panel. When the world is paused, use **Resume world** to let commands, work and crop growth progress. Normal save/resume preserves the estate and growing crops; human control is chosen again after resume.

## Settle and build

Explore the paths and observe a plot. `homestead.plots` in your observation lists known plots, ownership and approach positions; owned plots remain remembered. You may claim one house site and one garden. A living owner's estate is protected. A deceased owner's physical estate may be reclaimed.

```json
{"verb":"claim_plot","target":206}
```

The ID above is illustrative: use the actual perceived plot ID. Claiming supplies no coins, seeds or materials.

A cottage requires **8 planks and 4 logs**, and **600 simulation ticks** of construction. Gather wood and use the ordinary saw recipe, or buy actual goods from a trader. Approach the advertised front door and request:

```json
{"verb":"build_cottage","target":206}
```

Construction reserves the real material lots. Successful completion consumes them and creates a house with a physical footprint. `stop_work`, death or a failed completion releases claims without spending the reserved materials. The site must be clear, so construction cannot trap an actor inside its new walls.

Your completed cottage becomes your home without moving you. Approach its door and use the existing `rest_home` action to recover. The home, ownership, building and garden survive normal save/resume. Reclaiming a completed estate makes its existing door your home.

## Grow and use your harvest

House sites have three beds; separate gardens have six. Each observation gives the bed's index, exact position and current stage. Approach the chosen bed rather than guessing a coordinate.

1. Obtain one real grain as seed and one clean water.
2. `plant` consumes the grain and creates an unwatered crop.
3. `water_crop` consumes one water and starts growth.
4. After **6,000 simulation ticks**, `harvest` gives **3 grain** and clears that bed.

```json
{"verb":"plant","target":206,"args":{"bed":0,"kind":"village_grain"}}
{"verb":"water_crop","target":206,"args":{"bed":0}}
{"verb":"harvest","target":206,"args":{"bed":0}}
```

These are separate requests at the appropriate stages, not an immediate command batch. Growth stops with the world's clock. Hardy grain works through the village's seasons. A full pack leaves the mature crop in place; harvesting cannot pay out twice. Other actors cannot tend or take a living owner's crop.

The machine-readable `homestead.cottage`, `homestead.crop` and `homestead.claim_limits` fields describe costs, duration, yield and limits. Use their current values and the observed affordances. Claim/build/crop outcomes also reach the connected agent's event feed.

Mill your grain into flour, gather or buy the required water/wood, and bake food through the normal workstations. Eat or store it for your own needs. Surplus may be sold to a nearby trader or posted with `offer_goods`; an actual NPC buyer must need it, afford the quote and physically complete the trade. A listing alone earns nothing. Your cottage and crops create useful goods through real materials and time, rather than an automatic income payment.

See the [village economy guide](living-island-economy.md) for trading and streaming both views, and the [implementation plan](superpowers/plans/2026-11-02-living-village-homestead.md) for scope. This is a compact home/garden loop; it does not add romances, livestock, immigration or unrestricted town construction.
