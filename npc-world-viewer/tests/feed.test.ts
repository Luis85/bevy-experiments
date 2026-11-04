import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, rename, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { FileFeed } from "../server/feed.ts";
import { example } from "./fixture.ts";

test("file feed tolerates missing/partial files, atomic replacement, invalid schema, recovery and reset", async () => {
    const dir = await mkdtemp(join(tmpdir(), "npc-viewer-"));
    try {
        const file = join(dir, "world.json");
        const feed = new FileFeed(file);
        await feed.refresh();
        assert.match(feed.current.error!, /Waiting/);
        await writeFile(file, JSON.stringify(example(20)));
        await feed.refresh();
        assert.equal(feed.current.state!.tick, 20);
        assert.equal(feed.current.error, undefined);
        const revision = feed.current.revision;
        await writeFile(file, "{");
        await feed.refresh();
        assert.equal(feed.current.state!.tick, 20);
        assert.ok(feed.current.error);
        assert.equal(feed.current.revision, revision);
        await writeFile(
            file,
            JSON.stringify({ ...example(), schema_version: 99 }),
        );
        await feed.refresh();
        assert.equal(feed.current.state!.tick, 20);
        await writeFile(file + ".tmp", JSON.stringify(example(0)));
        await rename(file + ".tmp", file);
        await Promise.all([feed.refresh(), feed.refresh()]);
        assert.equal(feed.current.state!.tick, 0);
        assert.equal(feed.current.error, undefined);
        assert.notEqual(feed.current.revision, revision);
        await rm(file);
        await feed.refresh();
        assert.equal(feed.current.state!.tick, 0);
        assert.ok(feed.current.error);
        await writeFile(file, JSON.stringify(example(0)));
        await feed.refresh();
        assert.equal(feed.current.error, undefined);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
