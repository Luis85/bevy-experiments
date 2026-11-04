import { readFile } from "node:fs/promises";
import { parseWorldState } from "../src/state.ts";

const file = process.argv[2];
if (!file) throw new Error("Usage: npm run check:state -- PATH_TO_EXPORTED_JSON");
const state = parseWorldState(JSON.parse(await readFile(file, "utf8")));
console.log(JSON.stringify({ ok: true, schema_version: state.schema_version, tick: state.tick,
    scene: state.world.scene, entities: state.entities.length,
    people: state.entities.filter(e => e.kind === "person").length,
    buildings: state.entities.filter(e => e.kind === "building").length,
    resources: state.entities.filter(e => e.kind === "resource").length }));
