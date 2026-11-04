import { readFile, stat } from "node:fs/promises";
import { createHash } from "node:crypto";
import { parseWorldState, type WorldState } from "../src/state.ts";

export type FeedResult = {
    state?: WorldState;
    revision?: string;
    updated_at?: string;
    error?: string;
};
export class FileFeed {
    current: FeedResult = {};
    private refreshing?: Promise<void>;
    constructor(readonly path: string) {}
    refresh(): Promise<void> {
        // A single read in flight: rapid atomic renames cannot reorder accepted snapshots.
        return (this.refreshing ??= this.read().finally(() => {
            this.refreshing = undefined;
        }));
    }
    private async read(): Promise<void> {
        try {
            const info = await stat(this.path);
            if (!info.isFile() || info.size > 16 * 1024 * 1024)
                throw new Error("snapshot must be a file smaller than 16 MiB");
            const text = await readFile(this.path, "utf8");
            const revision = createHash("sha256").update(text).digest("hex");
            if (revision !== this.current.revision) {
                const state = parseWorldState(
                    JSON.parse(text.replace(/^\uFEFF/, "")),
                );
                this.current = {
                    state,
                    revision,
                    updated_at: info.mtime.toISOString(),
                };
            } else {
                this.current = {
                    ...this.current,
                    updated_at: info.mtime.toISOString(),
                    error: undefined,
                };
            }
        } catch (error) {
            const missing = (error as NodeJS.ErrnoException).code === "ENOENT";
            this.current = {
                ...this.current,
                error: missing
                    ? "Waiting for the world-state file. Start the simulation exporter."
                    : "World-state file is unreadable or invalid. Keeping the last valid snapshot; retrying automatically.",
            };
        }
    }
}
