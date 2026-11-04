import type { PublicBubble, WorldEntity } from "./state.ts";

/** Category headings already identify actions; preserve the public transport text unchanged. */
export function displayBubbleText(bubble: PublicBubble): string {
    return bubble.kind === "thought" && bubble.text.startsWith("Action: ")
        ? bubble.text.slice("Action: ".length)
        : bubble.text;
}

/** Only the latest received simulation tick controls visibility, including on a stale feed. */
export function activeBubbles(entity: WorldEntity, tick: number): PublicBubble[] {
    return entity.kind === "person"
        ? entity.bubbles.filter((bubble) => bubble.tick <= tick && bubble.expires_tick > tick && bubble.text.trim())
        : [];
}

/** Complete the visible conversation before allocating space to action history. */
export function orderBubbles<T extends { id: number; is_player: boolean; selected: boolean; bubble: PublicBubble }>(candidates: readonly T[]): T[] {
    const priority = (candidate: T) => candidate.is_player ? 0 : candidate.selected ? 1 : 2;
    return [...candidates].sort((a, b) =>
        (a.bubble.kind === "speech" ? 0 : 1) - (b.bubble.kind === "speech" ? 0 : 1)
        || priority(a) - priority(b) || a.id - b.id);
}

export interface Rect { x: number; y: number; width: number; height: number }
export interface Point { x: number; y: number }
export function overlaps(a: Rect, b: Rect, gap = 3): boolean {
    return a.x < b.x + b.width + gap && a.x + a.width + gap > b.x
        && a.y < b.y + b.height + gap && a.y + a.height + gap > b.y;
}

/** Connectors are optional: omit a whole tail if any pixel block would cross another surface. */
export function bubbleTail(kind: PublicBubble["kind"], body: Rect, head: Point, bounds: Rect, occupied: Rect[]): Rect[] {
    if (head.x >= body.x && head.x <= body.x + body.width
        && head.y >= body.y && head.y <= body.y + body.height) return [];
    const edge = {
        x: Math.max(body.x + 8, Math.min(body.x + body.width - 8, head.x)),
        y: head.y >= body.y + body.height / 2 ? body.y + body.height : body.y,
    };
    const segments: Rect[] = kind === "speech"
        ? Array.from({ length: 4 }, (_, step) => ({
            x: Math.round(edge.x - 4 + step),
            y: Math.round(edge.y + (edge.y < head.y ? step * 2 : -(step + 1) * 2)),
            width: 8 - step * 2,
            height: 2,
        }))
        : [[0.25, 4], [0.55, 3], [0.8, 2]].map(([part, size]) => ({
            x: Math.round(edge.x + (head.x - edge.x) * part),
            y: Math.round(edge.y + (head.y - edge.y) * part),
            width: size,
            height: size,
        }));
    if (segments.some((segment) => segment.x < bounds.x || segment.y < bounds.y
        || segment.x + segment.width > bounds.x + bounds.width
        || segment.y + segment.height > bounds.y + bounds.height
        || occupied.some((other) => overlaps(segment, other, 1)))) return [];
    return segments;
}

/** Suppress crowded bubbles rather than covering labels, controls or leaving the viewport. */
export function placeBubble(anchor: Point, size: Pick<Rect, "width" | "height">, bounds: Rect, occupied: Rect[]): Rect | undefined {
    if (size.width > bounds.width || size.height > bounds.height
        || anchor.x < bounds.x || anchor.x > bounds.x + bounds.width
        || anchor.y < bounds.y || anchor.y > bounds.y + bounds.height) return;
    const { width, height } = size;
    const preferred = { x: anchor.x - width / 2, y: anchor.y - height - 28 };
    // Fixed strides can miss a usable gap by a few pixels. Consider the edges of only eight
    // nearby obstacles, bounding candidate count while retaining the original placement order.
    const distance = (rect: Rect) => Math.hypot(
        Math.max(rect.x - anchor.x, 0, anchor.x - rect.x - rect.width),
        Math.max(rect.y - anchor.y, 0, anchor.y - rect.y - rect.height));
    const nearby = [...occupied].sort((a, b) => distance(a) - distance(b)).slice(0, 8);
    for (const [x, y] of [
        ...Array.from({ length: 5 }, (_, index) =>
            [anchor.x - width / 2, anchor.y - height - 28 - index * (height + 10)]),
        [anchor.x - width - 22, anchor.y - height / 2],
        [anchor.x + 22, anchor.y - height / 2],
        [anchor.x - width / 2, anchor.y + 36],
        [anchor.x - width / 2, anchor.y + height + 48],
        ...nearby.flatMap((other) => [
            [preferred.x, other.y - height - 4],
            [preferred.x, other.y + other.height + 4],
            [other.x - width - 4, preferred.y],
            [other.x + other.width + 4, preferred.y],
        ]),
    ]) {
        const rect = {
            x: Math.round(Math.max(bounds.x, Math.min(bounds.x + bounds.width - width, x))),
            y: Math.round(Math.max(bounds.y, Math.min(bounds.y + bounds.height - height, y))),
            width, height,
        };
        if (!occupied.some((other) => overlaps(rect, other))) return rect;
    }
}

/** Canvas-measured wrapping also breaks long unspaced/localized text without overflow. */
export function wrapBubble(text: string, width: number, measure: (value: string) => number, maxLines = 4): string[] {
    const lines: string[] = [];
    let line = "";
    for (const word of text.trim().split(/\s+/u)) {
        const joined = line ? `${line} ${word}` : word;
        if (measure(joined) <= width) {
            line = joined;
            continue;
        }
        if (line) {
            lines.push(line);
            line = "";
        }
        for (const character of Array.from(word)) {
            if (line && measure(line + character) > width) {
                lines.push(line.trimEnd());
                line = "";
            }
            line += character;
        }
    }
    if (line) lines.push(line.trimEnd());
    if (lines.length > maxLines) {
        const last = Array.from(lines[maxLines - 1]);
        while (last.length && measure(last.join("") + "…") > width) last.pop();
        lines[maxLines - 1] = last.join("") + "…";
    }
    return lines.slice(0, maxLines);
}
