import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { FileFeed } from "./feed.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
const args = process.argv.slice(2);
function option(name: string, fallback: string): string {
    const index = args.indexOf(name);
    if (index < 0) return fallback;
    const value = args[index + 1];
    if (!value || value.startsWith("--"))
        throw new Error(`${name} needs a value`);
    return value;
}
const port = Number(option("--port", process.env.PORT ?? "5173"));
if (!Number.isInteger(port) || port < 1 || port > 65535)
    throw new Error("--port must be 1..65535");
const file = path.resolve(
    option(
        "--state",
        process.env.WORLD_STATE_FILE ??
            path.join(root, "../npc-framework/target/world-state/island.json"),
    ),
);
const feed = new FileFeed(file);
const production = args.includes("--production");
const vite = production
    ? undefined
    : await (
          await import("vite")
      ).createServer({
          root,
          server: { middlewareMode: true, hmr: false },
          appType: "spa",
      });
const mime: Record<string, string> = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript",
    ".css": "text/css",
    ".png": "image/png",
    ".woff2": "font/woff2",
  ".woff": "font/woff",
    ".svg": "image/svg+xml",
    ".ico": "image/x-icon",
};
const server = createServer(async (req, res) => {
    // This is an observer on loopback, with no arbitrary file routes or gameplay RPC proxy.
    const host = req.headers.host?.split(":")[0];
    if (
        !host ||
        !["127.0.0.1", "localhost"].includes(host) ||
        (req.headers.origin &&
            ![`http://127.0.0.1:${port}`, `http://localhost:${port}`].includes(
                req.headers.origin,
            ))
    ) {
        res.writeHead(403).end();
        return;
    }
    if (req.method !== "GET" && req.method !== "HEAD") {
        res.writeHead(405).end();
        return;
    }
    const url = new URL(req.url ?? "/", `http://127.0.0.1:${port}`);
    if (url.pathname === "/api/world-state") {
        await feed.refresh();
        res.writeHead(200, {
            "Content-Type": "application/json",
            "Cache-Control": "no-store",
        });
        res.end(
            JSON.stringify({ ...feed.current, source: path.basename(file) }),
        );
        return;
    }
    if (url.pathname.startsWith("/api/")) {
        res.writeHead(404).end();
        return;
    }
    if (vite) {
        vite.middlewares(req, res);
        return;
    }
    try {
        const dist = path.join(root, "dist");
        const target = path.resolve(
            dist,
            `.${decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname)}`,
        );
        if (!target.startsWith(dist + path.sep)) {
            res.writeHead(403).end();
            return;
        }
        const bytes = await readFile(target);
        res.writeHead(200, {
            "Content-Type":
                mime[path.extname(target)] ?? "application/octet-stream",
        });
        res.end(bytes);
    } catch {
        res.writeHead(404).end("Not found");
    }
});
await feed.refresh();
// Poll the configured file rather than a persistent inode watch: works across Windows atomic replacement.
const watcher = setInterval(() => {
    void feed.refresh();
}, 250);
server.listen(port, "127.0.0.1", () => {
    console.log(
        `NPC World Viewer: http://127.0.0.1:${port}\nWatching: ${file}`,
    );
});
function close(): void {
    clearInterval(watcher);
    server.close();
    void vite?.close();
}
process.once("SIGINT", close);
process.once("SIGTERM", close);
