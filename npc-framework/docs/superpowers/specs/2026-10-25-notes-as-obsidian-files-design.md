# Notes as one Obsidian file per entry: design

Date: 2026-10-25. Status: draft for approval. Changes the notes storage of
`2026-10-25-agent-notes-and-workspace-design.md` (G3, already built in Phase A) and the landmark files of
`2026-10-25-actor-map-and-memory-design.md` (M4, not built yet). Implemented as part of the same plan.

## 1. Intent

Today the diary and memory are one file each (`diary.md`, `memory.md`, entries under `## tick T - title`
headings with an id comment), and proposals are one file each with a plain header block. The dev wants
every entry to be **its own markdown file in a folder for its kind**, and every markdown file the game
creates to be **Obsidian compatible with proper frontmatter**, so the dev can open the agent folder (or
the folder holding all agents) as an Obsidian vault and browse, search and edit an agent's notes and map
there.

## 2. Goals

- O1. **One file per entry, in a folder per kind:** `diary/`, `memory/`, `proposals/`, `map/` in the
  agent folder. `diary.md` and `memory.md` are no longer written.
- O2. **Readable file names (decided by the dev):** the file name is the entry's title, so Obsidian's
  file list, graph and links show real titles.
  - Diary: `diary/YYYY-MM-DD <title>.md` (the created date first, so entries sort by day; my call).
  - Memory, proposals: `<title>.md`; landmarks: `<name>.md`.
  - Names are made safe for Obsidian and Windows (my call): the characters `* " \ / < > : | ? # ^ [ ]`
    are dropped, runs of spaces collapsed, leading/trailing spaces and dots trimmed, Windows reserved
    names (`CON`, `NUL`, `COM1`, …) get a trailing `_`, and the name is cut to 120 characters. A
    title that becomes empty falls back to the kind and id (`Memory 12`).
  - Two entries with the same name in one folder get ` (2)`, ` (3)`, … in creation order.
  - **A title change renames the file.** Write order (my call): write the new content to a temp file,
    rename it onto the new name, then remove the old file. A diary entry keeps its date prefix (the
    UTC date of `created`); only the title part changes. The old title is added to the
    entry's `aliases`, so Obsidian still finds the note by its old name in search and link
    suggestions (my call). Links made to the old file name break; that is the cost of readable names
    the dev accepted.
- O3. **Frontmatter on every entry**, valid YAML that Obsidian shows as properties:
  ```yaml
  ---
  id: 7
  type: diary            # diary | memory | proposal | landmark
  title: Found the grove north of the wall
  aliases: []            # earlier titles
  tags: [npc/diary]      # npc/diary, npc/memory, npc/proposal, npc/landmark
  agent: Wren Ashdown
  world: Hollowmere
  author: agent          # agent | player (the last writer)
  created: 2026-10-25T12:32:10Z
  created_tick: 12345
  updated: 2026-10-25T12:40:02Z
  updated_tick: 13020
  revision: 2
  ---
  The body, plain markdown.
  ```
  Proposals add `status` (`new`, `liked`, `pursuing`, `rejected`) and `prompted_by`; landmarks add
  `pos: [x, z]` (mm), `kind`, `thing` (`"#ID"`, quoted) and use `name` as their title. Dates are ISO 8601
  in UTC (`…Z`, my call: the hosts' injected notes clock returns RFC 3339 UTC, so no time crate or
  local-offset code is needed). Values the YAML could misread (a title with `: ` or starting with a
  symbol) are quoted; an old free-form date met in migration is kept verbatim, quoted.
- O4. **The game finds entries by `id`, not by file name.** It scans the folder, reads each file's
  frontmatter, and indexes by `id`, so a file the dev renames or moves within the folder in Obsidian is
  still the same entry. Hand edits to the body or the properties are picked up (mtime) like today; a
  property the game does not know is kept verbatim on update. A file with no or broken frontmatter is
  listed as unreadable (with the reason) and never overwritten. Folders are flat: subfolders and
  non-`.md` files are ignored. `list` returns the readable entries as today plus an additive
  `unreadable: [{file, reason}]` field, also shown in F7. Two files with the same `id` (a crash between
  rename and removal, a copy made in Obsidian, or an old file Windows kept locked): the
  newest one is the entry (compared by `updated` when both are UTC timestamps, then `updated_tick`,
  then `revision`, then mtime — `revision` restarts at each new tick); the others are listed as unreadable
  ("duplicate id N") and never overwritten.
- O5. **The API does not change.** `notes_list/add/update` (and the map tools) keep their fields; the
  API's `updated` stamp keeps today's shape (`{tick, date}` with the `#rN` suffix in `date`), rebuilt
  from `updated_tick`, `updated` and `revision`, and the stale check works as today.
- O6. **Generated markdown gets frontmatter too** (my call): `AGENTS.md` and `prompts/*.md` start with a
  small frontmatter (`type: agent-guide` / `type: prompt`, `agent`, `world`, `generated: true`; no
  date, so an unchanged folder regenerates byte-identical). `CLAUDE.md` stays exactly `@AGENTS.md` (the Claude Code import must not change). The
  prompt text the modal and menu copy never includes the frontmatter.
- O7. **Old folders migrate once.** On first open of an agent folder in the old layout: each `diary.md`
  and `memory.md` entry becomes a file (ids, titles, stamps and text kept; unplaceable text becomes its
  own entry titled "Kept from diary.md" so nothing is lost), each old-style proposal gets frontmatter and
  a readable name, and the old `diary.md`/`memory.md` are renamed to `diary.md.bak`/`memory.md.bak`. The
  migration is atomic per file and runs once: a book migrates when its old file exists and its `.bak`
  does not; entries whose id already has a file (a partial earlier run) are skipped, so a rerun is
  safe. An old-style proposal (no frontmatter, old header) is rewritten under its new name and the old
  file removed; missing frontmatter is what marks it as not yet migrated.

## 3. Non-goals

- No Obsidian plugin, no `.obsidian/` settings folder, no wikilinks written by the game (my call; the
  agent may write `[[links]]` in bodies itself).
- No change to notes being host-side and outside the sim, the save and the hash.

## 4. Anti-goals

- A migration or a rename loses or duplicates an entry.
- A hand edit in Obsidian (body, a property, a rename) is silently overwritten or makes the entry vanish.
- The copied prompt contains YAML.

## 5. How it fits (for the builder)

- `crates/npc_agent_io/src/notes.rs` + `notes/format.rs`: replace the one-file books with a
  folder-per-book store (one `Note` per file), keep `NotesStore`'s operations and errors; a YAML
  frontmatter reader/writer (my call: `serde_yaml` is unmaintained — use a crate already in
  `Cargo.lock` or a small hand-written reader for this fixed, flat schema; `cargo deny` must pass).
- The F7 editor and `testbed_content` readers (`read_lossy` in `explorer/home.rs`) read the new layout.
- `content/testbed_content/src/explorer/workspace.rs`: frontmatter on `AGENTS.md` and prompts;
  `GENERATED` unchanged.
- The map store (Task 21) uses the same file machinery from the start.

## 6. Proving it

- Round trip per kind (frontmatter + body), YAML-hostile titles, safe names (each forbidden character,
  reserved names, length, empty), same-name collisions, rename on title change with `aliases`, a file
  renamed by hand still found by id, unknown properties kept, broken frontmatter reported and not
  overwritten, migration of an old folder (entries, ids, stray text, `.bak`, runs once), API unchanged
  (existing notes tests pass with only path expectations changed), copied prompt has no YAML.
- `cargo xtask verify` prints `"ok":true`.

## 7. Decided by the dev

- Diary and memory become one markdown file per entry in their own folders, like proposals.
- Every markdown file the game creates is Obsidian compatible with proper frontmatter.
- File names are readable titles (renamed when the title changes).

## 8. Calls I made (check while reading)

- Diary names start with the created date (UTC); others are the bare title (landmarks: their name).
- Dates in UTC (`Z`); the API stamp keeps today's `date#rN` shape.
- Flat folders; unreadable and duplicate-id files listed in an additive `unreadable` field, never
  overwritten; the newest (updated, tick, revision, mtime) wins a duplicate id; new ids never reuse
  an id seen in any file, readable or not.
- Safe-name rules, ` (2)` for duplicates, 120-character cap, `Memory 12` fallback.
- The old title goes into `aliases` on a rename.
- Tags `npc/diary`, `npc/memory`, `npc/proposal`, `npc/landmark`; `author` is the last writer.
- `AGENTS.md` and prompts get frontmatter without a date; `CLAUDE.md` does not; copied prompts have none.
- Migration keeps `.bak` copies of `diary.md`/`memory.md`; stray text becomes its own entry.
- No `.obsidian/` folder and no game-written wikilinks.
