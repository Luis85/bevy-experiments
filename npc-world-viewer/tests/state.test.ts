import { test } from "node:test";
import assert from "node:assert/strict";
import { parseWorldState, reconcile, type WorldState } from "../src/state.ts";
import { example } from "./fixture.ts";

test("accepts authoritative millimetres and a reset to tick zero", () => {
    assert.equal(example(100).entities[0].position.x, -12000);
    assert.equal(example(0).tick, 0);
});
test("cleared plot metadata is additive, bounded and never creates gameplay entities", () => {
    const source = example();
    assert.deepEqual(source.terrain.expansion_plots, []);
    const plot = {id:"garden_nw",label:"Garden plot 1",x:0,z:0,width_mm:1000,depth_mm:1000,use_kind:"garden"};
    const next = parseWorldState({...source,terrain:{...source.terrain,expansion_plots:[plot]}});
    assert.deepEqual(next.terrain.expansion_plots,[plot]);
    assert.deepEqual(next.entities,source.entities);
    for (const plots of [[plot,plot],[{...plot,x:2000}],[{...plot,use_kind:"owner"}],
        [{...plot,width_mm:0}],[{...plot,x:0.5}],Array.from({length:33},(_,index)=>({...plot,id:`plot_${index}`}))]) {
        assert.throws(()=>parseWorldState({...source,terrain:{...source.terrain,expansion_plots:plots}}));
    }
});
test("trade district captions validate separately from gameplay identity", () => {
    const source = example();
    assert.deepEqual(source.terrain.districts,[]);
    const district = {id:"market_food",label:"Market & food",x:0,z:0};
    const parsed = parseWorldState({...source,terrain:{...source.terrain,districts:[district]}});
    assert.deepEqual(parsed.terrain.districts,[district]);
    assert.deepEqual(parsed.entities,source.entities);
    for (const districts of [[district,district],[{...district,x:2000}],[{...district,z:0.5}]])
        assert.throws(()=>parseWorldState({...source,terrain:{...source.terrain,districts}}));
});
test("accepts public production and resource timing while preserving old snapshots", () => {
    const source = example();
    assert.deepEqual(source.entities[0].kind === "building" && source.entities[0].recipes, []);
    const recipe = {name:"village_bake",duration_ticks:240,
        inputs:[{kind:"village_flour",count:2}],outputs:[{kind:"bread",count:4}]};
    const value = parseWorldState({...source,entities:[
        {...source.entities[0],recipes:[recipe]},
        {id:2,name:"grain",kind:"resource",position:{x:0,z:0},remaining:0,capacity:80,
            replenish_at:24000,gather_ticks:180},
    ]});
    assert.equal(value.entities[0].kind === "building" && value.entities[0].recipes[0].outputs[0].count,4);
    assert.equal(value.entities[1].kind === "resource" && value.entities[1].replenish_at,24000);
    assert.throws(()=>parseWorldState({...source,entities:[{...source.entities[0],recipes:[{...recipe,duration_ticks:-1}]}]}));
});
test("living market snapshots retain priced bids and wildlife without unsafe numeric values", () => {
    const source = example();
    const good = {kind:"village_warm_clothes",unit_price:200,offered_supply:0,
        funded_demand:1,below_reference_bid_units:2,max_affordable_bid:200,
        traded_units:0,total_traded_units:1};
    const state = parseWorldState({...source,world:{...source.world,economy:"needs_driven"},
        market:[good],entities:[...source.entities,{id:2,name:"deer",kind:"resource",
        position:{x:0,z:0},remaining:1,capacity:1,gather_ticks:240,replenish_at:24000}]});
    assert.equal(state.world.economy,"needs_driven");
    assert.equal(state.market[0].below_reference_bid_units,2);
    assert.equal(state.entities[1].name,"deer");
    for (const bad of [{...good,unit_price:0},{...good,max_affordable_bid:Number.MAX_SAFE_INTEGER+1}])
        assert.throws(()=>parseWorldState({...source,market:[bad]}));
});
test("rejects incompatible, malformed, duplicate and unsafe-number snapshots", () => {
    for (const value of [
        { ...example(), schema_version: 2 },
        { ...example(), tick: Number.MAX_SAFE_INTEGER + 1 },
        { ...example(), terrain: { ...example().terrain, cells: [1] } },
        {
            ...example(),
            entities: [...example().entities, ...example().entities],
        },
        {
            ...example(),
            entities: [
                { ...example().entities[0], position: { x: 0.5, z: 0 } },
            ],
        },
    ])
        assert.throws(() => parseWorldState(value));
});
test("reconciles full replacements by ID, removes despawns and retains surviving objects", () => {
    const existing = new Map<number, { name: string }>();
    const removed: object[] = [];
    const apply = (state: WorldState) =>
        reconcile(
            existing,
            state.entities,
            (e) => ({ name: e.name }),
            (v, e) => {
                v.name = e.name;
            },
            (v) => removed.push(v),
        );
    apply(example());
    const first = existing.get(1)!;
    apply({
        ...example(),
        entities: [{ ...example().entities[0], name: "Updated" }],
    });
    assert.equal(existing.get(1), first);
    assert.equal(first.name, "Updated");
    apply({ ...example(), entities: [] });
    assert.equal(existing.size, 0);
    assert.deepEqual(removed, [first]);
});
