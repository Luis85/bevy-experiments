import {
    Actor,
    Canvas,
    Color,
    DisplayMode,
    Engine,
    vec,
    type Vector,
} from "excalibur";
import { entityGraphic, PX_PER_MM, terrainGraphic } from "./art.ts";
import { reconcile, type ExpansionPlot, type PublicBubble, type WorldEntity, type WorldState } from "./state.ts";
import { activeBubbles, bubbleTail, displayBubbleText, orderBubbles, overlaps, placeBubble, wrapBubble, type Rect } from "./bubbles.ts";
import { entityName, landBounds } from "./presentation.ts";

class ProjectionActor extends Actor {
    data: WorldEntity;
    target: Vector;
    private graphicKey = "";
    site?: ExpansionPlot;
    constructor(entity: WorldEntity) {
        const position = vec(
            entity.position.x * PX_PER_MM,
            entity.position.z * PX_PER_MM,
        );
        super({ pos: position, width: 28, height: 30 });
        this.target = position;
        this.data = entity;
    }
    project(
        entity: WorldEntity,
        selected: boolean,
        labels: boolean,
        snap: boolean,
        site?: ExpansionPlot,
    ): void {
        this.data = entity;
        this.site = site;
        this.target = vec(
            entity.position.x * PX_PER_MM,
            entity.position.z * PX_PER_MM,
        );
        if (
            snap ||
            entity.kind !== "person" ||
            this.pos.distance(this.target) > 200
        )
            this.pos = this.target.clone();
        const key = JSON.stringify([entity.name,entity.kind,"role" in entity ? entity.role : "",
            entity.kind === "person" ? [entity.activity,entity.is_player,entity.wearing_warm_clothes] : "",
            entity.kind === "homestead_plot" ? [entity.owner,entity.cottage_built,entity.beds] : "",site,selected,labels]);
        if (key !== this.graphicKey) {
            this.graphics.use(entityGraphic(entity, selected, labels,site));
            this.graphicKey = key;
        }
        this.z = entity.kind === "homestead_plot" ? -5000 : 1000 + this.pos.y;
    }
    onPostUpdate(_engine: Engine, elapsed: number): void {
        // Presentation smoothing stops at the last received position; no extrapolated NPC behavior.
        const alpha = Math.min(1, elapsed / 90);
        this.pos = this.pos.lerp(this.target, alpha);
        this.z = this.data.kind === "homestead_plot" ? -5000 : 1000 + this.pos.y;
    }
}

export class WorldView {
    readonly engine: Engine;
    private actors = new Map<number, ProjectionActor>();
    private terrain?: Actor;
    private terrainKey = "";
    private boundsKey = "";
    private state?: WorldState;
    private selected?: number;
    private labels = true;
    private following = false;
    private fitted = false;
    private fittedZoom = 1;
    private feedbackCanvas: HTMLCanvasElement;
    constructor(
        canvas: HTMLCanvasElement,
        private onSelect: (entity?: WorldEntity) => void,
        private onManualPan: () => void,
    ) {
        this.engine = new Engine({
            canvasElement: canvas,
            displayMode: DisplayMode.FillContainer,
            backgroundColor: Color.fromHex("#235a6e"),
            antialiasing: false,
            pixelArt: true,
            suppressConsoleBootMessage: true,
            suppressPlayButton: true,
            maxFps: 60,
        });
        this.feedbackCanvas = document.createElement("canvas");
        this.feedbackCanvas.className = "bubble-overlay";
        this.feedbackCanvas.setAttribute("aria-hidden", "true");
        canvas.after(this.feedbackCanvas);
        new ResizeObserver(() => {
            // Excalibur sizes its screen first; refit once for the new device layout.
            requestAnimationFrame(() => {
                if (this.state) this.fit(false);
            });
        }).observe(canvas.parentElement!);
        let drag:
            | {
                  x: number;
                  y: number;
                  startX: number;
                  startY: number;
                  moved: boolean;
              }
            | undefined;
        canvas.addEventListener("pointerdown", (e) => {
            if (e.button !== 0) return;
            canvas.focus();
            canvas.setPointerCapture(e.pointerId);
            drag = {
                x: e.clientX,
                y: e.clientY,
                startX: e.clientX,
                startY: e.clientY,
                moved: false,
            };
        });
        canvas.addEventListener("pointermove", (e) => {
            if (!drag) return;
            if (
                Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY) > 4
            )
                drag.moved = true;
            if (drag.moved) {
                const camera = this.engine.currentScene.camera;
                const ratio =
                    this.engine.screen.resolution.width /
                    canvas.getBoundingClientRect().width;
                camera.pos = camera.pos.sub(
                    vec(
                        ((e.clientX - drag.x) * ratio) / camera.zoom,
                        ((e.clientY - drag.y) * ratio) / camera.zoom,
                    ),
                );
                this.follow(false);
                this.onManualPan();
            }
            drag.x = e.clientX;
            drag.y = e.clientY;
        });
        canvas.addEventListener("pointerup", (e) => {
            if (drag && !drag.moved) {
                const bounds = canvas.getBoundingClientRect();
                const point = this.engine.screenToWorldCoordinates(
                    vec(
                        ((e.clientX - bounds.left) *
                            this.engine.screen.resolution.width) /
                            bounds.width,
                        ((e.clientY - bounds.top) *
                            this.engine.screen.resolution.height) /
                            bounds.height,
                    ),
                );
                const nearest = [...this.actors.values()]
                    .map((a) => ({ actor: a, distance: a.pos.distance(point),
                        inPlot:a.data.kind === "homestead_plot" && !!a.site
                            && Math.abs(point.x-a.site.x*PX_PER_MM)<=a.site.width_mm*PX_PER_MM/2
                            && Math.abs(point.y-a.site.z*PX_PER_MM)<=a.site.depth_mm*PX_PER_MM/2 }))
                    .filter(
                        (a) =>
                            a.inPlot || a.distance <
                            (a.actor.data.kind === "building" || a.actor.data.kind === "cottage" ? 40 : 22),
                    )
                    .sort((a, b) => Number(a.actor.data.kind === "homestead_plot")-Number(b.actor.data.kind === "homestead_plot") || a.distance - b.distance)[0];
                this.select(nearest?.actor.data.id);
            }
            drag = undefined;
        });
        canvas.addEventListener("pointercancel", () => {
            drag = undefined;
        });
        canvas.addEventListener(
            "wheel",
            (e) => {
                e.preventDefault();
                this.zoom(e.deltaY < 0 ? 1.15 : 1 / 1.15);
            },
            { passive: false },
        );
        canvas.addEventListener("keydown", (e) => {
            const direction: Record<string, [number, number]> = {
                ArrowLeft: [-1, 0],
                ArrowRight: [1, 0],
                ArrowUp: [0, -1],
                ArrowDown: [0, 1],
            };
            if (e.key in direction) {
                e.preventDefault();
                const [x, y] = direction[e.key];
                this.engine.currentScene.camera.pos =
                    this.engine.currentScene.camera.pos.add(
                        vec(
                            (x * 40) / this.engine.currentScene.camera.zoom,
                            (y * 40) / this.engine.currentScene.camera.zoom,
                        ),
                    );
                this.follow(false);
                this.onManualPan();
            }
            if (e.key === "+" || e.key === "=") this.zoom(1.2);
            if (e.key === "-") this.zoom(1 / 1.2);
            if (e.key === "0") this.fit();
            if (e.key === "Escape") this.select(undefined);
        });
        this.engine.on("postupdate", () => {
            if (this.following) {
                const player = [...this.actors.values()].find(
                    (a) => a.data.kind === "person" && a.data.is_player,
                );
                if (player)
                    this.engine.currentScene.camera.pos = player.pos.clone();
            }
            this.drawFeedback(canvas);
        });
    }
    async start(): Promise<void> {
        await this.engine.start();
    }
    setState(state: WorldState): void {
        const reset =
            !!this.state &&
            (state.world.id !== this.state.world.id ||
                state.tick < this.state.tick);
        const key = JSON.stringify([state.terrain, state.world.season]);
        const boundsKey = JSON.stringify(landBounds(state.terrain));
        const boundsChanged = boundsKey !== this.boundsKey;
        this.boundsKey = boundsKey;
        if (key !== this.terrainKey) {
            const graphic: Canvas = terrainGraphic(state);
            this.terrain?.kill();
            this.terrain = new Actor({
                pos: vec(
                    (state.terrain.origin.x +
                        (state.terrain.width * state.terrain.cell_size_mm) /
                            2) *
                        PX_PER_MM,
                    (state.terrain.origin.z +
                        (state.terrain.height * state.terrain.cell_size_mm) /
                            2) *
                        PX_PER_MM,
                ),
                z: -10000,
            });
            this.terrain.graphics.use(graphic);
            this.engine.add(this.terrain);
            this.terrainKey = key;
        }
        this.state = state;
        reconcile(
            this.actors,
            state.entities,
            (entity) => {
                const actor = new ProjectionActor(entity);
                this.engine.add(actor);
                return actor;
            },
            (actor, entity) =>
                actor.project(
                    entity,
                    this.selected === entity.id,
                    this.labels,
                    reset,
                    "plot_id" in entity ? state.terrain.expansion_plots.find(plot=>plot.id === entity.plot_id) : undefined,
                ),
            (actor) => actor.kill(),
        );
        if (this.selected !== undefined && !this.actors.has(this.selected))
            this.select(undefined);
        if (!this.fitted || reset || boundsChanged) {
            this.fit();
            this.fitted = true;
        }
    }
    select(id?: number, center = false): void {
        this.selected = id;
        const actor = id === undefined ? undefined : this.actors.get(id);
        if (center && actor)
            this.engine.currentScene.camera.pos = actor.pos.clone();
        for (const value of this.actors.values())
            value.project(value.data, value.data.id === id, this.labels, false,value.site);
        this.onSelect(actor?.data);
    }
    showLabels(value: boolean): void {
        this.labels = value;
        this.select(this.selected);
    }
    follow(value: boolean): void {
        this.following = value;
    }
    private drawFeedback(canvas: HTMLCanvasElement): void {
        const viewport = canvas.getBoundingClientRect();
        const width = viewport.width, height = viewport.height;
        const density = window.devicePixelRatio || 1;
        const overlay = this.feedbackCanvas;
        if (overlay.width !== Math.round(width * density) || overlay.height !== Math.round(height * density)) {
            overlay.width = Math.round(width * density);
            overlay.height = Math.round(height * density);
        }
        const ctx = overlay.getContext("2d");
        if (!ctx || !width || !height) return;
        ctx.setTransform(density, 0, 0, density, 0, 0);
        ctx.clearRect(0, 0, width, height);
        if (!this.state || width < 160 || height < 120) return;
        const scaleX = width / this.engine.screen.resolution.width;
        const scaleY = height / this.engine.screen.resolution.height;
        const point = (actor: ProjectionActor) => {
            const screen = this.engine.worldToScreenCoordinates(actor.pos);
            return { x: screen.x * scaleX, y: screen.y * scaleY };
        };
        const occupied: Rect[] = [];
        // The overlay never covers the incumbent calendar, controls or exploration hint.
        for (const element of canvas.parentElement!.querySelectorAll<HTMLElement>(".map-heading, .map-tools, .map-footer, .plot-legend")) {
            const rect = element.getBoundingClientRect();
            if (rect.width && rect.height) occupied.push({ x: rect.x - viewport.x, y: rect.y - viewport.y, width: rect.width, height: rect.height });
        }
        const zoom = this.engine.currentScene.camera.zoom;
        ctx.font = "10px monospace";
        if (this.labels) for (const actor of this.actors.values()) {
            const entity = actor.data;
            if (entity.kind === "item" || (entity.kind === "person" && !entity.is_player && entity.id !== this.selected)) continue;
            if (entity.kind === "homestead_plot" && entity.cottage_built) continue;
            const anchor = point(actor);
            const name = entityName(entity);
            const label = name.length > 24 ? name.slice(0, 23) + "…" : name;
            const labelWidth = (ctx.measureText(label).width + 10) * zoom * scaleX;
            occupied.push({ x: anchor.x - labelWidth / 2, y: anchor.y + 24 * zoom * scaleY, width: labelWidth, height: 16 * zoom * scaleY });
        }
        const candidates = orderBubbles([...this.actors.values()].flatMap((actor) =>
            activeBubbles(actor.data, this.state!.tick).map((bubble) => ({
                actor, bubble, id: actor.data.id,
                is_player: actor.data.kind === "person" && actor.data.is_player,
                selected: actor.data.id === this.selected,
            })),
        ));
        ctx.save();
        ctx.beginPath();
        ctx.rect(0, 0, width, height);
        ctx.clip();
        ctx.imageSmoothingEnabled = false;
        ctx.font = "11px monospace";
        ctx.textBaseline = "top";
        const bounds = { x: 8, y: 8, width: width - 16, height: height - 16 };
        const placed: { actor: ProjectionActor; bubble: PublicBubble; rect: Rect; lines: string[]; head: { x: number; y: number } }[] = [];
        for (const { actor, bubble } of candidates) {
            if (placed.length === 32) break;
            const anchor = point(actor);
            const bubbleWidth = Math.min(220, width - 16);
            const lines = wrapBubble(displayBubbleText(bubble), bubbleWidth - 20, (text) => ctx.measureText(text).width);
            const rect = placeBubble(anchor, { width: bubbleWidth, height: 30 + lines.length * 14 }, bounds, occupied);
            if (!rect) continue;
            occupied.push(rect);
            placed.push({ actor, bubble, rect, lines, head: { x: anchor.x, y: anchor.y - 18 * zoom * scaleY } });
        }
        // Resolve connectors only after all body rectangles are fixed. A tail may disappear,
        // but it cannot change body placement or cross any later-placed bubble.
        const connectors: Rect[] = [];
        for (const { actor, bubble, rect, lines, head } of placed) {
            const speech = bubble.kind === "speech";
            const background = speech ? "#faf8f0" : "#e4eaca";
            const tail = bubbleTail(bubble.kind, rect, head, bounds, [...occupied.filter((other) => other !== rect), ...connectors]);
            connectors.push(...tail);
            ctx.fillStyle = background;
            for (const segment of tail) ctx.fillRect(segment.x, segment.y, segment.width, segment.height);
            ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
            ctx.strokeStyle = "#315e4e";
            ctx.lineWidth = 1;
            ctx.strokeRect(rect.x + 0.5, rect.y + 0.5, rect.width - 1, rect.height - 1);
            ctx.fillStyle = "#315e4e";
            const name = Array.from(actor.data.name.replace(/\s+/gu, " ")).slice(0, 20).join("");
            const heading = wrapBubble(`${name}: ${speech ? "says" : "action"}`, rect.width - 20, (text) => ctx.measureText(text).width, 1)[0];
            ctx.fillText(heading ?? "", rect.x + 10, rect.y + 7);
            ctx.fillStyle = "#263d38";
            lines.forEach((line, index) => ctx.fillText(line, rect.x + 10, rect.y + 23 + index * 14));
        }
        // Public action feedback and actor names take precedence over quiet site labels.
        if (this.labels) {
            ctx.font = "10px monospace";
            for (const plot of this.state.terrain.expansion_plots) {
                if (this.state.entities.some(entity=>entity.kind === "homestead_plot" && entity.plot_id === plot.id)) continue;
                const center = this.engine.worldToScreenCoordinates(vec(plot.x*PX_PER_MM,plot.z*PX_PER_MM));
                const text = width < 600 ? plot.label.replace(/\bplot\s*/i, "").replace(/\s+(\d+)$/, "$1").trim() : plot.label;
                const label = wrapBubble(text, width < 600 ? 52 : 110, text=>ctx.measureText(text).width,1)[0];
                if (!label) continue;
                const box = {x:Math.round(center.x*scaleX-(ctx.measureText(label).width+8)/2),
                    y:Math.round(center.y*scaleY + plot.depth_mm*PX_PER_MM*zoom*scaleY/2 + 3),
                    width:Math.ceil(ctx.measureText(label).width+8),height:16};
                if (box.x < 5 || box.y < 5 || box.x+box.width > width-5 || box.y+box.height > height-5
                    || occupied.some(other=>overlaps(box,other)) || connectors.some(other=>overlaps(box,other))) continue;
                occupied.push(box);
                ctx.fillStyle = "#faf8f0e8"; ctx.fillRect(box.x,box.y,box.width,box.height);
                ctx.fillStyle = "#315e4e"; ctx.fillText(label,box.x+4,box.y+3);
            }
            ctx.font = '600 11px "Segoe UI", sans-serif';
            for (const district of this.state.terrain.districts) {
                const point = this.engine.worldToScreenCoordinates(vec(district.x*PX_PER_MM,district.z*PX_PER_MM));
                const label = wrapBubble(district.label,160,text=>ctx.measureText(text).width,1)[0];
                if (!label) continue;
                const w = Math.ceil(ctx.measureText(label).width + 12);
                const box = [-24,-44,4,24,-64,44,-84,64].map(offset=>({x:Math.round(point.x*scaleX-w/2),y:Math.round(point.y*scaleY+offset),width:w,height:19}))
                    .find(box=>box.x>=5 && box.y>=5 && box.x+box.width<=width-5 && box.y+box.height<=height-5
                        && !occupied.some(other=>overlaps(box,other)) && !connectors.some(other=>overlaps(box,other)));
                if (!box) continue;
                occupied.push(box);
                ctx.fillStyle = "#faf8f0df"; ctx.fillRect(box.x,box.y,box.width,box.height);
                ctx.fillStyle = "#315e4e"; ctx.fillText(label,box.x+6,box.y+3);
            }
        }
        ctx.restore();
    }
    zoom(factor: number): void {
        const camera = this.engine.currentScene.camera;
        camera.zoom = Math.max(0.25, Math.min(5, camera.zoom * factor));
    }
    fit(releaseFollow = true): void {
        if (!this.state) return;
        const t = this.state.terrain;
        const bounds = landBounds(t);
        if (!bounds) return;
        const width = (bounds.maxX-bounds.minX)*PX_PER_MM;
        const height = (bounds.maxZ-bounds.minZ)*PX_PER_MM;
        this.engine.currentScene.camera.pos = vec(
            (bounds.minX+bounds.maxX)/2*PX_PER_MM,
            (bounds.minZ+bounds.maxZ)/2*PX_PER_MM,
        );
        this.fittedZoom = Math.min(
            this.engine.screen.resolution.width / (width + 100),
            this.engine.screen.resolution.height / (height + 150),
        );
        this.engine.currentScene.camera.zoom = Math.max(
            0.25,
            Math.min(3, this.fittedZoom),
        );
        if (releaseFollow) {
            this.follow(false);
            this.onManualPan();
        }
    }
}
