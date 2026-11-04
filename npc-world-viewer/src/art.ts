import { Canvas } from "excalibur";
import type { ExpansionPlot, WorldEntity, WorldState } from "./state.ts";
import { entityName } from "./presentation.ts";

export const PX_PER_MM = 0.016;
const colors: Record<string, string> = {
    farmer: "#cba74f",
    miller: "#d8ca99",
    baker: "#c7784c",
    water_keeper: "#75b6c1",
    woodcutter: "#7c9863",
    carpenter: "#a77e4b",
    miner: "#86949a",
    smith: "#6f7b86",
    innkeeper: "#a36b88",
    visitor: "#f5d178",
    hunter: "#87966a",
    tanner: "#b68a61",
    tailor: "#af7b94",
};
type Painter = (
    color: string,
    x: number,
    y: number,
    w: number,
    h: number,
) => void;
function painter(ctx: CanvasRenderingContext2D): Painter {
    return (color, x, y, w, h) => {
        ctx.fillStyle = color;
        ctx.fillRect(
            Math.round(x),
            Math.round(y),
            Math.round(w),
            Math.round(h),
        );
    };
}

export function terrainGraphic(state: WorldState): Canvas {
    const t = state.terrain;
    const cell = t.cell_size_mm * PX_PER_MM;
    // Limit render allocation independently of the supported integer world extent.
    const width = t.width * cell,
        height = t.height * cell;
    if (width > 4096 || height > 4096)
        throw new Error("Map exceeds the 4096 pixel terrain budget");
    return new Canvas({
        width,
        height,
        cache: true,
        draw: (ctx) => {
            const p = painter(ctx);
            const land = (x: number, y: number) =>
                x >= 0 &&
                y >= 0 &&
                x < t.width &&
                y < t.height &&
                t.cells[y * t.width + x] === 1;
            const winter = state.world.season === "winter";
            for (let y = 0; y < t.height; y++)
                for (let x = 0; x < t.width; x++) {
                    const px = x * cell,
                        py = y * cell;
                    const n = (x * 37 + y * 73 + x * y * 3) % 31;
                    if (!land(x, y)) {
                        const shore = [
                            [1, 0],
                            [-1, 0],
                            [0, 1],
                            [0, -1],
                        ].some(([dx, dy]) => land(x + dx, y + dy));
                        p(
                            shore ? "#3c8190" : n < 6 ? "#245e72" : "#235a6e",
                            px,
                            py,
                            cell,
                            cell,
                        );
                        if (n < 4) {
                            p("#3d7b8b", px + 3, py + 6, 7, 1);
                            p("#2f6b7d", px + 6, py + 8, 5, 1);
                        }
                        continue;
                    }
                    const sand =
                        !land(x + 1, y) ||
                        !land(x - 1, y) ||
                        !land(x, y + 1) ||
                        !land(x, y - 1);
                    p(
                        sand
                            ? "#d2bd83"
                            : winter
                              ? "#b4c9bd"
                              : n < 12
                                ? "#8eac63"
                                : "#88a65e",
                        px,
                        py,
                        cell,
                        cell,
                    );
                    if (sand) {
                        if (n < 12) p("#bcaa74", px + 5, py + 9, 2, 2);
                    } else if (n < 14) {
                        p(
                            winter ? "#8aa69a" : "#72934e",
                            px + (n % 8) + 2,
                            py + 4,
                            1,
                            3,
                        );
                        p(
                            winter ? "#d1dbcc" : "#acc27b",
                            px + 7,
                            py + 10,
                            3,
                            1,
                        );
                    } else if (n === 20) {
                        p("#e6ce8e", px + 8, py + 6, 2, 2);
                        p("#6e904c", px + 8, py + 8, 1, 3);
                    }
                }
            // Cleared sites are map metadata, not buildings, owners or gameplay controls.
            for (const plot of t.expansion_plots) {
                const x = (plot.x - t.origin.x - plot.width_mm/2) * PX_PER_MM;
                const y = (plot.z - t.origin.z - plot.depth_mm/2) * PX_PER_MM;
                const w = plot.width_mm * PX_PER_MM, h = plot.depth_mm * PX_PER_MM;
                p(winter ? "#b7bda4" : "#9caa72",x,y,w,h);
                p(winter ? "#c1c8b1" : "#aeb67f",x+3,y+3,w-6,h-6);
                for (const [cx,cy] of [[x+1,y+1],[x+w-4,y+1],[x+1,y+h-4],[x+w-4,y+h-4]]) {
                    p("#7f805a",cx,cy,3,3);
                    p("#dad2a6",cx,cy,2,1);
                }
            }
            // Presentation paths are supplied by the producer, as in the native island view.
            for (const road of t.paths) {
                const x = (road.x - t.origin.x - road.width_mm / 2) * PX_PER_MM;
                const y = (road.z - t.origin.z - road.depth_mm / 2) * PX_PER_MM;
                p(
                    "#a78f61",
                    x,
                    y,
                    road.width_mm * PX_PER_MM,
                    road.depth_mm * PX_PER_MM,
                );
                p(
                    "#baa675",
                    x + 2,
                    y + 2,
                    road.width_mm * PX_PER_MM - 4,
                    road.depth_mm * PX_PER_MM - 4,
                );
            }
        },
    });
}

function roof(p: Painter, color: string): void {
    p("#493f39", 61, 29, 54, 20);
    for (let row = 0; row < 5; row++)
        p(color, 64 - row * 3, 25 + row * 4, 40 + row * 6, 4);
    for (let row = 0; row < 4; row++)
        p("#ffffff20", 65 - row * 3, 26 + row * 4, 38 + row * 6, 1);
    p("#413e36", 59, 45, 58, 3);
}
function tree(p: Painter, x: number, y: number): void {
    p("#4d6044", x + 5, y + 21, 8, 5);
    p("#73583e", x + 7, y + 11, 4, 14);
    p("#385d48", x, y + 6, 18, 12);
    p("#426e4e", x + 3, y, 12, 20);
    p("#64894f", x + 4, y + 2, 6, 9);
}

export function entityGraphic(
    e: WorldEntity,
    selected: boolean,
    labels: boolean,
    site?: ExpansionPlot,
): Canvas {
    // All world graphics are purpose-drawn bitmap sprites. No AI assets or external art licensing.
    return new Canvas({
        width: 176,
        height: 112,
        cache: true,
        draw: (ctx) => {
            const p = painter(ctx);
            if (selected && e.kind === "homestead_plot" && site) {
                const x = 88+(site.x-e.position.x-site.width_mm/2)*PX_PER_MM;
                const y = 56+(site.z-e.position.z-site.depth_mm/2)*PX_PER_MM;
                const w = site.width_mm*PX_PER_MM, h = site.depth_mm*PX_PER_MM;
                p("#ffe3a0",x,y,w,2); p("#ffe3a0",x,y+h-2,w,2);
                p("#ffe3a0",x,y,2,h); p("#ffe3a0",x+w-2,y,2,h);
            } else if (selected) {
                p("#ffe3a0", 75, 52, 26, 2);
                p("#ffe3a0", 75, 67, 26, 2);
                p("#ffe3a0", 73, 54, 2, 13);
                p("#ffe3a0", 101, 54, 2, 13);
            }
            if (e.kind === "person") {
                p("#35493670", 81, 63, 16, 4);
                if (e.activity === "dead") {
                    p("#625951", 80, 59, 18, 5);
                    p("#ceaa80", 80, 58, 5, 5);
                } else {
                    p("#3d4543", 83, 58, 4, 6);
                    p("#3d4543", 90, 58, 4, 6);
                    p(e.wearing_warm_clothes ? "#8f7553" : colors[e.role] ?? "#b48c68", 82, 48, 13, 11);
                    if (e.wearing_warm_clothes) { p("#dfd0b3",81,48,15,3); p("#dfd0b3",87,51,3,9); }
                    p("#e2b88c", 80, 49, 3, 8);
                    p("#d8a77f", 95, 49, 3, 8);
                    p("#e3bd96", 84, 39, 10, 10);
                    p("#63503c", 83, 37, 11, 4);
                    p("#3e453e", 91, 43, 2, 2);
                    if (e.role === "farmer" || e.role === "woodcutter") {
                        p("#cfb472", 81, 37, 16, 3);
                        p("#b99b5e", 85, 33, 9, 4);
                    }
                    if (e.is_player) {
                        p("#f5d178", 86, 27, 6, 3);
                        p("#f5d178", 88, 30, 2, 3);
                    }
                }
            } else if (e.kind === "homestead_plot") {
                for (const bed of e.beds) {
                    const x = 88+(bed.position.x-e.position.x)*PX_PER_MM;
                    const y = 56+(bed.position.z-e.position.z)*PX_PER_MM;
                    p("#997c55",x-10,y-7,20,14); p("#b09a6c",x-9,y-6,18,12);
                    for (const row of [-3,2]) p("#866e4c",x-8,y+row,16,1);
                    if (bed.stage === "needs_water") {
                        p("#7c9058",x-2,y-3,2,4); p("#d1b778",x+1,y-3,2,2);
                        p("#7ba6ae",x+6,y-6,2,3);
                    } else if (bed.stage === "growing") {
                        for (const cx of [x-5,x+4]) {
                            p("#567445",cx,y-3,2,8); p("#8fa866",cx-3,y-3,5,3);
                            p("#799653",cx+1,y-5,4,3);
                        }
                    } else if (bed.stage === "ready") {
                        for (const cx of [x-5,x+4]) {
                            p("#668347",cx,y-5,2,10);
                            p(bed.kind?.includes("berries") ? "#97516b" : "#e2bf69",cx-3,y-8,7,6);
                            p(bed.kind?.includes("berries") ? "#c27a8d" : "#edda97",cx-2,y-8,3,2);
                        }
                    }
                }
            } else if (e.kind === "building" || e.kind === "cottage") {
                const role = e.kind === "cottage" ? "home" : e.role;
                if (role === "water_keeper") {
                    p("#45665c70", 68, 61, 40, 8);
                    p("#778b86", 73, 49, 28, 17);
                    p("#aec0ae", 71, 48, 32, 5);
                    p("#315b67", 78, 48, 18, 5);
                    p("#705340", 70, 30, 4, 25);
                    p("#705340", 101, 30, 4, 25);
                    roof(p, "#688f8c");
                } else {
                    p("#46604560", 64, 68, 58, 8);
                    p("#9d8866", 64, 43, 48, 30);
                    p("#e0c79c", 64, 43, 45, 25);
                    p("#786245", 64, 46, 3, 23);
                    p("#786245", 107, 46, 3, 23);
                    p("#6f523a", 82, 52, 11, 17);
                    p("#d3a667", 90, 59, 2, 2);
                    p("#6c8381", 69, 52, 9, 9);
                    p("#efd79e", 70, 53, 6, 6);
                    p("#887453", 73, 52, 1, 9);
                    p("#6c8381", 97, 52, 9, 9);
                    p("#c5d5b9", 98, 53, 6, 6);
                    roof(p, e.kind === "cottage" ? "#8b976c" : colors[role] ?? "#a57153");
                    if (["baker", "smith"].includes(role)) {
                        p("#5f6260", 100, 20, 7, 17);
                        p("#a2a795", 99, 18, 9, 4);
                        p("#c3c4ae80", 102, 9, 6, 6);
                    }
                    if (role === "miller") {
                        p("#665c4a", 76, 11, 24, 31);
                        p("#e8d6ab", 82, 8, 3, 42);
                        p("#e8d6ab", 64, 26, 39, 3);
                        p("#b09a6d", 80, 24, 7, 7);
                    }
                    if (role === "farmer")
                        for (let i = 0; i < 4; i++) {
                            p("#b39552", 47 + i * 5, 56, 2, 14);
                            p("#ddc17a", 46 + i * 5, 52, 4, 7);
                        }
                    if (role === "innkeeper") {
                        p("#634b39", 114, 46, 2, 23);
                        p("#e3bd79", 110, 48, 13, 10);
                        p("#8f654b", 115, 50, 3, 6);
                    }
                    if (role === "woodcutter") {
                        p("#745138", 49, 60, 10, 6);
                        p("#b39964", 50, 61, 3, 3);
                    }
                    if (role === "miner") {
                        p("#606c68", 57, 59, 12, 13);
                        p("#8c9c8b", 59, 56, 7, 6);
                    }
                }
            } else if (e.kind === "resource") {
                if (e.name === "deer") {
                    p("#a17e51",76,46,25,13); p("#b39264",96,38,8,16);
                    p("#c2a779",98,32,13,10); p("#a17e51",97,27,3,7); p("#a17e51",105,27,3,7);
                    p("#293f38",107,34,2,2); p("#e0d4b1",75,46,5,6);
                    for (const x of [78,84,94,99]) p("#6b573e",x,57,2,10);
                } else if (e.name.includes("berries")) {
                    p("#456e49",72,45,32,19); p("#668b54",77,38,22,19);
                    for (const [x,y] of [[76,48],[85,43],[94,49],[82,57],[100,56]]) p("#8a4c65",x,y,3,3);
                } else if (e.name.includes("fiber")) {
                    for (const x of [73,79,85,91,97]) { p("#53724b",x,42,2,23); p("#b8c88c",x-2,38,6,9); }
                } else if (e.name === "log") {
                    tree(p, 66, 28);
                    tree(p, 87, 35);
                    tree(p, 78, 18);
                } else if (e.name === "ore") {
                    p("#596f69", 72, 55, 30, 12);
                    p("#819087", 76, 43, 19, 19);
                    p("#a8b0a0", 78, 43, 11, 6);
                    p("#b5a571", 91, 52, 3, 4);
                } else if (e.name.includes("water")) {
                    p("#638e86", 76, 54, 26, 13);
                    p("#71b1bc", 78, 54, 22, 6);
                    p("#adcbd0", 82, 55, 12, 2);
                } else {
                    p("#997b4d", 66, 48, 42, 23);
                    for (let i = 0; i < 6; i++)
                        for (let j = 0; j < 2; j++) {
                            p("#cfb565", 69 + i * 6, 44 + j * 12, 2, 14);
                            p("#edce7d", 68 + i * 6, 41 + j * 12, 4, 6);
                        }
                }
            } else {
                p("#876947", 83, 55, 11, 10);
                p("#c8a675", 83, 55, 11, 3);
            }
            if (
                labels &&
                e.kind !== "item" &&
                !(e.kind === "homestead_plot" && e.cottage_built) &&
                (e.kind !== "person" || selected || e.is_player)
            ) {
                const displayName = entityName(e);
                const label =
                    displayName.length > 24
                        ? displayName.slice(0, 23) + "…"
                        : displayName;
                ctx.font = "10px monospace";
                ctx.textAlign = "center";
                const width = ctx.measureText(label).width + 10;
                p(
                    selected ? "#edd7a3" : "#1e3938ed",
                    88 - width / 2,
                    80,
                    width,
                    16,
                );
                ctx.fillStyle = selected ? "#243c37" : "#f4edce";
                ctx.fillText(label, 88, 91);
            }
        },
    });
}
