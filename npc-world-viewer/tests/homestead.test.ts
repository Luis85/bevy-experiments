import { test } from "node:test";
import assert from "node:assert/strict";
import { parseWorldState } from "../src/state.ts";
import { cropStatus } from "../src/presentation.ts";
import { example } from "./fixture.ts";

const site = {id:"garden_ne",label:"Garden plot 2",x:0,z:0,width_mm:1000,depth_mm:1000,use_kind:"garden"};
const bed = {index:0,position:{x:0,z:0},kind:"village_grain",stage:"growing",planted_at:0,watered_at:10,ready_at:100};
const plot = {id:2,kind:"homestead_plot",name:"Garden plot 2",position:{x:0,z:0},plot_id:site.id,
    use_kind:"garden",owner:3,owner_name:"Herp",cottage_built:false,cottage_center:{x:0,z:0},front_door:{x:0,z:0},beds:[bed]};
function sample() {const old = example(50);return {...old,terrain:{...old.terrain,expansion_plots:[site]},entities:[...old.entities,plot]};}
test("real plot and cottage SimIds remain separate from static terrain plot IDs", ()=>{
    const state = parseWorldState({...sample(),entities:[plot,{id:3,kind:"cottage",name:"Herp’s cottage",
        position:{x:0,z:0},plot_id:site.id,owner:3,owner_name:"Herp"}]});
    assert.equal(state.entities[0].kind,"homestead_plot");
    assert.equal(state.entities[1].kind,"cottage");
    assert.equal(state.entities[0].id,2);
    assert.deepEqual(example().terrain.expansion_plots,[]);
});
test("homestead ownership, bed indices, crop stages and timing reject malformed or unsafe replacements", ()=>{
    for (const invalid of [
        {...plot,owner:4294967296}, {...plot,plot_id:"missing"}, {...plot,beds:[bed,bed]},
        {...plot,use_kind:"housing",beds:[{...bed,index:3}]},
        {...plot,beds:[{...bed,stage:"fake_ready"}]}, {...plot,beds:[{...bed,stage:"empty"}]},
        {...plot,beds:[{...bed,ready_at:Number.MAX_SAFE_INTEGER+1}]},
        {...plot,beds:[{...bed,watered_at:11,ready_at:10}]},
        {...plot,beds:[{...bed,position:{x:0.5,z:0}}]},
    ]) assert.throws(()=>parseWorldState({...sample(),entities:[invalid]}));
});
test("crop status uses the supplied stage and received tick, without predicting growth from browser time", ()=>{
    const state = parseWorldState(sample());
    const p = state.entities.find(entity=>entity.kind === "homestead_plot")!;
    assert.equal(p.kind,"homestead_plot");
    if (p.kind !== "homestead_plot") return;
    assert.equal(cropStatus(p.beds[0],50),"Growing · 50 ticks until ready");
    assert.equal(cropStatus({...p.beds[0],stage:"ready"},50),"Ready to harvest");
    assert.equal(cropStatus({...p.beds[0],stage:"needs_water",watered_at:null,ready_at:null},50),"Needs water");
    assert.equal(cropStatus({...p.beds[0],stage:"growing",ready_at:null},50),"Growing");
});
