import { z } from "zod";

const uint = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);
const pos = z.object({
    x: z.number().int().min(-2147483648).max(2147483647),
    z: z.number().int().min(-2147483648).max(2147483647),
});
const base = {
    id: uint.max(4294967295),
    name: z.string().max(512),
    position: pos,
};
const need = z.object({
    value: z.number().int().min(0).max(100000),
    level: z.string(),
});
export const bubbleSchema = z.object({
    kind: z.enum(["thought", "speech"]),
    text: z.string().refine((text) => Array.from(text).length <= 512, "bubble text exceeds 512 characters"),
    tick: uint,
    expires_tick: uint,
}).refine((bubble) => bubble.expires_tick >= bubble.tick, "bubble expires before it was recorded");
export const expansionPlotSchema = z.object({
    id: z.string().min(1).max(64),
    label: z.string().min(1).max(128),
    x: pos.shape.x,
    z: pos.shape.z,
    width_mm: uint.min(1).max(256000),
    depth_mm: uint.min(1).max(256000),
    use_kind: z.enum(["housing", "garden"]),
});
const districtSchema = z.object({id:z.string().min(1).max(64),label:z.string().min(1).max(128),x:pos.shape.x,z:pos.shape.z});
const ownership = {owner:uint.max(4294967295).nullable(),owner_name:z.string().max(512).nullable()};
export const cropBedSchema = z.object({
    index:uint.max(5),position:pos,kind:z.string().min(1).max(128).nullable(),
    stage:z.enum(["empty","needs_water","growing","ready"]),
    planted_at:uint.nullable(),watered_at:uint.nullable(),ready_at:uint.nullable(),
}).superRefine((bed,ctx)=>{
    if ((bed.stage === "empty") !== (bed.kind === null))
        ctx.addIssue({code:"custom",message:"empty beds must have no crop; planted beds need a crop kind"});
    if (bed.planted_at !== null && bed.watered_at !== null && bed.watered_at < bed.planted_at)
        ctx.addIssue({code:"custom",message:"bed was watered before planting"});
    if (bed.watered_at !== null && bed.ready_at !== null && bed.ready_at < bed.watered_at)
        ctx.addIssue({code:"custom",message:"bed becomes ready before watering"});
});
export const entitySchema = z.discriminatedUnion("kind", [
    z.object({
        ...base,
        kind: z.literal("person"),
        role: z.string(),
        home: pos,
        workplace: uint,
        is_player: z.boolean(),
        activity: z.string(),
        wearing_warm_clothes: z.boolean().default(false),
        decision: z.string().max(128).nullable().optional(),
        health: uint.max(100000),
        coins: uint,
        needs: z.record(z.string(), need),
        inventory: z.array(
            z.object({ id: uint, kind: z.string(), count: uint }),
        ),
        bubbles: z.array(bubbleSchema).max(2).default([]),
    }),
    z.object({ ...base, kind: z.literal("building"), role: z.string(),
        recipes: z.array(z.object({name:z.string().max(128),duration_ticks:uint,
            inputs:z.array(z.object({kind:z.string().max(128),count:uint})).max(32),
            outputs:z.array(z.object({kind:z.string().max(128),count:uint})).max(32)})).max(128).default([]) }),
    z.object({
        ...base,
        kind: z.literal("resource"),
        remaining: uint,
        capacity: uint,
        replenish_at: uint.optional(),
        gather_ticks: uint.optional(),
    }),
    z.object({ ...base, kind: z.literal("item"), count: uint }),
    z.object({...base,...ownership,kind:z.literal("homestead_plot"),plot_id:z.string().min(1).max(64),
        use_kind:z.enum(["housing","garden"]),cottage_built:z.boolean(),cottage_center:pos,front_door:pos,
        beds:z.array(cropBedSchema).max(6)}),
    z.object({...base,...ownership,kind:z.literal("cottage"),plot_id:z.string().min(1).max(64)}),
]);
export const worldStateSchema = z
    .object({
        schema_version: z.literal(1),
        tick: uint,
        world: z.object({
            id: z.string(),
            name: z.string(),
            scene: z.literal("island_village"),
            units: z.literal("mm"),
            day: uint,
            season: z.enum(["spring", "summer", "autumn", "winter"]),
            night: z.boolean(),
            day_length_ticks: uint.min(1),
            economy: z.enum(["fixed","needs_driven"]).default("fixed"),
        }),
        terrain: z
            .object({
                origin: pos,
                cell_size_mm: uint.min(1).max(1000000),
                width: uint.min(1).max(256),
                height: uint.min(1).max(256),
                cells: z
                    .array(z.union([z.literal(0), z.literal(1)]))
                    .max(65536),
                paths: z
                    .array(
                        z.object({
                            x: z.number().int(),
                            z: z.number().int(),
                            width_mm: uint.min(1),
                            depth_mm: uint.min(1),
                        }),
                    )
                    .max(1000)
                    .default([]),
                expansion_plots: z.array(expansionPlotSchema).max(32).default([]),
                districts: z.array(districtSchema).max(16).default([]),
            })
            .refine(
                (t) => t.cells.length === t.width * t.height,
                "terrain dimensions do not match cells",
            )
            .refine(
                (t) =>
                    Math.max(t.width, t.height) * t.cell_size_mm * 0.016 <=
                    4096,
                "terrain exceeds render budget",
            ),
        entities: z.array(entitySchema).max(10000),
        market: z.array(z.object({kind:z.string().max(128),unit_price:uint.min(1).max(1000000),
            offered_supply:uint,funded_demand:uint,traded_units:uint,total_traded_units:uint,
            below_reference_bid_units:uint.default(0),max_affordable_bid:uint.nullable().optional()})).max(32).default([]),
    })
    .superRefine((s, ctx) => {
        const ids = new Set<number>();
        for (const e of s.entities) {
            if (ids.has(e.id))
                ctx.addIssue({
                    code: "custom",
                    message: `duplicate entity ID ${e.id}`,
                });
            ids.add(e.id);
        }
        const plots = new Set<string>();
        const t = s.terrain;
        for (const plot of t.expansion_plots) {
            if (plots.has(plot.id)) ctx.addIssue({code:"custom",message:`duplicate plot ID ${plot.id}`});
            plots.add(plot.id);
            if (plot.x - plot.width_mm/2 < t.origin.x || plot.z - plot.depth_mm/2 < t.origin.z
                || plot.x + plot.width_mm/2 > t.origin.x + t.width*t.cell_size_mm
                || plot.z + plot.depth_mm/2 > t.origin.z + t.height*t.cell_size_mm) {
                ctx.addIssue({code:"custom",message:`plot ${plot.id} lies outside terrain bounds`});
            }
        }
        const districts = new Set<string>();
        for (const district of t.districts) {
            if (districts.has(district.id)) ctx.addIssue({code:"custom",message:`duplicate district ID ${district.id}`});
            districts.add(district.id);
            if (district.x < t.origin.x || district.z < t.origin.z
                || district.x >= t.origin.x+t.width*t.cell_size_mm || district.z >= t.origin.z+t.height*t.cell_size_mm)
                ctx.addIssue({code:"custom",message:`district ${district.id} lies outside terrain bounds`});
        }
        const projectedPlots = new Set<string>(), cottages = new Set<string>();
        for (const entity of s.entities) if (entity.kind === "homestead_plot" || entity.kind === "cottage") {
            if (!plots.has(entity.plot_id)) ctx.addIssue({code:"custom",message:`entity ${entity.id} references an unknown plot`});
            const registry = entity.kind === "homestead_plot" ? projectedPlots : cottages;
            if (registry.has(entity.plot_id)) ctx.addIssue({code:"custom",message:`duplicate ${entity.kind} for ${entity.plot_id}`});
            registry.add(entity.plot_id);
            if (entity.kind === "homestead_plot") {
                const indices = new Set<number>();
                const limit = entity.use_kind === "housing" ? 3 : 6;
                for (const bed of entity.beds) {
                    if (indices.has(bed.index) || bed.index >= limit)
                        ctx.addIssue({code:"custom",message:`invalid or repeated bed index on ${entity.plot_id}`});
                    indices.add(bed.index);
                }
            }
        }
    });
export type WorldState = z.infer<typeof worldStateSchema>;
export type WorldEntity = z.infer<typeof entitySchema>;
export type Person = Extract<WorldEntity, { kind: "person" }>;
export type PublicBubble = z.infer<typeof bubbleSchema>;
export type ExpansionPlot = z.infer<typeof expansionPlotSchema>;
export type CropBed = z.infer<typeof cropBedSchema>;

export function parseWorldState(value: unknown): WorldState {
    return worldStateSchema.parse(value);
}

/** Full replacements deliberately accept lower ticks (reset/resume) and absent entities (despawn). */
export function reconcile<T>(
    existing: Map<number, T>,
    next: WorldEntity[],
    create: (entity: WorldEntity) => T,
    update: (value: T, entity: WorldEntity) => void,
    remove: (value: T) => void,
): void {
    const ids = new Set(next.map((e) => e.id));
    for (const [id, value] of existing)
        if (!ids.has(id)) {
            remove(value);
            existing.delete(id);
        }
    for (const entity of next) {
        let value = existing.get(entity.id);
        if (!value) {
            value = create(entity);
            existing.set(entity.id, value);
        }
        update(value, entity);
    }
}
