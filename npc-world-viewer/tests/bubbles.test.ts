import { test } from "node:test";
import assert from "node:assert/strict";
import { activeBubbles, bubbleTail, displayBubbleText, orderBubbles, overlaps, placeBubble, wrapBubble, type Rect } from "../src/bubbles.ts";
import { parseWorldState, reconcile, type Person, type WorldEntity } from "../src/state.ts";
import { example } from "./fixture.ts";

const speech = { kind: "speech", text: "Hello, island.", tick: 90, expires_tick: 120 };

test("thought display removes one exact Action prefix while speech and transport remain unchanged", () => {
    const thought = { kind: "thought" as const, text: "Action: talk", tick: 90, expires_tick: 120 };
    assert.equal(displayBubbleText(thought), "talk");
    assert.equal(thought.text, "Action: talk");
    assert.equal(displayBubbleText({ ...thought, text: "Action: Action: talk" }), "Action: talk");
    assert.equal(displayBubbleText({ ...thought, text: "Action:talk" }), "Action:talk");
    assert.equal(displayBubbleText({ ...thought, kind: "speech", text: "Action: talk" }), "Action: talk");
});
function snapshot(tick = 100, bubbles: unknown = [speech]) {
    return parseWorldState({ ...example(tick), entities: [{
        id: 7, name: "Herp", kind: "person", role: "visitor", position: { x: 0, z: 0 },
        home: { x: 0, z: 0 }, workplace: 1, is_player: true, activity: "idle",
        health: 100000, coins: 0, needs: {}, inventory: [],
        ...(bubbles === "legacy" ? {} : { bubbles }),
    }] });
}

test("schema-1 people without feedback keep an empty default and Unicode uses character bounds", () => {
    const old = snapshot(100, "legacy").entities[0] as Person;
    assert.deepEqual(old.bubbles, []);
    const unicode = snapshot(100, [{ ...speech, text: "🙂".repeat(512), private_reasoning: "must be discarded" }]).entities[0] as Person;
    assert.equal(Array.from(unicode.bubbles[0].text).length, 512);
    assert.equal("private_reasoning" in unicode.bubbles[0], false);
});

test("malformed, excessive and unsafe feedback invalidates the full replacement", () => {
    for (const bubbles of [
        null, {}, [speech, speech, speech], [{ ...speech, kind: "private_reasoning" }],
        [{ ...speech, text: "x".repeat(513) }], [{ ...speech, tick: -1 }],
        [{ ...speech, expires_tick: Number.MAX_SAFE_INTEGER + 1 }],
        [{ ...speech, tick: 0.5 }], [{ ...speech, expires_tick: 89 }],
    ]) assert.throws(() => snapshot(100, bubbles));
});

test("feedback expires only at the latest received simulation tick and never invents future events", () => {
    const actor = snapshot().entities[0];
    assert.equal(activeBubbles(actor, 90).length, 1);
    assert.equal(activeBubbles(actor, 119).length, 1);
    assert.equal(activeBubbles(actor, 120).length, 0);
    assert.equal(activeBubbles(actor, 89).length, 0);
    // Repeated/stale reads at the same tick are identical; wall time is not a simulation clock.
    assert.deepEqual(activeBubbles(actor, 100), activeBubbles(actor, 100));
    assert.deepEqual(activeBubbles(example().entities[0], 100), []);
});

test("replacement, reset and despawn remove old feedback without mutating surviving identity", () => {
    const existing = new Map<number, { data: WorldEntity }>();
    const update = (entities: WorldEntity[]) => reconcile(existing, entities,
        (data) => ({ data }), (value, data) => { value.data = data; }, () => {});
    update(snapshot().entities);
    const original = existing.get(7)!;
    assert.equal(activeBubbles(original.data, 100).length, 1);
    update(snapshot(101, []).entities);
    assert.equal(existing.get(7), original);
    assert.deepEqual(activeBubbles(original.data, 101), []);
    update(snapshot(0, "legacy").entities);
    assert.deepEqual(activeBubbles(original.data, 0), []);
    update([]);
    assert.equal(existing.size, 0);
});

test("pixel bubble placement avoids labels, HUD and one another at narrow viewport bounds", () => {
    const bounds = { x: 8, y: 8, width: 374, height: 420 };
    const occupied: Rect[] = [{ x: 12, y: 12, width: 350, height: 26 }, { x: 12, y: 350, width: 350, height: 68 }];
    for (const size of [{ width: 220, height: 86 }, { width: 220, height: 58 }]) {
        const placed = placeBubble({ x: 190, y: 270 }, size, bounds, occupied);
        assert.ok(placed);
        assert.ok(placed.x >= bounds.x && placed.y >= bounds.y);
        assert.ok(placed.x + placed.width <= bounds.x + bounds.width);
        assert.ok(placed.y + placed.height <= bounds.y + bounds.height);
        assert.ok(occupied.every((other) => !overlaps(placed, other)));
        occupied.push(placed);
    }
    assert.equal(placeBubble({ x: -10, y: 100 }, { width: 220, height: 86 }, bounds, []), undefined);
    assert.equal(placeBubble({ x: 190, y: 200 }, { width: 400, height: 86 }, bounds, []), undefined);
    assert.equal(placeBubble({ x: 190, y: 200 }, { width: 220, height: 86 }, bounds, [bounds]), undefined);
});

test("wrapping clips long unspaced Unicode to four lines and keeps markup as literal plain text", () => {
    const measure = (text: string) => Array.from(text).length * 7;
    const lines = wrapBubble("長".repeat(512), 140, measure);
    assert.equal(lines.length, 4);
    assert.ok(lines.every((line) => measure(line) <= 140));
    assert.ok(lines[3].endsWith("…"));
    assert.deepEqual(wrapBubble("Hello neighbor", 70, measure), ["Hello", "neighbor"]);
    assert.deepEqual(wrapBubble("<script>alert('public')</script>", 400, measure), ["<script>alert('public')</script>"]);
});

test("narrow and high-zoom tails never cross a speech body, labels, HUD or viewport", () => {
    const bounds = { x: 8, y: 8, width: 374, height: 420 };
    const hud = { x: 12, y: 350, width: 350, height: 68 };
    const speech = { x: 80, y: 156, width: 220, height: 86 };
    const thought = { x: 80, y: 48, width: 220, height: 58 };
    // Both bubble bodies fit. At 1x and 3x zoom, the thought connector would
    // cross the already placed speech; its text/body must remain, with no tail.
    for (const zoom of [1, 3]) {
        const head = { x: 190, y: 270 - 18 * zoom };
        assert.deepEqual(bubbleTail("thought", thought, head, bounds, [speech, hud]), []);
        const safe = bubbleTail("speech", speech, { x: 190, y: 270 }, bounds, [hud]);
        assert.equal(safe.length, 4);
        assert.ok(safe.every((segment) => !overlaps(segment, hud, 1)));
        assert.deepEqual(bubbleTail("speech", speech, head, bounds, safe), [], "an existing connector also protects its pixels");
    }
    const nearHud = { x: 80, y: 304, width: 220, height: 40 };
    assert.deepEqual(bubbleTail("speech", nearHud, { x: 190, y: 390 }, bounds, [hud]), []);
    const nearEdge = { x: 80, y: 10, width: 220, height: 40 };
    assert.deepEqual(bubbleTail("speech", nearEdge, { x: 190, y: -10 }, bounds, []), []);
    const label = { x: 185, y: 184, width: 30, height: 18 };
    assert.deepEqual(bubbleTail("thought", thought, { x: 190, y: 252 }, bounds, [label]), []);
});

test("390-wide Herp dialogue keeps both speakers and a thought clear of controls and labels", () => {
    const dialogue = "Bonjour, île! Grüß dich, village. I am Herp; I am gathering wood, "
        + "learning our daily trades, and looking for useful work. Shall we "
        + "make bread, clean water, tools, and a good home together?";
    assert.equal(Array.from(dialogue).length, 188);
    const candidates = [
        { id: 126, is_player: true, selected: true, anchor: { x: 190, y: 270 }, bubble: { kind: "speech" as const, text: dialogue, tick: 2595, expires_tick: 2955 } },
        { id: 126, is_player: true, selected: true, anchor: { x: 190, y: 270 }, bubble: { kind: "thought" as const, text: "Action: talk", tick: 2595, expires_tick: 2755 } },
        { id: 127, is_player: false, selected: false, anchor: { x: 194, y: 268 }, bubble: { kind: "speech" as const, text: "Hello. I am Ada, the farmer. I sell grain. Ask about a trade when you are ready.", tick: 2595, expires_tick: 2955 } },
    ];
    const before = JSON.stringify(candidates);
    const bounds = { x: 8, y: 8, width: 374, height: 420 };
    // A 390px map with the calendar, bottom toolbar and the hero's nameplate reserved.
    const surfaces: Rect[] = [
        { x: 12, y: 12, width: 350, height: 26 },
        { x: 12, y: 350, width: 350, height: 68 },
        { x: 165, y: 281, width: 50, height: 10 },
    ];
    const layout = (ordered: typeof candidates) => {
        const occupied = [...surfaces];
        return ordered.map((candidate) => {
            const lines = wrapBubble(displayBubbleText(candidate.bubble), 200, (text) => Array.from(text).length * 7);
            const rect = placeBubble(candidate.anchor, { width: 220, height: 30 + lines.length * 14 }, bounds, occupied);
            if (rect) occupied.push(rect);
            return { candidate, rect };
        });
    };
    assert.equal(layout(candidates)[2].rect, undefined, "actor-first placement starves the resident reply");
    const ordered = orderBubbles(candidates);
    assert.deepEqual(ordered.map((candidate) => `${candidate.id}:${candidate.bubble.kind}`), ["126:speech", "127:speech", "126:thought"]);
    const placed = layout(ordered);
    assert.ok(placed.every(({ rect }) => rect), "both public speech records and the lower-priority thought fit");
    const occupied = [...surfaces];
    for (const { rect } of placed) {
        assert.ok(rect);
        assert.ok(rect.x >= bounds.x && rect.y >= bounds.y && rect.x + rect.width <= bounds.x + bounds.width && rect.y + rect.height <= bounds.y + bounds.height);
        assert.ok(occupied.every((other) => !overlaps(rect, other)));
        occupied.push(rect);
    }
    assert.equal(JSON.stringify(candidates), before, "layout never changes public payloads or source order");
});
