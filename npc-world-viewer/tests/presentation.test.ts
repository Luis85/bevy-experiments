import { test } from "node:test";
import assert from "node:assert/strict";
import { carrying, connectionMessage, entityName, goodName, jobName, landBounds, recipeName } from "../src/presentation.ts";
import { example } from "./fixture.ts";

test("public goods, resources and recipe jobs use friendly names without altering actor names", () => {
    assert.equal(goodName("village_water"), "Clean water");
    assert.equal(goodName("village_warm_clothes"), "Warm clothes");
    assert.equal(goodName("village_new_crop"), "New crop");
    assert.equal(recipeName("village_tailor"), "Sew warm clothes");
    assert.equal(jobName("Craft village_tailor"), "Sew warm clothes");
    assert.equal(jobName("Buy village_leather"), "Buy leather");
    assert.equal(entityName(example().entities[0]), "Ada’s farm");
});
test("carrying combines repeated stacks exactly and keeps item identity records unchanged", () => {
    const items = [{id:1,kind:"village_thread",count:2},{id:2,kind:"bread",count:1},
        {id:3,kind:"village_thread",count:3},{id:4,kind:"log",count:0}];
    const before = structuredClone(items);
    assert.deepEqual(carrying(items), [{kind:"bread",label:"Bread",count:1n},
        {kind:"village_thread",label:"Thread",count:5n}]);
    assert.deepEqual(items,before);
    assert.equal(carrying([{id:1,kind:"ore",count:Number.MAX_SAFE_INTEGER},
        {id:2,kind:"ore",count:1}])[0].count, 9007199254740992n);
});
test("camera bounds follow actual land rather than a hardcoded island radius", () => {
    assert.deepEqual(landBounds(example().terrain),{minX:-1000,maxX:1000,minZ:-1000,maxZ:1000});
    assert.equal(landBounds({...example().terrain,cells:[0,0,0,0]}),undefined);
    assert.deepEqual(landBounds({...example().terrain,origin:{x:-5000,z:2000},cells:[0,1,0,1]}),
        {minX:-4000,maxX:-3000,minZ:2000,maxZ:4000});
});
test("offline arrival never claims a retained world exists before the first snapshot", () => {
    for (const problem of ["disconnected","invalid","stale"] as const)
        assert.doesNotMatch(connectionMessage(false,problem).message,/last valid|saved view/i);
    assert.match(connectionMessage(true,"disconnected").message,/last valid snapshot/i);
    assert.match(connectionMessage(true,"stale",20).message,/20s ago/);
});
