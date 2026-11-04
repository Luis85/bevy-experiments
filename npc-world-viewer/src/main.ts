import "@fontsource/silkscreen/latin-400.css";
import "./style.css";
import { parseWorldState, type WorldEntity, type WorldState } from "./state.ts";
import { WorldView } from "./world.ts";
import { activeBubbles, displayBubbleText } from "./bubbles.ts";
import { carrying, connectionMessage, cropStatus, entityName, goodName, jobName, recipeName, words } from "./presentation.ts";

function el<T extends HTMLElement = HTMLElement>(id: string): T {
    return document.getElementById(id) as T;
}
const escape = (s: string): string =>
    s.replace(
        /[&<>"']/g,
        (c) =>
            ({
                "&": "&amp;",
                "<": "&lt;",
                ">": "&gt;",
                '"': "&quot;",
                "'": "&#39;",
            })[c]!,
    );
const pretty = words;
const placeDescription = (entity: WorldEntity): string =>
    entity.kind === "building" && entity.role === "water_keeper"
        ? "Well & water supply"
        : "Workplace & housing";
function ownerName(entity: Extract<WorldEntity,{kind:"homestead_plot" | "cottage"}>): string {
    if (entity.owner === null) return "Unclaimed";
    return entity.owner_name ?? state?.entities.find(e=>e.kind === "person" && e.id === entity.owner)?.name ?? `Actor #${entity.owner}`;
}
let state: WorldState | undefined;
let selectedId: number | undefined;
let filter = "all";
let follow = false;
let labels = true;
let revision = "";
let directoryKey = "";
let stopped = false;
let lastWrite = 0;

function syncFollow(value: boolean): void {
    follow = value;
    el("follow").setAttribute("aria-pressed", String(value));
}
const view = new WorldView(
    el<HTMLCanvasElement>("world"),
    (entity) => {
        selectedId = entity?.id;
        detail(entity);
        directory(true);
        // Selection is explicit user navigation; polling must never move the page.
        if (entity) {
            if (window.matchMedia("(max-width: 800px)").matches)
                el("inspector").scrollIntoView({block:"start"});
            else document.querySelector("aside")!.scrollTo({top:0});
        }
    },
    () => syncFollow(false),
);

function detail(entity?: WorldEntity): void {
    el("selection-hint").hidden = !!entity;
    if (!entity) {
        el("detail").replaceChildren();
        return;
    }
    let html = `<div class="selected-name"><span class="entity-symbol ${entity.kind}"></span><div><h3>${escape(entityName(entity))}</h3><p>${entity.kind === "person" ? (entity.is_player ? "Played character · " : "") + escape(pretty(entity.role)) : entity.kind === "building" ? escape(pretty(entity.role)) + " · " + escape(placeDescription(entity)) : entity.kind === "homestead_plot" ? entity.use_kind === "housing" ? "House plot" : "Garden plot" : escape(pretty(entity.kind))}</p></div></div>`;
    html += `<dl><div><dt>Position</dt><dd>${(entity.position.x / 1000).toFixed(1)}, ${(entity.position.z / 1000).toFixed(1)} m</dd></div><div><dt>Stable ID</dt><dd>${entity.id}</dd></div>`;
    if (entity.kind === "person") {
        html += `<div><dt>Activity</dt><dd>${escape(pretty(entity.activity))}</dd></div>${entity.decision ? `<div class="decision"><dt>Current plan</dt><dd>${escape(jobName(entity.decision))}</dd></div>` : ""}<div><dt>Coins</dt><dd>${entity.coins.toLocaleString()}</dd></div><div><dt>Home</dt><dd>${(entity.home.x / 1000).toFixed(1)}, ${(entity.home.z / 1000).toFixed(1)} m</dd></div></dl>`;
        const meters = [
            ["Health", entity.health],
            ...Object.entries(entity.needs).map(([key, need]) => [
                pretty(key),
                need.value,
            ]),
        ] as [string, number][];
        html += meters
            .map(
                ([key, value]) =>
                    `<label class="need"><span>${escape(key)}<strong>${Math.round(value / 1000)}%</strong></span><meter min="0" max="100000" low="30000" high="60000" optimum="100000" value="${value}">${Math.round(value / 1000)}%</meter></label>`,
            )
            .join("");
        const pack = carrying(entity.inventory);
        html += `<h4>Carrying</h4>${pack.length ? `<ul class="inventory-list">${pack.map(item=>`<li><span>${escape(item.label)}</span><strong>×${item.count.toLocaleString()}</strong></li>`).join("")}</ul>` : '<p class="inventory">Empty inventory</p>'}`;
        if (state?.world.economy === "needs_driven") html += `<h4>Cold protection</h4><p>${entity.wearing_warm_clothes ? "Warm clothes equipped" : "No warm clothes equipped"}</p>`;
    } else if (entity.kind === "resource") {
        html += `<div><dt>Available</dt><dd>${entity.remaining} / ${entity.capacity}</dd></div>`;
        if (entity.gather_ticks !== undefined) html += `<div><dt>${entity.name === "deer" ? "Hunt" : "Gather"} duration</dt><dd>${entity.gather_ticks} ticks</dd></div>`;
        if (entity.replenish_at !== undefined) html += `<div><dt>Next renewal</dt><dd>Tick ${entity.replenish_at} · ${Math.max(0, entity.replenish_at - (state?.tick ?? 0))} ticks away</dd></div>`;
        html += `</dl>`;
    }
    else if (entity.kind === "building") {
        const residents =
            state?.entities.filter(
                (e) => e.kind === "person" && e.workplace === entity.id,
            ) ?? [];
        html += `</dl><h4>Resident</h4><p>${residents.length ? residents.map((e) => escape(e.name)).join(", ") : "No resident assigned"}</p>`;
        if (entity.recipes.length) html += `<h4>Production</h4>${entity.recipes.map(recipe => `<p class="recipe"><strong>${escape(recipeName(recipe.name))}</strong><small>${recipe.duration_ticks} ticks</small><span>${recipe.inputs.map(input => `${escape(goodName(input.kind))} ×${input.count}`).join(" + ")} → ${recipe.outputs.map(output => `${escape(goodName(output.kind))} ×${output.count}`).join(" + ")}</span></p>`).join("")}`;
    } else if (entity.kind === "homestead_plot") {
        html += `<div><dt>Owner</dt><dd>${escape(ownerName(entity))}</dd></div>`;
        if (entity.use_kind === "housing") {
            html += `<div><dt>Home</dt><dd>${entity.cottage_built ? "Cottage built" : "Not built"}</dd></div>`;
            if (entity.cottage_built) html += `<div><dt>Front door</dt><dd>${(entity.front_door.x/1000).toFixed(1)}, ${(entity.front_door.z/1000).toFixed(1)} m</dd></div>`;
        }
        html += `</dl><h4>Garden beds</h4>`;
        html += entity.beds.length ? `<ol class="garden-beds">${entity.beds.map(bed=>`<li data-bed-index="${bed.index}"><strong>Bed ${bed.index+1}${bed.kind ? ` · ${escape(goodName(bed.kind))}` : ""}</strong><span class="crop-status stage-${bed.stage}">${escape(cropStatus(bed,state?.tick??0))}</span>${bed.planted_at !== null || bed.watered_at !== null || bed.ready_at !== null ? `<dl class="crop-times">${bed.planted_at !== null ? `<div><dt>Planted</dt><dd>Tick ${bed.planted_at.toLocaleString()}</dd></div>` : ""}${bed.watered_at !== null ? `<div><dt>Watered</dt><dd>Tick ${bed.watered_at.toLocaleString()}</dd></div>` : ""}${bed.ready_at !== null ? `<div><dt>Ready at</dt><dd>Tick ${bed.ready_at.toLocaleString()}</dd></div>` : ""}</dl>` : ""}</li>`).join("")}</ol>` : '<p class="inventory">No beds in this snapshot.</p>';
    } else if (entity.kind === "cottage") {
        html += `<div><dt>Owner</dt><dd>${escape(ownerName(entity))}</dd></div>`;
        const plot = state?.entities.find(e=>e.kind === "homestead_plot" && e.plot_id === entity.plot_id);
        if (plot) html += `<div><dt>House plot</dt><dd>${escape(plot.name)}</dd></div>`;
        html += `</dl><p>A built village home. The map reflects the actual cottage and garden state.</p>`;
    } else html += `<div><dt>Quantity</dt><dd>${entity.count}</dd></div></dl>`;
    el("detail").innerHTML = html;
    const feedback = activeBubbles(entity, state?.tick ?? 0);
    if (feedback.length) {
        const section = document.createElement("section");
        section.className = "public-feedback";
        const heading = document.createElement("h4");
        heading.textContent = "Recent public feedback";
        section.append(heading);
        for (const bubble of feedback) {
            const line = document.createElement("p");
            const label = document.createElement("strong");
            label.textContent = `${bubble.kind === "speech" ? "Spoken dialogue" : "Action"} · tick ${bubble.tick}`;
            line.append(label, document.createTextNode(displayBubbleText(bubble)));
            section.append(line);
        }
        el("detail").append(section);
    }
}

function directory(force = false): void {
    if (!state) return;
    const query = el<HTMLInputElement>("search").value.toLowerCase();
    const entries = state.entities.filter(
        (e) =>
            e.kind !== "item" &&
            (filter === "all" || e.kind === filter || (filter === "building" && (e.kind === "homestead_plot" || e.kind === "cottage"))) &&
            `${entityName(e)} ${e.name} ${"role" in e ? pretty(e.role) : e.kind} ${"owner_name" in e ? e.owner_name??"" : ""}`
                .toLowerCase()
                .includes(query),
    );
    const key = JSON.stringify([
        entries.map((e) => [
            e.id,
            e.name,
            e.kind,
            e.kind === "person" ? e.activity : "",
            e.kind === "person" && e.is_player,
            e.kind === "homestead_plot" || e.kind === "cottage" ? e.owner : "",
            e.kind === "homestead_plot" || e.kind === "cottage" ? e.owner_name : "",
            e.kind === "homestead_plot" && e.cottage_built,
        ]),
        selectedId,
    ]);
    if (!force && directoryKey === key) return;
    directoryKey = key;
    const focused =
        document.activeElement instanceof HTMLButtonElement
            ? document.activeElement.dataset.id
            : undefined;
    el("directory").innerHTML = entries.length
        ? entries
              .map(
                  (e) =>
                      `<button class="directory-row ${e.id === selectedId ? "selected" : ""}" data-id="${e.id}" aria-pressed="${e.id === selectedId}"><span class="entity-symbol ${e.kind}"></span><span><strong>${escape(entityName(e))}</strong><small>${escape(e.kind === "person" ? pretty(e.role) : e.kind === "building" ? placeDescription(e) : e.kind === "homestead_plot" ? `${e.use_kind === "housing" ? "House plot" : "Garden plot"} · ${ownerName(e)}` : e.kind === "cottage" ? `Home · ${ownerName(e)}` : e.name === "deer" ? "Wildlife" : "Resource patch")}</small></span>${e.kind === "person" && e.is_player ? '<span class="player-tag">You</span>' : e.kind === "person" ? `<span class="activity">${escape(pretty(e.activity))}</span>` : e.kind === "homestead_plot" && e.cottage_built ? '<span class="activity">Built</span>' : ""}</button>`,
              )
              .join("")
        : '<p class="no-results">No matching residents or places.</p>';
    if (focused)
        el("directory")
            .querySelector<HTMLButtonElement>(`[data-id="${focused}"]`)
            ?.focus({ preventScroll: true });
    el("entity-count").textContent = String(entries.length);
}
el("directory").addEventListener("click", (e) => {
    const button = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "[data-id]",
    );
    if (button) {
        view.follow(false);
        syncFollow(false);
        view.select(Number(button.dataset.id), true);
    }
});
el("search").addEventListener("input", () => directory());
document
    .querySelectorAll<HTMLButtonElement>("[data-filter]")
    .forEach((button) =>
        button.addEventListener("click", () => {
            filter = button.dataset.filter!;
            document
                .querySelectorAll("[data-filter]")
                .forEach((b) =>
                    b.setAttribute("aria-pressed", String(b === button)),
                );
            directory();
        }),
    );
el("zoom-in").addEventListener("click", () => view.zoom(1.25));
el("zoom-out").addEventListener("click", () => view.zoom(0.8));
el("fit").addEventListener("click", () => view.fit());
el("follow").addEventListener("click", () => {
    syncFollow(!follow);
    view.follow(follow);
    const player = state?.entities.find(
        (e) => e.kind === "person" && e.is_player,
    );
    if (follow && player) view.select(player.id);
});
el("labels").addEventListener("click", () => {
    labels = !labels;
    el("labels").setAttribute("aria-pressed", String(labels));
    view.showLabels(labels);
});

function connection(status: string, message: string, warning = false): void {
    el("status").textContent = status;
    el("updated").textContent = message;
    el("signal").classList.toggle("warning", warning);
}
async function poll(): Promise<void> {
    try {
        const response = await fetch("/api/world-state", {
            cache: "no-store",
            signal: AbortSignal.timeout(4000),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const result = await response.json();
        if (result.state && result.revision !== revision) {
            const next = parseWorldState(result.state);
            view.setState(next);
            state = next;
            revision = result.revision;
            el("world-name").textContent = state.world.name;
            document.title = `${state.world.name} · NPC World Viewer`;
            el("empty").hidden = true;
            el("plot-legend").hidden = state.terrain.expansion_plots.length === 0;
            el("plot-legend").textContent = state.entities.some(e=>e.kind === "homestead_plot" && e.owner !== null)
                ? "House plots and gardens · select a site to inspect"
                : "Open plots · house sites and gardens";
            el("world").classList.toggle("night", state.world.night);
            el("tick").textContent = `Tick ${state.tick.toLocaleString()}`;
            el("calendar").textContent =
                `Day ${state.world.day + 1} · ${pretty(state.world.season)} · ${state.world.night ? "Night" : "Daylight"}`;
            el("population").textContent =
                `${state.entities.filter((e) => e.kind === "person" && e.activity !== "dead").length} people`;
            el("market-panel").hidden = state.world.economy !== "needs_driven";
            if (state.world.economy === "needs_driven") {
                const active = state.market.filter(row=>row.funded_demand>0 || row.below_reference_bid_units>0 || row.offered_supply>0 || row.total_traded_units>0);
                el("market-goods").innerHTML = active.length ? `<table><caption class="visually-hidden">Current public prices, goods for sale and funded purchase interest</caption><thead><tr><th scope="col">Good</th><th scope="col">Coins</th><th scope="col">For sale</th><th scope="col">Demand</th></tr></thead><tbody>${active.map(row=>`<tr><th scope="row">${escape(goodName(row.kind))}</th><td>${row.unit_price}${row.max_affordable_bid ? `<small>cap ${row.max_affordable_bid}</small>` : ""}</td><td>${row.offered_supply}</td><td>${row.funded_demand}${row.below_reference_bid_units ? `<small>${row.below_reference_bid_units} lower bids</small>` : ""}</td></tr>`).join("")}</tbody></table><p class="market-note">Demand can pay the current quote. Lower bids wait for a better price; caps are conditional.</p>` : '<p class="no-results">No public offers or purchase demand in this snapshot.</p>';
            }
            detail(state.entities.find((e) => e.id === selectedId));
            directory();
        }
        lastWrite = Date.parse(result.updated_at ?? "") || 0;
        const age = Math.max(0, Math.floor((Date.now() - lastWrite) / 1000));
        if (result.error) {
            const summary = connectionMessage(!!state,"invalid");
            connection(summary.status,summary.message,true);
        } else if (age > 5) {
            const summary = connectionMessage(!!state,"stale",age);
            connection(summary.status,summary.message,true);
        }
        else
            connection(
                "Following world state",
                `File updated ${age}s ago · read-only`,
            );
        el("source").textContent =
            `${result.source ?? "World state"} · v1 projection`;
    } catch (error) {
        const summary = connectionMessage(!!state,"disconnected");
        connection(summary.status,summary.message,true);
        console.warn("World-state update failed", error);
    } finally {
        if (!stopped)
            setTimeout(() => {
                void poll();
            }, 500);
    }
}
window.addEventListener("pagehide", () => {
    stopped = true;
});
try {
    await view.start();
    void poll();
} catch (error) {
    connection(
        "Renderer could not start",
        "This browser could not initialize Excalibur. Try a browser with WebGL support.",
        true,
    );
    console.error(error);
}
