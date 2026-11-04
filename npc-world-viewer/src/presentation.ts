import type { CropBed, Person, WorldEntity, WorldState } from "./state.ts";

const goods: Record<string, string> = {
    village_grain: "Grain", village_flour: "Flour", village_water: "Clean water",
    village_bowl: "Bowls", village_raw_meat: "Raw meat", village_hide: "Hides",
    village_leather: "Leather", village_fiber: "Fiber", village_thread: "Thread",
    village_cooked_meal: "Cooked meals", village_warm_clothes: "Warm clothes",
    village_spear: "Hunting spears", village_berries: "Berries", village_coins: "Coins",
    village_diary: "Diary", log: "Logs", plank: "Planks", ore: "Ore",
    ingot: "Ingots", bread: "Bread", hammer: "Hammers", axe: "Axes", pickaxe: "Pickaxes",
};
const recipes: Record<string, string> = {
    village_mill: "Mill grain", village_bake: "Bake bread", village_saw: "Saw planks",
    village_carve: "Carve bowls", village_smelt: "Smelt ore", village_tool: "Forge a hammer",
    village_forge_spear: "Forge a hunting spear", village_cook: "Cook meals",
    village_tan: "Tan hides", village_spin: "Spin thread", village_tailor: "Sew warm clothes",
};

export function words(value: string): string {
    const text = value.replace(/^village_/, "").replaceAll("_", " ");
    return text ? text[0].toUpperCase() + text.slice(1) : text;
}
export function goodName(kind: string): string { return goods[kind] ?? words(kind); }
export function recipeName(name: string): string { return recipes[name] ?? words(name); }
export function entityName(entity: WorldEntity): string {
    if (entity.kind === "resource") {
        return ({village_grain: "Grain field", village_fiber: "Fiber patch", village_berries: "Berry patch",
            log: "Timber grove", ore: "Ore deposit", deer: "Deer"} as Record<string,string>)[entity.name] ?? goodName(entity.name);
    }
    return entity.kind === "item" ? goodName(entity.name) : entity.name;
}
export function jobName(value: string): string {
    return value.replace(/\bcraft[:\s]+(village_[a-z0-9_]+)\b/gi, (_whole, recipe: string) => recipeName(recipe))
        .replace(/\bvillage_[a-z0-9_]+\b/g, kind => goodName(kind).toLowerCase())
        .replaceAll("_", " ");
}
export function cropStatus(bed: CropBed, tick: number): string {
    if (bed.stage === "empty") return "Empty bed";
    if (bed.stage === "needs_water") return "Needs water";
    if (bed.stage === "ready") return "Ready to harvest";
    return bed.ready_at === null ? "Growing" : `Growing · ${Math.max(0,bed.ready_at-tick).toLocaleString()} ticks until ready`;
}

/** Group only the presentation. Stable item records and authoritative counts stay untouched. */
export function carrying(inventory: Person["inventory"]): {kind: string; label: string; count: bigint}[] {
    const totals = new Map<string,bigint>();
    for (const item of inventory) if (item.count > 0) {
        totals.set(item.kind, (totals.get(item.kind) ?? 0n) + BigInt(item.count));
    }
    return [...totals].map(([kind,count]) => ({kind,label:goodName(kind),count}))
        .sort((a,b) => a.label.localeCompare(b.label, "en"));
}

/** Bounds come exclusively from received land cells, including older/smaller worlds. */
export function landBounds(terrain: WorldState["terrain"]): {minX:number;maxX:number;minZ:number;maxZ:number} | undefined {
    let left = terrain.width, right = -1, top = terrain.height, bottom = -1;
    for (let index = 0; index < terrain.cells.length; index++) if (terrain.cells[index] === 1) {
        const x = index % terrain.width, z = Math.floor(index / terrain.width);
        left = Math.min(left,x); right = Math.max(right,x);
        top = Math.min(top,z); bottom = Math.max(bottom,z);
    }
    if (right < left || bottom < top) return undefined;
    return {minX:terrain.origin.x + left * terrain.cell_size_mm,
        maxX:terrain.origin.x + (right+1) * terrain.cell_size_mm,
        minZ:terrain.origin.z + top * terrain.cell_size_mm,
        maxZ:terrain.origin.z + (bottom+1) * terrain.cell_size_mm};
}

export function connectionMessage(hasState: boolean, problem: "disconnected" | "invalid" | "stale", age = 0): {status:string;message:string} {
    if (!hasState) return {status:problem === "disconnected" ? "Cannot connect" : "Waiting for simulation",
        message:"The village will appear when a valid world snapshot is available. Reconnecting automatically."};
    if (problem === "stale") return {status:"Snapshot is stale", message:`Last file update ${age}s ago. Showing the saved view; check the simulation host.`};
    return {status:problem === "disconnected" ? "Connection interrupted" : "Update unavailable",
        message:"Showing the last valid snapshot. Reconnecting automatically."};
}
