import { parseWorldState, type WorldState } from "../src/state.ts";

export function example(tick = 0): WorldState {
    return parseWorldState({
        schema_version: 1,
        tick,
        world: {
            id: "island_village",
            name: "Island Village",
            scene: "island_village",
            units: "mm",
            day: 0,
            season: "spring",
            night: false,
            day_length_ticks: 24000,
        },
        terrain: {
            origin: { x: -1000, z: -1000 },
            cell_size_mm: 1000,
            width: 2,
            height: 2,
            cells: [0, 1, 1, 0],
        },
        entities: [
            {
                id: 1,
                name: "Ada’s farm",
                kind: "building",
                role: "farmer",
                position: { x: -12000, z: 6000 },
            },
        ],
    });
}
