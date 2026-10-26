//! The notes store (diary, memory, proposals) on real files in a temporary
//! agent folder: one Obsidian-compatible markdown file per entry in a folder
//! per book (`diary/`, `memory/`, `proposals/`), round trips, ids, limits,
//! optimistic concurrency, writers, readable file names, renames, unreadable
//! files, migration of the old one-file books and proposals, external edits,
//! BOM and CRLF.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use npc_agent_io::notes::{
    BODY_MAX, Book, NewNote, NoteChange, NotesError, NotesOwner, NotesStore, ProposalStatus, Stamp,
    TITLE_MAX, Writer, safe_name,
};

/// A fresh, empty folder for one test.
fn folder(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npc-notes-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn at(tick: u64) -> Stamp {
    Stamp {
        tick,
        date: format!("2026-10-25T10:00:{:02}Z", tick % 60),
    }
}

fn new(title: &str, body: &str) -> NewNote {
    NewNote {
        title: title.to_string(),
        body: body.to_string(),
        prompted_by: None,
    }
}

fn body_change(body: &str) -> NoteChange {
    NoteChange {
        title: None,
        body: Some(body.to_string()),
        status: None,
    }
}

fn title_change(title: &str) -> NoteChange {
    NoteChange {
        title: Some(title.to_string()),
        body: None,
        status: None,
    }
}

fn read(p: &Path) -> String {
    String::from_utf8(std::fs::read(p).unwrap()).unwrap()
}

/// The file names in `dir/sub` (files only), sorted.
fn files_in(dir: &Path, sub: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir.join(sub)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn proposal_files(dir: &Path) -> Vec<String> {
    files_in(dir, "proposals")
}

fn owner() -> NotesOwner {
    NotesOwner {
        agent: "Wren Ashdown".into(),
        world: "Hollowmere".into(),
    }
}

fn mtime(p: &Path) -> SystemTime {
    std::fs::metadata(p).unwrap().modified().unwrap()
}

/// Moves `p`'s modification time `secs` seconds back (no clock read: from
/// its own time).
fn age(p: &Path, secs: u64) {
    let back = mtime(p) - Duration::from_secs(secs);
    let f = std::fs::File::options().write(true).open(p).unwrap();
    f.set_modified(back).unwrap();
}

const BOOKS: [Book; 3] = [Book::Diary, Book::Memory, Book::Proposals];

#[test]
fn add_then_list_round_trips() {
    let d = folder("round-trip");
    let mut store = NotesStore::open(&d).unwrap();
    for book in BOOKS {
        let a = store
            .add(
                book,
                Writer::Agent,
                new("Left the compound", "Out through the gate.\n\nThen north."),
                at(120),
            )
            .unwrap();
        let mut second = new("Doors: who has the key", "Line one\n  indented line two");
        if book == Book::Proposals {
            second.prompted_by = Some("the locked store door".to_string());
        }
        let b = store.add(book, Writer::Player, second, at(130)).unwrap();
        assert_eq!(a.title, "Left the compound");
        assert_eq!(a.body, "Out through the gate.\n\nThen north.");
        assert_eq!((a.created.clone(), a.updated.clone()), (at(120), at(120)));
        assert_eq!(b.body, "Line one\n  indented line two");
        if book == Book::Proposals {
            assert_eq!(a.status, Some(ProposalStatus::New));
            assert_eq!(b.prompted_by.as_deref(), Some("the locked store door"));
        } else {
            assert_eq!((a.status, b.prompted_by.clone()), (None, None));
        }
        assert_eq!(store.list(book).unwrap(), vec![a.clone(), b.clone()]);
        // A new store (a resumed game) reads the same notes from the files.
        let mut again = NotesStore::open(&d).unwrap();
        assert_eq!(again.list(book).unwrap(), vec![a, b]);
    }
    assert_eq!(
        files_in(&d, "diary"),
        [
            "2026-10-25 Doors who has the key.md",
            "2026-10-25 Left the compound.md"
        ]
    );
    let diary = read(&d.join("diary/2026-10-25 Left the compound.md"));
    assert!(
        diary.starts_with("---\nid: 1\ntype: diary\ntitle: Left the compound\n"),
        "{diary}"
    );
    let memory = read(&d.join("memory/Doors who has the key.md"));
    assert!(
        memory.starts_with("---\nid: 2\ntype: memory\ntitle: \"Doors: who has the key\"\n"),
        "{memory}"
    );
    assert_eq!(
        proposal_files(&d),
        ["Doors who has the key.md", "Left the compound.md"]
    );
    let p = read(&d.join("proposals/Doors who has the key.md"));
    assert!(
        p.starts_with("---\nid: 2\ntype: proposal\ntitle: \"Doors: who has the key\"\n"),
        "{p}"
    );
    assert!(p.contains("\nstatus: new\n"), "{p}");
    assert!(p.contains("\nprompted_by: the locked store door\n"), "{p}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn each_kind_round_trips() {
    let d = folder("each-kind");
    let mut store = NotesStore::open_as(&d, owner()).unwrap();
    for book in BOOKS {
        let mut n = new(
            "Found the grove north of the wall",
            "The body,\n\nplain markdown.",
        );
        if book == Book::Proposals {
            n.prompted_by = Some("the wall".into());
        }
        let note = store.add(book, Writer::Agent, n, at(125)).unwrap();
        let mut again = NotesStore::open(&d).unwrap();
        assert_eq!(again.list(book).unwrap(), vec![note]);
        assert!(again.unreadable(book).is_empty());
    }
    let common = |ty: &str| {
        format!(
            "---\nid: 1\ntype: {ty}\ntitle: Found the grove north of the wall\naliases: []\n\
             tags: [npc/{ty}]\nagent: Wren Ashdown\nworld: Hollowmere\nauthor: agent\n\
             created: 2026-10-25T10:00:05Z\ncreated_tick: 125\nupdated: 2026-10-25T10:00:05Z\n\
             updated_tick: 125\nrevision: 1\n"
        )
    };
    let body = "---\nThe body,\n\nplain markdown.\n";
    assert_eq!(
        read(&d.join("diary/2026-10-25 Found the grove north of the wall.md")),
        format!("{}{body}", common("diary"))
    );
    assert_eq!(
        read(&d.join("memory/Found the grove north of the wall.md")),
        format!("{}{body}", common("memory"))
    );
    assert_eq!(
        read(&d.join("proposals/Found the grove north of the wall.md")),
        format!(
            "{}status: new\nprompted_by: the wall\n{body}",
            common("proposal")
        )
    );
    // The last writer is the author; an empty body is just the frontmatter.
    let n = store
        .add(Book::Memory, Writer::Player, new("Empty", ""), at(126))
        .unwrap();
    let text = read(&d.join("memory/Empty.md"));
    assert!(text.contains("\nauthor: player\n"), "{text}");
    assert!(text.ends_with("revision: 1\n---\n"), "{text}");
    store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &n.updated,
            body_change("Now with text."),
            at(127),
        )
        .unwrap();
    let text = read(&d.join("memory/Empty.md"));
    assert!(text.contains("\nauthor: agent\n"), "{text}");
    assert!(text.ends_with("---\nNow with text.\n"), "{text}");
    // A store opened without an owner writes empty agent and world.
    let e = folder("each-kind-no-owner");
    let mut bare = NotesStore::open(&e).unwrap();
    bare.add(Book::Memory, Writer::Agent, new("x", "y"), at(1))
        .unwrap();
    let text = read(&e.join("memory/x.md"));
    assert!(text.contains("\nagent: \"\"\nworld: \"\"\n"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&e);
}

#[test]
fn yaml_hostile_titles_round_trip() {
    let d = folder("hostile");
    let mut store = NotesStore::open_as(&d, owner()).unwrap();
    let titles = [
        "Doors: who has the key",
        "- starts with a dash",
        "#tag first",
        "Room #3",
        "\"quoted\" start",
        "'single' start",
        "yes",
        "null",
        "1984",
        "-12.5",
        "trailing colon:",
        "[bracketed]",
        "{braced}",
        "a \\ backslash",
        "50% done",
        "@mention",
        "* star",
        "& anchor",
        "! tag",
        "| pipe",
        "> folded",
        "`code`",
        "? question",
        "~",
        "tab\there",
        "Café ☕ ünïcödé",
    ];
    let mut added = Vec::new();
    for t in titles {
        let mut n = new(t, "body");
        n.prompted_by = Some(t.to_string());
        added.push(store.add(Book::Proposals, Writer::Agent, n, at(5)).unwrap());
    }
    let mut again = NotesStore::open(&d).unwrap();
    let listed = again.list(Book::Proposals).unwrap();
    assert_eq!(listed, added);
    assert!(again.unreadable(Book::Proposals).is_empty());
    let got: Vec<&str> = listed.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(got, titles);
    for n in &listed {
        assert_eq!(n.prompted_by.as_deref(), Some(n.title.as_str()));
    }
    // Quoted where YAML would misread it, plain where it would not.
    let file = |name: &str| read(&d.join("proposals").join(name));
    assert!(file("Doors who has the key.md").contains("\ntitle: \"Doors: who has the key\"\n"));
    assert!(file("Room 3.md").contains("\ntitle: \"Room #3\"\n"));
    assert!(file("1984.md").contains("\ntitle: \"1984\"\n"));
    assert!(file("yes.md").contains("\ntitle: \"yes\"\n"));
    assert!(file("Café ☕ ünïcödé.md").contains("\ntitle: Café ☕ ünïcödé\n"));
    // Hostile titles survive as aliases after a rename too.
    let n = &listed[0];
    let r = again
        .update(
            Book::Proposals,
            Writer::Player,
            n.id,
            &n.updated,
            title_change("Keys"),
            at(6),
        )
        .unwrap();
    assert_eq!(r.title, "Keys");
    assert!(
        file("Keys.md").contains("\naliases: [\"Doors: who has the key\"]\n"),
        "{}",
        file("Keys.md")
    );
    let mut third = NotesStore::open(&d).unwrap();
    assert_eq!(third.list(Book::Proposals).unwrap()[0], r);
    // A body line `---` is body text, not frontmatter.
    let b = third
        .add(
            Book::Memory,
            Writer::Agent,
            new("Rules", "above\n---\nbelow\n---"),
            at(7),
        )
        .unwrap();
    let mut fourth = NotesStore::open(&d).unwrap();
    assert_eq!(fourth.list(Book::Memory).unwrap(), vec![b]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn safe_names() {
    for c in [
        '*', '"', '\\', '/', '<', '>', ':', '|', '?', '#', '^', '[', ']',
    ] {
        assert_eq!(safe_name(&format!("a{c}b")), "ab", "{c:?}");
        assert_eq!(safe_name(&format!("a {c} b")), "a b", "{c:?}");
    }
    assert_eq!(
        safe_name("Doors: who has the key?"),
        "Doors who has the key"
    );
    // Runs of spaces collapse; spaces and dots at the ends go.
    assert_eq!(safe_name("a    b \t c"), "a b c");
    assert_eq!(safe_name("  ..The end..  "), "The end");
    assert_eq!(safe_name("v1.2 notes"), "v1.2 notes");
    // Windows reserved names get a trailing `_` (any case, any extension).
    for (name, safe) in [
        ("CON", "CON_"),
        ("con", "con_"),
        ("Prn", "Prn_"),
        ("AUX", "AUX_"),
        ("nul", "nul_"),
        ("COM1", "COM1_"),
        ("com9", "com9_"),
        ("LPT1", "LPT1_"),
        ("lpt9", "lpt9_"),
        ("nul.txt", "nul_.txt"),
        ("CON .", "CON_"),
        ("COM0", "COM0_"),
        ("lpt0", "lpt0_"),
        ("COM¹", "COM¹_"),
        ("com²", "com²_"),
        ("LPT³", "LPT³_"),
        ("lpt¹.log", "lpt¹_.log"),
        ("CONIN$", "CONIN$_"),
        ("conout$", "conout$_"),
        ("CONOUT$.txt", "CONOUT$_.txt"),
    ] {
        assert_eq!(safe_name(name), safe, "{name}");
    }
    for name in [
        "console", "nullify", "LPT", "com10", "COM⁴", "conin", "CONIN$x",
    ] {
        assert_eq!(safe_name(name), name);
    }
    // Cut to 120 characters (not bytes), then trimmed again.
    assert_eq!(safe_name(&"a".repeat(130)), "a".repeat(120));
    assert_eq!(safe_name(&"é".repeat(130)).chars().count(), 120);
    assert_eq!(
        safe_name(&format!("{} bcd", "a".repeat(119))),
        "a".repeat(119)
    );
    // Nothing left: empty, and the store falls back to the kind and id.
    for t in ["", "???", " . ", "[[]]"] {
        assert_eq!(safe_name(t), "", "{t:?}");
    }
    let d = folder("safe-names");
    let mut store = NotesStore::open(&d).unwrap();
    for book in BOOKS {
        store
            .add(book, Writer::Agent, new("first", "x"), at(1))
            .unwrap();
        store
            .add(book, Writer::Agent, new("???", "x"), at(2))
            .unwrap();
    }
    assert_eq!(
        files_in(&d, "diary"),
        ["2026-10-25 Diary 2.md", "2026-10-25 first.md"]
    );
    assert_eq!(files_in(&d, "memory"), ["Memory 2.md", "first.md"]);
    assert_eq!(proposal_files(&d), ["Proposal 2.md", "first.md"]);
    // The title itself is kept as typed.
    assert_eq!(store.list(Book::Memory).unwrap()[1].title, "???");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn same_title_gets_a_number() {
    let d = folder("same-title");
    let mut store = NotesStore::open(&d).unwrap();
    for i in 0..3 {
        store
            .add(
                Book::Memory,
                Writer::Agent,
                new("Grove", &format!("{i}")),
                at(i),
            )
            .unwrap();
    }
    // Names differing only in case collide on Windows: numbered too.
    store
        .add(Book::Memory, Writer::Agent, new("grove", "3"), at(3))
        .unwrap();
    // `Grove?` makes the same name.
    store
        .add(Book::Memory, Writer::Agent, new("Grove?", "4"), at(4))
        .unwrap();
    assert_eq!(
        files_in(&d, "memory"),
        [
            "Grove (2).md",
            "Grove (3).md",
            "Grove (5).md",
            "Grove.md",
            "grove (4).md"
        ]
    );
    for (file, id) in [
        ("Grove.md", 1),
        ("Grove (2).md", 2),
        ("Grove (3).md", 3),
        ("grove (4).md", 4),
        ("Grove (5).md", 5),
    ] {
        let text = read(&d.join("memory").join(file));
        assert!(
            text.starts_with(&format!("---\nid: {id}\n")),
            "{file}: {text}"
        );
    }
    // Diary names include the day: the same title on another day is free.
    store
        .add(Book::Diary, Writer::Agent, new("Grove", "a"), at(1))
        .unwrap();
    store
        .add(Book::Diary, Writer::Agent, new("Grove", "b"), at(2))
        .unwrap();
    let next_day = Stamp {
        tick: 90_000,
        date: "2026-10-26T08:00:00Z".into(),
    };
    store
        .add(Book::Diary, Writer::Agent, new("Grove", "c"), next_day)
        .unwrap();
    assert_eq!(
        files_in(&d, "diary"),
        [
            "2026-10-25 Grove (2).md",
            "2026-10-25 Grove.md",
            "2026-10-26 Grove.md"
        ]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn title_change_renames_and_keeps_an_alias() {
    let d = folder("rename");
    let mut store = NotesStore::open(&d).unwrap();
    let n = store
        .add(Book::Memory, Writer::Agent, new("Grove", "Trees."), at(10))
        .unwrap();
    let r1 = store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &n.updated,
            title_change("The north grove"),
            at(11),
        )
        .unwrap();
    assert_eq!(files_in(&d, "memory"), ["The north grove.md"]);
    let text = read(&d.join("memory/The north grove.md"));
    assert!(
        text.contains("\ntitle: The north grove\naliases: [Grove]\n"),
        "{text}"
    );
    assert!(text.ends_with("---\nTrees.\n"), "{text}");
    let r2 = store
        .update(
            Book::Memory,
            Writer::Player,
            n.id,
            &r1.updated,
            title_change("Grove by the wall"),
            at(12),
        )
        .unwrap();
    assert_eq!(files_in(&d, "memory"), ["Grove by the wall.md"]);
    let text = read(&d.join("memory/Grove by the wall.md"));
    assert!(
        text.contains("\naliases: [Grove, The north grove]\n"),
        "{text}"
    );
    // Back to an earlier title: no alias twice.
    let r3 = store
        .update(
            Book::Memory,
            Writer::Player,
            n.id,
            &r2.updated,
            title_change("Grove"),
            at(13),
        )
        .unwrap();
    let text = read(&d.join("memory/Grove.md"));
    assert!(
        text.contains("\naliases: [Grove, The north grove, Grove by the wall]\n"),
        "{text}"
    );
    // A body change leaves the name alone; a title taken by another file
    // gets a number.
    let other = store
        .add(Book::Memory, Writer::Agent, new("Wall", "Stone."), at(14))
        .unwrap();
    store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &r3.updated,
            body_change("More trees."),
            at(15),
        )
        .unwrap();
    assert_eq!(files_in(&d, "memory"), ["Grove.md", "Wall.md"]);
    store
        .update(
            Book::Memory,
            Writer::Agent,
            other.id,
            &other.updated,
            title_change("grove"),
            at(16),
        )
        .unwrap();
    assert_eq!(files_in(&d, "memory"), ["Grove.md", "grove (2).md"]);
    // A diary entry keeps its created day when renamed on another day.
    let e = store
        .add(Book::Diary, Writer::Agent, new("Day one", "x"), at(20))
        .unwrap();
    let later = Stamp {
        tick: 90_000,
        date: "2026-10-27T09:00:00Z".into(),
    };
    store
        .update(
            Book::Diary,
            Writer::Agent,
            e.id,
            &e.updated,
            title_change("The first day"),
            later,
        )
        .unwrap();
    assert_eq!(files_in(&d, "diary"), ["2026-10-25 The first day.md"]);
    // Every rename went through a temp file: none left behind.
    for sub in ["diary", "memory"] {
        for f in files_in(&d, sub) {
            assert!(f.ends_with(".md") && !f.starts_with('.'), "{f}");
        }
    }
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Memory).unwrap().len(), 2);
    assert!(again.unreadable(Book::Memory).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn renamed_by_hand_is_found_by_id() {
    let d = folder("hand-rename");
    let mut store = NotesStore::open(&d).unwrap();
    let n = store
        .add(Book::Memory, Writer::Agent, new("Grove", "Trees."), at(10))
        .unwrap();
    std::fs::rename(d.join("memory/Grove.md"), d.join("memory/My grove.md")).unwrap();
    let listed = store.list(Book::Memory).unwrap();
    assert_eq!(listed, vec![n.clone()], "same entry, same stamp");
    // An update writes the file where it is now.
    let u = store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &n.updated,
            body_change("Old trees."),
            at(11),
        )
        .unwrap();
    assert_eq!(files_in(&d, "memory"), ["My grove.md"]);
    assert!(read(&d.join("memory/My grove.md")).ends_with("---\nOld trees.\n"));
    // A fresh store finds it too; subfolders and other files are ignored.
    std::fs::create_dir_all(d.join("memory/archive")).unwrap();
    std::fs::copy(
        d.join("memory/My grove.md"),
        d.join("memory/archive/My grove.md"),
    )
    .unwrap();
    std::fs::write(d.join("memory/picture.png"), b"\x89PNG").unwrap();
    std::fs::write(d.join("memory/notes.txt"), "---\nid: 9\n---\n").unwrap();
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Memory).unwrap(), vec![u]);
    assert!(again.unreadable(Book::Memory).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn unknown_properties_survive_update() {
    let d = folder("unknown-props");
    std::fs::create_dir_all(d.join("proposals")).unwrap();
    // As Obsidian writes it: block lists, an extra property, a comment.
    let head = "---\nid: 4\ntype: proposal\ntitle: Lamps\naliases:\n  - Lights\ntags:\n  - npc/proposal\n  - idea\n\
                mood: curious\ncssclasses:\n  - wide\n# kept by hand\nagent: Wren Ashdown\nworld: Hollowmere\n\
                author: agent\ncreated: 2026-10-25T10:00:00Z\ncreated_tick: 60\nupdated: 2026-10-25T10:00:00Z\n\
                updated_tick: 60\nrevision: 1\nstatus: liked\nprompted_by: the night walk\nrating: 5\n";
    let file = d.join("proposals/Lamps.md");
    std::fs::write(&file, format!("{head}---\nDark at night.\n")).unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    let p = store.list(Book::Proposals).unwrap().remove(0);
    assert_eq!(
        (p.id, p.title.as_str(), p.status, p.body.as_str()),
        (4, "Lamps", Some(ProposalStatus::Liked), "Dark at night.")
    );
    let u = store
        .update(
            Book::Proposals,
            Writer::Player,
            4,
            &p.updated,
            body_change("Very dark at night."),
            at(70),
        )
        .unwrap();
    let text = read(&file);
    // Only the changed properties are rewritten; every other line is kept.
    let want = head
        .replace(
            "updated: 2026-10-25T10:00:00Z",
            "updated: 2026-10-25T10:00:10Z",
        )
        .replace("updated_tick: 60", "updated_tick: 70")
        .replace("author: agent", "author: player");
    assert_eq!(text, format!("{want}---\nVery dark at night.\n"));
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Proposals).unwrap(), vec![u]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn broken_frontmatter_is_unreadable_and_untouched() {
    let d = folder("broken");
    std::fs::create_dir_all(d.join("diary")).unwrap();
    let good =
        "---\nid: 1\ntype: diary\ntitle: Good\ncreated_tick: 5\nupdated_tick: 5\n---\nFine.\n";
    let broken: [(&str, &[u8], &str); 8] = [
        ("No front.md", b"Just text.\n", "no frontmatter"),
        ("Unclosed.md", b"---\nid: 3\ntitle: x\n", "closing ---"),
        ("Bad line.md", b"---\nid: 4\njust words\n---\nx\n", "line 3"),
        ("Nested id.md", b"---\nid:\n  n: 5\n---\nx\n", "plain value"),
        ("Bad id.md", b"---\nid: seven\n---\nx\n", "id"),
        (
            "Wrong type.md",
            b"---\nid: 8\ntype: memory\n---\nx\n",
            "memory",
        ),
        (
            "Bad quote.md",
            b"---\nid: 9\ntitle: \"open\n---\nx\n",
            "quote",
        ),
        ("Latin1.md", b"---\nid: 10\ntitle: caf\xe9\n---\n", "UTF-8"),
    ];
    std::fs::write(d.join("diary/Good.md"), good).unwrap();
    for (name, bytes, _) in &broken {
        std::fs::write(d.join("diary").join(name), bytes).unwrap();
    }
    let mut store = NotesStore::open(&d).unwrap();
    let list = store.list(Book::Diary).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].id, list[0].title.as_str()), (1, "Good"));
    let unreadable = store.unreadable(Book::Diary);
    let mut want: Vec<&str> = broken.iter().map(|(n, _, _)| *n).collect();
    want.sort_unstable();
    let got: Vec<String> = unreadable.iter().map(|u| u.file.clone()).collect();
    assert_eq!(
        got,
        want.iter()
            .map(|n| format!("diary/{n}"))
            .collect::<Vec<_>>()
    );
    for (name, _, hint) in &broken {
        let u = unreadable
            .iter()
            .find(|u| u.file == format!("diary/{name}"))
            .unwrap();
        assert!(u.reason.contains(hint), "{name}: {}", u.reason);
    }
    // Writes around them never touch them.
    let added = store
        .add(Book::Diary, Writer::Agent, new("No front", "x"), at(20))
        .unwrap();
    store
        .update(
            Book::Diary,
            Writer::Agent,
            1,
            &list[0].updated,
            title_change("Unclosed"),
            at(21),
        )
        .unwrap();
    for (name, bytes, _) in &broken {
        assert_eq!(&std::fs::read(d.join("diary").join(name)).unwrap(), bytes);
    }
    assert!(d.join("diary/2026-10-25 No front.md").is_file());
    // `Good` has no created date, so no day in its name: `Unclosed.md` is
    // taken by the broken file, the renamed entry gets a number.
    assert!(d.join("diary/Unclosed (2).md").is_file());
    assert!(!d.join("diary/Good.md").exists());
    // Above every id an unreadable file names (`Latin1.md` has 10).
    assert_eq!(added.id, 11);
    // Fixed by hand, a file becomes an entry.
    std::fs::write(
        d.join("diary/Bad id.md"),
        "---\nid: 30\ntitle: Lost\n---\nx\n",
    )
    .unwrap();
    let list = store.list(Book::Diary).unwrap();
    assert!(list.iter().any(|n| n.id == 30 && n.title == "Lost"));
    assert_eq!(store.unreadable(Book::Diary).len(), broken.len() - 1);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn duplicate_id_newest_wins() {
    let d = folder("duplicate");
    std::fs::create_dir_all(d.join("memory")).unwrap();
    let file = |title: &str, id: u32, rev: u32| {
        format!(
            "---\nid: {id}\ntype: memory\ntitle: {title}\ncreated_tick: 1\nupdated: d\nupdated_tick: 2\n\
             revision: {rev}\n---\n{title} body\n"
        )
    };
    // Same id 5, same tick, dates that are no timestamps: A has the higher
    // revision, B is newer on disk.
    std::fs::write(d.join("memory/A.md"), file("A", 5, 3)).unwrap();
    std::fs::write(d.join("memory/B.md"), file("B", 5, 1)).unwrap();
    age(&d.join("memory/A.md"), 100);
    // Same id 6 and revision: the newer file wins.
    std::fs::write(d.join("memory/C.md"), file("C", 6, 2)).unwrap();
    std::fs::write(d.join("memory/D.md"), file("D", 6, 2)).unwrap();
    age(&d.join("memory/C.md"), 100);
    let mut store = NotesStore::open(&d).unwrap();
    let list = store.list(Book::Memory).unwrap();
    let got: Vec<(u32, &str)> = list.iter().map(|n| (n.id, n.title.as_str())).collect();
    assert_eq!(got, [(5, "A"), (6, "D")]);
    assert_eq!(list[0].updated.date, "d#r3");
    let unreadable: Vec<(String, String)> = store
        .unreadable(Book::Memory)
        .into_iter()
        .map(|u| (u.file, u.reason))
        .collect();
    assert_eq!(
        unreadable,
        [
            ("memory/B.md".to_string(), "duplicate id 5".to_string()),
            ("memory/C.md".to_string(), "duplicate id 6".to_string()),
        ]
    );
    // Updates go to the winner; the losers are never overwritten.
    let (b, c) = (read(&d.join("memory/B.md")), read(&d.join("memory/C.md")));
    store
        .update(
            Book::Memory,
            Writer::Player,
            5,
            &list[0].updated,
            body_change("new A"),
            at(9),
        )
        .unwrap();
    assert!(read(&d.join("memory/A.md")).ends_with("---\nnew A\n"));
    assert_eq!(read(&d.join("memory/B.md")), b);
    assert_eq!(read(&d.join("memory/C.md")), c);
    // New ids never reuse a duplicate's.
    let n = store
        .add(Book::Memory, Writer::Agent, new("E", "x"), at(10))
        .unwrap();
    assert_eq!(n.id, 7);
    // Timestamps beat ticks; ticks beat revisions.
    let stamped = |title: &str, id: u32, date: &str, tick: u64, rev: u32| {
        format!(
            "---\nid: {id}\ntitle: {title}\nupdated: {date}\nupdated_tick: {tick}\nrevision: {rev}\n---\n"
        )
    };
    std::fs::write(
        d.join("memory/F.md"),
        stamped("F", 8, "2026-10-25T10:00:00Z", 900, 4),
    )
    .unwrap();
    std::fs::write(
        d.join("memory/G.md"),
        stamped("G", 8, "2026-10-25T10:00:01Z", 3, 1),
    )
    .unwrap();
    std::fs::write(d.join("memory/H.md"), stamped("H", 9, "x", 50, 1)).unwrap();
    std::fs::write(d.join("memory/I.md"), stamped("I", 9, "x", 40, 7)).unwrap();
    age(&d.join("memory/G.md"), 100);
    age(&d.join("memory/H.md"), 100);
    let titles: Vec<(u32, String)> = store
        .list(Book::Memory)
        .unwrap()
        .into_iter()
        .map(|n| (n.id, n.title))
        .collect();
    assert!(titles.contains(&(8, "G".into())), "{titles:?}");
    assert!(titles.contains(&(9, "H".into())), "{titles:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn renamed_entry_left_behind_loses_to_the_new_file() {
    let d = folder("left-behind");
    let mut store = NotesStore::open(&d).unwrap();
    // Three writes at tick 10: revision 3.
    let n = store
        .add(Book::Memory, Writer::Agent, new("Grove", "v1"), at(10))
        .unwrap();
    let u1 = store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &n.updated,
            body_change("v2"),
            at(10),
        )
        .unwrap();
    let u2 = store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &u1.updated,
            body_change("v3"),
            at(10),
        )
        .unwrap();
    let old = read(&d.join("memory/Grove.md"));
    assert!(old.contains("\nrevision: 3\n"), "{old}");
    // Renamed at a later tick (revision 1) ...
    let r = store
        .update(
            Book::Memory,
            Writer::Agent,
            n.id,
            &u2.updated,
            title_change("North grove"),
            at(11),
        )
        .unwrap();
    // ... and the old file left behind (the removal never happened).
    std::fs::write(d.join("memory/Grove.md"), &old).unwrap();
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Memory).unwrap(), vec![r]);
    let u: Vec<(String, String)> = again
        .unreadable(Book::Memory)
        .into_iter()
        .map(|u| (u.file, u.reason))
        .collect();
    assert_eq!(
        u,
        [(
            "memory/Grove.md".to_string(),
            format!("duplicate id {}", n.id)
        )]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn pre_fresh_world_file_loses_despite_a_higher_tick() {
    let d = folder("fresh-world");
    std::fs::create_dir_all(d.join("diary")).unwrap();
    // The same entry: before a fresh world (tick 90000, yesterday) and
    // after it (tick 30, today).
    let file = |title: &str, date: &str, tick: u64| {
        format!(
            "---\nid: 4\ntype: diary\ntitle: {title}\nupdated: {date}\nupdated_tick: {tick}\nrevision: 1\n---\n"
        )
    };
    std::fs::write(
        d.join("diary/Old.md"),
        file("Before", "2026-10-24T22:00:00Z", 90_000),
    )
    .unwrap();
    std::fs::write(
        d.join("diary/New.md"),
        file("After", "2026-10-25T08:00:00Z", 30),
    )
    .unwrap();
    // The older file is even newer on disk.
    age(&d.join("diary/New.md"), 100);
    let mut store = NotesStore::open(&d).unwrap();
    let list = store.list(Book::Diary).unwrap();
    assert_eq!((list.len(), list[0].title.as_str()), (1, "After"));
    assert_eq!(store.unreadable(Book::Diary)[0].file, "diary/Old.md");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn new_ids_never_reuse_a_seen_id() {
    let d = folder("high-water");
    let mut store = NotesStore::open(&d).unwrap();
    for t in ["a", "b", "c"] {
        store
            .add(Book::Memory, Writer::Agent, new(t, "x"), at(1))
            .unwrap();
    }
    // The dev breaks the highest id's frontmatter by hand.
    let c = d.join("memory/c.md");
    let fixed = read(&c);
    std::fs::write(&c, fixed.replace("title: c\n", "title: \"c\n")).unwrap();
    assert_eq!(store.list(Book::Memory).unwrap().len(), 2);
    assert_eq!(store.unreadable(Book::Memory)[0].file, "memory/c.md");
    let n = store
        .add(Book::Memory, Writer::Agent, new("d", "x"), at(2))
        .unwrap();
    assert_eq!(n.id, 4, "above the broken file's id 3");
    // A fresh store reads the broken file's `id:` line leniently.
    let mut fresh = NotesStore::open(&d).unwrap();
    assert_eq!(
        fresh
            .add(Book::Memory, Writer::Agent, new("e", "x"), at(3))
            .unwrap()
            .id,
        5
    );
    // Fixed again: no duplicate id.
    std::fs::write(&c, &fixed).unwrap();
    let ids: Vec<u32> = store
        .list(Book::Memory)
        .unwrap()
        .iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(ids, [1, 2, 3, 4, 5]);
    assert!(store.unreadable(Book::Memory).is_empty());
    // An id seen earlier counts even when its file is gone and the rest
    // has no `id:` line at all.
    std::fs::write(d.join("memory/e.md"), "no frontmatter\n").unwrap();
    std::fs::remove_file(d.join("memory/d.md")).unwrap();
    assert_eq!(store.list(Book::Memory).unwrap().len(), 3);
    assert_eq!(
        store
            .add(Book::Memory, Writer::Agent, new("f", "x"), at(4))
            .unwrap()
            .id,
        6
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_proposal_file_that_is_no_old_proposal_stays_unreadable() {
    let d = folder("not-old-proposal");
    std::fs::create_dir_all(d.join("proposals")).unwrap();
    let text = "Idea: x\nhttp://a.b";
    std::fs::write(d.join("proposals/Idea.md"), text).unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    assert!(store.list(Book::Proposals).unwrap().is_empty());
    let u = store.unreadable(Book::Proposals);
    assert_eq!(u[0].file, "proposals/Idea.md");
    assert!(u[0].reason.contains("no frontmatter"), "{}", u[0].reason);
    store
        .add(Book::Proposals, Writer::Agent, new("Other", "x"), at(1))
        .unwrap();
    assert_eq!(read(&d.join("proposals/Idea.md")), text);
    assert_eq!(proposal_files(&d), ["Idea.md", "Other.md"]);
    let _ = std::fs::remove_dir_all(&d);
}

/// A title change whose old file cannot be removed (Windows: another
/// program holds it without delete sharing).
#[cfg(windows)]
#[test]
fn a_locked_old_file_goes_only_while_unchanged() {
    use std::io::{Seek as _, Write as _};
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_SHARE_READ: u32 = 1;
    let d = folder("locked");
    let mut store = NotesStore::open(&d).unwrap();
    let a = store
        .add(Book::Memory, Writer::Agent, new("A", "a"), at(1))
        .unwrap();
    let b = store
        .add(Book::Memory, Writer::Agent, new("B", "b"), at(1))
        .unwrap();
    let lock = |name: &str| {
        std::fs::File::options()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ)
            .open(d.join("memory").join(name))
            .unwrap()
    };
    // Unchanged after the failed removal: removed on a later read.
    let held = lock("A.md");
    let ra = store
        .update(
            Book::Memory,
            Writer::Agent,
            a.id,
            &a.updated,
            title_change("A2"),
            at(2),
        )
        .unwrap();
    assert!(d.join("memory/A.md").is_file(), "still locked");
    assert_eq!(store.list(Book::Memory).unwrap()[0], ra);
    drop(held);
    store.list(Book::Memory).unwrap();
    assert!(!d.join("memory/A.md").exists());
    // Changed since (by whoever held it): left alone, a duplicate.
    let mut held = lock("B.md");
    let rb = store
        .update(
            Book::Memory,
            Writer::Agent,
            b.id,
            &b.updated,
            title_change("B2"),
            at(3),
        )
        .unwrap();
    held.seek(std::io::SeekFrom::End(0)).unwrap();
    held.write_all(b"\n").unwrap();
    drop(held);
    let list = store.list(Book::Memory).unwrap();
    assert!(d.join("memory/B.md").is_file(), "changed: kept");
    assert_eq!(list[1], rb);
    let u = store.unreadable(Book::Memory);
    assert_eq!(
        (u[0].file.as_str(), u[0].reason.clone()),
        ("memory/B.md", format!("duplicate id {}", b.id))
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// An old-layout folder: `diary.md` with a preamble, stray text, old and
/// new headings; `memory.md`; an old proposal.
fn old_folder(name: &str) -> PathBuf {
    let d = folder(name);
    let diary = "# Diary of Wren\nKept by hand before the game managed it.\n\n\
                 ## tick 40 - Woke up\nFirst body.\n\n\
                 ## Grove\n<!-- note id=7 created=50@2026-10-24T09:00:00Z updated=55@2026-10-24T09:05:00Z#r2 -->\nFound a grove.\n\n\
                 ## tick 60 - Woke up\nSecond body,\non two lines.\n";
    std::fs::write(d.join("diary.md"), diary).unwrap();
    std::fs::write(
        d.join("memory.md"),
        "# Memory\n\n## The store\n<!-- note id=1 created=10@2026-10-24 updated=10@2026-10-24 -->\nLocked.\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.join("proposals")).unwrap();
    std::fs::write(
        d.join("proposals/002-lamps.md"),
        "title: Lamps at the gate\nstatus: pursuing\nwritten: tick 70, 2026-10-24\n\nIt is dark.\n",
    )
    .unwrap();
    d
}

#[test]
fn old_folder_migrates_once() {
    let d = old_folder("old-folder");
    let old_diary = read(&d.join("diary.md"));
    let mut store = NotesStore::open_as(&d, owner()).unwrap();
    let diary = store.list(Book::Diary).unwrap();
    let got: Vec<(u32, &str, u64, &str)> = diary
        .iter()
        .map(|n| (n.id, n.title.as_str(), n.created.tick, n.body.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (7, "Grove", 50, "Found a grove."),
            (8, "Woke up", 40, "First body."),
            (9, "Woke up", 60, "Second body,\non two lines."),
            (
                10,
                "Kept from diary.md",
                0,
                "# Diary of Wren\nKept by hand before the game managed it."
            ),
        ]
    );
    // Stamps are kept, revisions included.
    assert_eq!(
        diary[0].updated,
        Stamp {
            tick: 55,
            date: "2026-10-24T09:05:00Z#r2".into()
        }
    );
    assert_eq!(
        diary[1].created,
        Stamp {
            tick: 40,
            date: String::new()
        }
    );
    assert_eq!(
        files_in(&d, "diary"),
        [
            "2026-10-24 Grove.md",
            "Kept from diary.md",
            "Woke up (2).md",
            "Woke up.md"
        ]
    );
    let grove = read(&d.join("diary/2026-10-24 Grove.md"));
    assert!(
        grove.starts_with(
            "---\nid: 7\ntype: diary\ntitle: Grove\naliases: []\ntags: [npc/diary]\n\
             agent: Wren Ashdown\nworld: Hollowmere\nauthor: agent\ncreated: 2026-10-24T09:00:00Z\n\
             created_tick: 50\nupdated: 2026-10-24T09:05:00Z\nupdated_tick: 55\nrevision: 2\n---\n"
        ),
        "{grove}"
    );
    let memory = store.list(Book::Memory).unwrap();
    assert_eq!(memory.len(), 1, "a lone `# Memory` title is not kept");
    assert_eq!(
        (
            memory[0].id,
            memory[0].title.as_str(),
            memory[0].body.as_str()
        ),
        (1, "The store", "Locked.")
    );
    // An old free-form date is kept verbatim, quoted.
    let store_file = read(&d.join("memory/The store.md"));
    assert!(
        store_file.contains("\ncreated: \"2026-10-24\"\ncreated_tick: 10\n"),
        "{store_file}"
    );
    let props = store.list(Book::Proposals).unwrap();
    assert_eq!(
        (props[0].id, props[0].title.as_str(), props[0].status),
        (2, "Lamps at the gate", Some(ProposalStatus::Pursuing))
    );
    // The old files are kept as `.bak`, byte for byte.
    assert!(!d.join("diary.md").exists() && !d.join("memory.md").exists());
    assert_eq!(read(&d.join("diary.md.bak")), old_diary);
    assert!(d.join("memory.md.bak").is_file());
    // A second open changes nothing.
    let stamps: Vec<(String, SystemTime)> = files_in(&d, "diary")
        .into_iter()
        .map(|f| (f.clone(), mtime(&d.join("diary").join(&f))))
        .collect();
    let mut again = NotesStore::open_as(&d, owner()).unwrap();
    assert_eq!(again.list(Book::Diary).unwrap(), diary);
    for (f, t) in &stamps {
        assert_eq!(mtime(&d.join("diary").join(f)), *t, "{f}");
    }
    // A run that stopped half-way: the old file is back without its `.bak`
    // and one entry is missing. Only that one is written again.
    std::fs::rename(d.join("diary.md.bak"), d.join("diary.md")).unwrap();
    std::fs::remove_file(d.join("diary/Woke up (2).md")).unwrap();
    let mut third = NotesStore::open_as(&d, owner()).unwrap();
    assert_eq!(third.list(Book::Diary).unwrap(), diary);
    assert!(third.unreadable(Book::Diary).is_empty());
    for (f, t) in &stamps {
        if f != "Woke up (2).md" {
            assert_eq!(mtime(&d.join("diary").join(f)), *t, "{f}");
        }
    }
    assert!(d.join("diary.md.bak").is_file() && !d.join("diary.md").exists());
    // New entries continue the numbering.
    assert_eq!(
        third
            .add(Book::Diary, Writer::Agent, new("Later", "x"), at(50))
            .unwrap()
            .id,
        11
    );
    // A `diary.md` that turns up next to its `.bak` is left alone.
    std::fs::write(d.join("diary.md"), "## tick 1 - Again\nx\n").unwrap();
    let mut fourth = NotesStore::open(&d).unwrap();
    assert_eq!(fourth.list(Book::Diary).unwrap().len(), 5);
    assert!(d.join("diary.md").is_file());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn old_diary_migrates_once() {
    let d = folder("old-diary");
    let old = "# Diary of Wren\n\n## tick 40 - Woke up\nFirst body.\n\n## tick 40 - Woke up\nSecond body,\non two lines.\n";
    std::fs::write(d.join("diary.md"), old).unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    let notes = store.list(Book::Diary).unwrap();
    let old_stamp = Stamp {
        tick: 40,
        date: String::new(),
    };
    assert_eq!(notes.iter().map(|n| n.id).collect::<Vec<_>>(), [1, 2]);
    for n in &notes {
        assert_eq!(n.title, "Woke up");
        assert_eq!((&n.created, &n.updated), (&old_stamp, &old_stamp));
    }
    assert_eq!(notes[0].body, "First body.");
    assert_eq!(notes[1].body, "Second body,\non two lines.");
    assert_eq!(files_in(&d, "diary"), ["Woke up (2).md", "Woke up.md"]);
    let migrated = read(&d.join("diary/Woke up.md"));
    assert_eq!(
        migrated,
        "---\nid: 1\ntype: diary\ntitle: Woke up\naliases: []\ntags: [npc/diary]\nagent: \"\"\n\
         world: \"\"\nauthor: agent\ncreated: \"\"\ncreated_tick: 40\nupdated: \"\"\nupdated_tick: 40\n\
         revision: 1\n---\nFirst body.\n"
    );
    assert_eq!(read(&d.join("diary.md.bak")), old);
    let mtime = mtime(&d.join("diary/Woke up.md"));
    // A second open reads the migrated files and does not write them again.
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Diary).unwrap(), notes);
    assert_eq!(read(&d.join("diary/Woke up.md")), migrated);
    assert_eq!(
        std::fs::metadata(d.join("diary/Woke up.md"))
            .unwrap()
            .modified()
            .unwrap(),
        mtime
    );
    // New entries continue the numbering.
    assert_eq!(
        again
            .add(Book::Diary, Writer::Agent, new("Later", "x"), at(50))
            .unwrap()
            .id,
        3
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn old_proposal_migrates() {
    let d = folder("old-proposal");
    std::fs::create_dir_all(d.join("proposals")).unwrap();
    let old = "title: Doors should say who has the key\nstatus: liked\nwritten: tick 1840, 2026-10-24\n\
               prompted by: the locked store door I could not explain\nmood: curious\n\n\
               The store door was locked and nothing said why.\n\nShow the key holder.\n";
    std::fs::write(d.join("proposals/003-door-hints.md"), old).unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    let list = store.list(Book::Proposals).unwrap();
    assert_eq!(list.len(), 1);
    let p = &list[0];
    let written = Stamp {
        tick: 1840,
        date: "2026-10-24".to_string(),
    };
    assert_eq!(p.id, 3);
    assert_eq!(p.title, "Doors should say who has the key");
    assert_eq!(p.status, Some(ProposalStatus::Liked));
    assert_eq!((&p.created, &p.updated), (&written, &written));
    assert_eq!(
        p.prompted_by.as_deref(),
        Some("the locked store door I could not explain")
    );
    assert_eq!(
        p.body,
        "The store door was locked and nothing said why.\n\nShow the key holder."
    );
    assert_eq!(proposal_files(&d), ["Doors should say who has the key.md"]);
    let file = d.join("proposals/Doors should say who has the key.md");
    // Unknown header lines survive an update too.
    let n = store
        .update(
            Book::Proposals,
            Writer::Player,
            3,
            &written,
            body_change("Shorter."),
            at(1900),
        )
        .unwrap();
    let text = read(&file);
    assert!(text.contains("\nmood: curious\n"), "{text}");
    assert!(
        text.contains("\ncreated: \"2026-10-24\"\ncreated_tick: 1840\n"),
        "{text}"
    );
    assert!(text.ends_with("\n---\nShorter.\n"), "{text}");
    assert_eq!(n.updated, at(1900));
    assert_eq!(
        store
            .add(
                Book::Proposals,
                Writer::Agent,
                new("Next idea", "x"),
                at(2000)
            )
            .unwrap()
            .id,
        4
    );
    assert_eq!(
        proposal_files(&d),
        ["Doors should say who has the key.md", "Next idea.md"]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn old_proposal_gets_frontmatter() {
    let d = folder("old-proposal-frontmatter");
    std::fs::create_dir_all(d.join("proposals")).unwrap();
    std::fs::write(
        d.join("proposals/003-door-hints.md"),
        "title: Doors: who has the key\nstatus: liked\nwritten: tick 1840, 2026-10-24\n\
         prompted by: the locked store door\nid: 3\nupdated: tick 1850, 2026-10-24T10:00:00Z#r2\n\
         mood: curious\nA line without a key\n\nShow the key holder.\n",
    )
    .unwrap();
    // A Phase A file with an `id:` line that is not its number, and one
    // without a title.
    std::fs::write(
        d.join("proposals/001-doors.md"),
        "# Doors\nid: 9\n\nBody.\n",
    )
    .unwrap();
    let mut store = NotesStore::open_as(&d, owner()).unwrap();
    let list = store.list(Book::Proposals).unwrap();
    let got: Vec<(u32, &str)> = list.iter().map(|n| (n.id, n.title.as_str())).collect();
    assert_eq!(got, [(3, "Doors: who has the key"), (9, "001-doors")]);
    assert_eq!(
        proposal_files(&d),
        ["001-doors.md", "Doors who has the key.md"]
    );
    assert_eq!(
        read(&d.join("proposals/Doors who has the key.md")),
        "---\nid: 3\ntype: proposal\ntitle: \"Doors: who has the key\"\naliases: []\n\
         tags: [npc/proposal]\nagent: Wren Ashdown\nworld: Hollowmere\nauthor: agent\n\
         created: \"2026-10-24\"\ncreated_tick: 1840\nupdated: 2026-10-24T10:00:00Z\n\
         updated_tick: 1850\nrevision: 2\nstatus: liked\nprompted_by: the locked store door\n\
         mood: curious\n---\nA line without a key\n\nShow the key holder.\n"
    );
    assert_eq!(
        list[0].updated,
        Stamp {
            tick: 1850,
            date: "2026-10-24T10:00:00Z#r2".into()
        }
    );
    let doors = read(&d.join("proposals/001-doors.md"));
    assert!(doors.starts_with("---\nid: 9\n"), "{doors}");
    assert!(doors.ends_with("---\n# Doors\n\nBody.\n"), "{doors}");
    // Opening again changes nothing.
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Proposals).unwrap(), list);
    assert_eq!(
        proposal_files(&d),
        ["001-doors.md", "Doors who has the key.md"]
    );
    // An old proposal whose id is taken gets the next free one.
    std::fs::write(d.join("proposals/003-other.md"), "title: Other\n\nx\n").unwrap();
    let list = again.list(Book::Proposals).unwrap();
    let got: Vec<(u32, &str)> = list.iter().map(|n| (n.id, n.title.as_str())).collect();
    assert_eq!(
        got,
        [
            (3, "Doors: who has the key"),
            (9, "001-doors"),
            (10, "Other")
        ]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn api_stamp_shape_unchanged() {
    let d = folder("stamp-shape");
    let mut store = NotesStore::open(&d).unwrap();
    let n = store
        .add(Book::Diary, Writer::Agent, new("t", "v1"), at(10))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&n.updated).unwrap(),
        serde_json::json!({"tick": 10, "date": "2026-10-25T10:00:10Z"})
    );
    // Two writes at the same tick and date: `#r2`, `#r3` in the date,
    // `revision` in the file, the date there without the suffix.
    let u1 = store
        .update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &n.updated,
            body_change("v2"),
            at(10),
        )
        .unwrap();
    let u2 = store
        .update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &u1.updated,
            body_change("v3"),
            at(10),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&u2.updated).unwrap(),
        serde_json::json!({"tick": 10, "date": "2026-10-25T10:00:10Z#r3"})
    );
    let text = read(&d.join("diary/2026-10-25 t.md"));
    assert!(
        text.contains("\nupdated: 2026-10-25T10:00:10Z\nupdated_tick: 10\nrevision: 3\n"),
        "{text}"
    );
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Diary).unwrap()[0].updated, u2.updated);
    // The stale check compares the rebuilt stamp.
    assert_eq!(
        again.update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &u1.updated,
            body_change("x"),
            at(11)
        ),
        Err(NotesError::Stale {
            id: n.id,
            current: u2.updated.clone()
        })
    );
    let u3 = again
        .update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &u2.updated,
            body_change("v4"),
            at(11),
        )
        .unwrap();
    assert_eq!(u3.updated, at(11));
    assert!(read(&d.join("diary/2026-10-25 t.md")).contains("\nrevision: 1\n"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn ids_are_sequential_per_book() {
    let d = folder("ids");
    let mut store = NotesStore::open(&d).unwrap();
    let ids: Vec<u32> = (0..3)
        .map(|i| {
            store
                .add(
                    Book::Diary,
                    Writer::Agent,
                    new(&format!("d{i}"), "x"),
                    at(i),
                )
                .unwrap()
                .id
        })
        .collect();
    assert_eq!(ids, [1, 2, 3]);
    assert_eq!(
        store
            .add(Book::Memory, Writer::Agent, new("m", "x"), at(5))
            .unwrap()
            .id,
        1
    );
    assert_eq!(
        store
            .add(Book::Proposals, Writer::Agent, new("p", "x"), at(5))
            .unwrap()
            .id,
        1
    );
    assert_eq!(
        store
            .add(Book::Proposals, Writer::Agent, new("q", "x"), at(6))
            .unwrap()
            .id,
        2
    );
    assert_eq!(
        store
            .add(Book::Diary, Writer::Player, new("d3", "x"), at(7))
            .unwrap()
            .id,
        4
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn limits_are_enforced() {
    let d = folder("limits");
    let mut store = NotesStore::open(&d).unwrap();
    assert_eq!((TITLE_MAX, BODY_MAX), (200, 8192));
    // Titles count characters, bodies bytes.
    let ok = store
        .add(
            Book::Diary,
            Writer::Agent,
            new(&"é".repeat(200), &"b".repeat(8192)),
            at(1),
        )
        .unwrap();
    assert_eq!(
        store.add(
            Book::Diary,
            Writer::Agent,
            new(&"t".repeat(201), "x"),
            at(2)
        ),
        Err(NotesError::TooLong {
            field: "title",
            max: 200
        })
    );
    assert_eq!(
        store.add(
            Book::Memory,
            Writer::Agent,
            new("t", &"b".repeat(8193)),
            at(2)
        ),
        Err(NotesError::TooLong {
            field: "body",
            max: 8192
        })
    );
    let err = store
        .update(
            Book::Diary,
            Writer::Agent,
            ok.id,
            &ok.updated,
            body_change(&"b".repeat(8193)),
            at(3),
        )
        .unwrap_err();
    assert_eq!(
        err,
        NotesError::TooLong {
            field: "body",
            max: 8192
        }
    );
    assert!(err.to_string().contains("8192"), "{err}");
    let title_err = NotesError::TooLong {
        field: "title",
        max: 200,
    }
    .to_string();
    assert!(title_err.contains("200"), "{title_err}");
    assert_eq!(store.list(Book::Diary).unwrap(), vec![ok]);
    assert!(store.list(Book::Memory).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn stale_update_is_rejected() {
    let d = folder("stale");
    let mut store = NotesStore::open(&d).unwrap();
    let n = store
        .add(
            Book::Diary,
            Writer::Agent,
            new("Grove", "Found a grove."),
            at(10),
        )
        .unwrap();
    // The dev saves first (read at tick 10) ...
    let dev = store
        .update(
            Book::Diary,
            Writer::Player,
            n.id,
            &n.updated,
            body_change("The dev's text."),
            at(20),
        )
        .unwrap();
    assert_eq!(dev.updated, at(20));
    assert_eq!(dev.created, at(10));
    let file = d.join("diary/2026-10-25 Grove.md");
    let before = read(&file);
    // ... then the agent, which also read it at tick 10.
    let err = store
        .update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &n.updated,
            body_change("The agent's text."),
            at(30),
        )
        .unwrap_err();
    assert_eq!(
        err,
        NotesError::Stale {
            id: n.id,
            current: at(20)
        }
    );
    assert!(
        err.to_string().contains("changed since you read it"),
        "{err}"
    );
    assert_eq!(read(&file), before);
    assert_eq!(store.list(Book::Diary).unwrap(), vec![dev]);
    assert_eq!(
        store.update(
            Book::Diary,
            Writer::Agent,
            9,
            &n.updated,
            body_change("x"),
            at(31)
        ),
        Err(NotesError::NotFound(9))
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn agent_cannot_set_status() {
    let d = folder("agent-status");
    let mut store = NotesStore::open(&d).unwrap();
    let p = store
        .add(
            Book::Proposals,
            Writer::Agent,
            new("Door hints", "Say who has the key."),
            at(5),
        )
        .unwrap();
    let file = d.join("proposals/Door hints.md");
    let before = read(&file);
    let change = NoteChange {
        title: None,
        body: Some("Liked it myself.".to_string()),
        status: Some(ProposalStatus::Liked),
    };
    let err = store
        .update(
            Book::Proposals,
            Writer::Agent,
            p.id,
            &p.updated,
            change,
            at(6),
        )
        .unwrap_err();
    assert_eq!(
        err,
        NotesError::Forbidden("only the player sets a proposal's status")
    );
    assert_eq!(err.to_string(), "only the player sets a proposal's status");
    assert_eq!(read(&file), before);
    // The agent may still edit its proposal's text.
    let edited = store
        .update(
            Book::Proposals,
            Writer::Agent,
            p.id,
            &p.updated,
            body_change("Clearer now."),
            at(7),
        )
        .unwrap();
    assert_eq!(
        (edited.body.as_str(), edited.status),
        ("Clearer now.", Some(ProposalStatus::New))
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn player_sets_status() {
    let d = folder("player-status");
    let mut store = NotesStore::open(&d).unwrap();
    let p = store
        .add(
            Book::Proposals,
            Writer::Agent,
            new("Door hints", "Say who has the key."),
            at(5),
        )
        .unwrap();
    let mut seen = p.updated.clone();
    for status in [
        ProposalStatus::Liked,
        ProposalStatus::Pursuing,
        ProposalStatus::Rejected,
    ] {
        let change = NoteChange {
            title: None,
            body: None,
            status: Some(status),
        };
        let n = store
            .update(Book::Proposals, Writer::Player, p.id, &seen, change, at(9))
            .unwrap();
        assert_eq!(n.status, Some(status));
        assert_eq!(n.body, "Say who has the key.");
        seen = n.updated;
    }
    let text = read(&d.join("proposals/Door hints.md"));
    assert!(text.contains("\nstatus: rejected\n"), "{text}");
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(
        again.list(Book::Proposals).unwrap()[0].status,
        Some(ProposalStatus::Rejected)
    );
    // Diary and memory entries have no status at all.
    let e = store
        .add(Book::Diary, Writer::Player, new("e", "x"), at(10))
        .unwrap();
    let change = NoteChange {
        title: None,
        body: None,
        status: Some(ProposalStatus::Liked),
    };
    assert!(matches!(
        store.update(
            Book::Diary,
            Writer::Player,
            e.id,
            &e.updated,
            change,
            at(11)
        ),
        Err(NotesError::Forbidden(_))
    ));
    let mut prompted = new("m", "x");
    prompted.prompted_by = Some("why".to_string());
    assert!(matches!(
        store.add(Book::Memory, Writer::Player, prompted, at(12)),
        Err(NotesError::Forbidden(_))
    ));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn unplaceable_text_survives_update() {
    let d = folder("unplaceable");
    let preamble = "# Diary of Wren\nKept by hand before the game managed it.\n\n";
    let loose = "> A block with no heading, typed in by the dev.\n> Two lines of it.\n\n";
    let first = "## Grove\n<!-- note id=1 created=10@2026-10-24 updated=10@2026-10-24 -->\nFound a grove.\n\n";
    let second = "## Wall\r\n<!-- note id=2 created=20@2026-10-24 updated=20@2026-10-24 -->\r\n  odd   spacing kept\r\n\r\n\r\n";
    std::fs::write(
        d.join("diary.md"),
        format!("{preamble}{loose}{first}{second}"),
    )
    .unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    let notes = store.list(Book::Diary).unwrap();
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[1].body, "  odd   spacing kept");
    // The text before the first heading is its own entry.
    assert_eq!(notes[2].title, "Kept from diary.md");
    assert_eq!(
        notes[2].body,
        "# Diary of Wren\nKept by hand before the game managed it.\n\n\
         > A block with no heading, typed in by the dev.\n> Two lines of it."
    );
    let kept = d.join("diary/Kept from diary.md");
    let wall = d.join("diary/2026-10-24 Wall.md");
    let (kept_before, wall_before) = (read(&kept), read(&wall));
    store
        .update(
            Book::Diary,
            Writer::Agent,
            1,
            &notes[0].updated,
            body_change("A bigger grove."),
            at(30),
        )
        .unwrap();
    assert!(read(&d.join("diary/2026-10-24 Grove.md")).contains("A bigger grove."));
    assert_eq!(read(&kept), kept_before);
    assert_eq!(read(&wall), wall_before);
    // Adding keeps them too.
    store
        .add(Book::Diary, Writer::Agent, new("After", "x"), at(40))
        .unwrap();
    assert_eq!(read(&kept), kept_before);
    assert_eq!(read(&wall), wall_before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn body_lines_that_look_like_headings_round_trip() {
    let d = folder("heading-body");
    let mut store = NotesStore::open(&d).unwrap();
    let body = "Before\n## not a heading\n\\## escaped already\n##\nafter";
    let n = store
        .add(Book::Memory, Writer::Agent, new("Tricky", body), at(1))
        .unwrap();
    store
        .add(Book::Memory, Writer::Agent, new("Next", "y"), at(2))
        .unwrap();
    let mut again = NotesStore::open(&d).unwrap();
    let list = again.list(Book::Memory).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], n);
    assert_eq!(list[0].body, body);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn external_edit_is_picked_up() {
    let d = folder("external");
    let mut store = NotesStore::open(&d).unwrap();
    let n = store
        .add(
            Book::Diary,
            Writer::Agent,
            new("Grove", "first body"),
            at(10),
        )
        .unwrap();
    store
        .add(Book::Proposals, Writer::Agent, new("Idea", "x"), at(10))
        .unwrap();
    assert_eq!(store.list(Book::Diary).unwrap().len(), 1);
    // The dev edits by hand while the game runs.
    let path = d.join("diary/2026-10-25 Grove.md");
    let edited = read(&path).replace("first body", "edited by hand, at length");
    std::fs::write(&path, edited).unwrap();
    let hand = "---\ntitle: Hand entry\ncreated_tick: 99\nmood: odd\n---\nWritten in an editor.\n";
    std::fs::write(d.join("diary/Hand entry.md"), hand).unwrap();
    std::fs::write(
        d.join("proposals/007-by-hand.md"),
        "title: By hand\n\nA proposal.\n",
    )
    .unwrap();
    let list = store.list(Book::Diary).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].body, "edited by hand, at length");
    assert_eq!(
        (list[1].id, list[1].title.as_str(), list[1].created.tick),
        (2, "Hand entry", 99)
    );
    // The hand-made entry got its id, written back once; the rest kept.
    assert_eq!(
        read(&d.join("diary/Hand entry.md")),
        format!("---\nid: 2\n{}", &hand[4..])
    );
    let props = store.list(Book::Proposals).unwrap();
    assert_eq!(
        props
            .iter()
            .map(|p| (p.id, p.title.as_str()))
            .collect::<Vec<_>>(),
        [(1, "Idea"), (7, "By hand")]
    );
    assert_eq!(props[1].status, Some(ProposalStatus::New));
    // The hand edit is a new version: same tick, a revision on the date,
    // written back so it survives a restart.
    assert_eq!(list[0].updated.tick, 10);
    assert_eq!(list[0].updated.date, format!("{}#r2", n.updated.date));
    assert_eq!(list[0].created, n.created);
    assert!(read(&path).contains("\nrevision: 2\n"));
    // An agent holding the stamp from before the hand edit is rejected ...
    let before = read(&path);
    assert_eq!(
        store.update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &n.updated,
            body_change("the agent's stale text"),
            at(11),
        ),
        Err(NotesError::Stale {
            id: n.id,
            current: list[0].updated.clone(),
        })
    );
    assert_eq!(read(&path), before);
    // ... and succeeds after listing again.
    let u = store
        .update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &list[0].updated,
            body_change("again"),
            at(11),
        )
        .unwrap();
    assert_eq!(u.body, "again");
    assert!(read(&d.join("diary/Hand entry.md")).contains("Written in an editor."));
    // A hand edit of a proposal's text bumps its version too.
    let p_path = d.join("proposals/Idea.md");
    let p_before = props[0].clone();
    std::fs::write(&p_path, read(&p_path).replace("\nx\n", "\nhand edited\n")).unwrap();
    let p_now = store.list(Book::Proposals).unwrap()[0].clone();
    assert_eq!(p_now.body, "hand edited");
    assert_ne!(p_now.updated, p_before.updated);
    assert!(matches!(
        store.update(
            Book::Proposals,
            Writer::Agent,
            1,
            &p_before.updated,
            body_change("y"),
            at(12)
        ),
        Err(NotesError::Stale { .. })
    ));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn same_tick_writes_get_distinct_stamps() {
    let d = folder("same-tick");
    let mut store = NotesStore::open(&d).unwrap();
    let paused = Stamp {
        tick: 5,
        date: String::new(),
    };
    for (book, when) in [
        (Book::Diary, at(10)),
        (Book::Memory, paused.clone()),
        (Book::Proposals, at(10)),
    ] {
        // A paused game: every write carries the same tick and date.
        let n = store
            .add(book, Writer::Agent, new("t", "v1"), when.clone())
            .unwrap();
        let u1 = store
            .update(
                book,
                Writer::Player,
                n.id,
                &n.updated,
                body_change("v2"),
                when.clone(),
            )
            .unwrap();
        assert_ne!(u1.updated, n.updated);
        assert_eq!(u1.updated.tick, when.tick);
        assert_eq!(u1.updated.date, format!("{}#r2", when.date));
        // The other writer still holds the first stamp: rejected.
        assert_eq!(
            store.update(
                book,
                Writer::Agent,
                n.id,
                &n.updated,
                body_change("v3"),
                when.clone()
            ),
            Err(NotesError::Stale {
                id: n.id,
                current: u1.updated.clone(),
            })
        );
        let u2 = store
            .update(
                book,
                Writer::Agent,
                n.id,
                &u1.updated,
                body_change("v3"),
                when.clone(),
            )
            .unwrap();
        assert_eq!(u2.updated.date, format!("{}#r3", when.date));
        // A later tick drops the revision.
        let mut later = when.clone();
        later.tick += 1;
        let u3 = store
            .update(
                book,
                Writer::Agent,
                n.id,
                &u2.updated,
                body_change("v4"),
                later.clone(),
            )
            .unwrap();
        assert_eq!(u3.updated, later);
        // The revisions round-trip through the files.
        let mut again = NotesStore::open(&d).unwrap();
        assert_eq!(again.list(book).unwrap(), vec![u3]);
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn expected_date_is_cleaned_like_at() {
    let d = folder("clean-expected");
    let mut store = NotesStore::open(&d).unwrap();
    let when = Stamp {
        tick: 3,
        date: "2026-10-25 10:00".to_string(),
    };
    let n = store
        .add(Book::Diary, Writer::Agent, new("t", "x"), when.clone())
        .unwrap();
    assert_eq!(n.updated.date, "2026-10-25_10:00");
    // The caller echoes its own `at`, space and all.
    let u = store
        .update(
            Book::Diary,
            Writer::Agent,
            n.id,
            &when,
            body_change("y"),
            at(4),
        )
        .unwrap();
    assert_eq!(u.body, "y");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn blank_line_before_id_comment_is_tolerated() {
    let d = folder("blank-before-comment");
    // What a markdown formatter (MD022) makes of an entry.
    let text = "# Diary\n\n## Grove\n\n<!-- note id=5 created=10@d1 updated=12@d2 -->\n\nFound a grove.\n\n\
                ## Wall\n<!-- note id=6 created=20@d3 updated=20@d3 -->\nA wall.\n";
    std::fs::write(d.join("diary.md"), text).unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    let list = store.list(Book::Diary).unwrap();
    assert_eq!(list.iter().map(|n| n.id).collect::<Vec<_>>(), [5, 6]);
    assert_eq!(list[0].title, "Grove");
    assert_eq!(list[0].body, "Found a grove.");
    assert_eq!(
        (&list[0].created, &list[0].updated),
        (
            &Stamp {
                tick: 10,
                date: "d1".to_string()
            },
            &Stamp {
                tick: 12,
                date: "d2".to_string()
            }
        )
    );
    // The migration kept the old file as it was.
    assert_eq!(read(&d.join("diary.md.bak")), text);
    store
        .update(
            Book::Diary,
            Writer::Agent,
            5,
            &list[0].updated,
            body_change("Bigger."),
            at(30),
        )
        .unwrap();
    let after = read(&d.join("diary/Grove.md"));
    assert!(
        after.contains("\ncreated: \"d1\"\ncreated_tick: 10\nupdated: 2026-10-25T10:00:30Z\n"),
        "{after}"
    );
    assert_eq!(
        read(&d.join("diary/Wall.md")),
        "---\nid: 6\ntype: diary\ntitle: Wall\naliases: []\ntags: [npc/diary]\nagent: \"\"\n\
         world: \"\"\nauthor: agent\ncreated: \"d3\"\ncreated_tick: 20\nupdated: \"d3\"\n\
         updated_tick: 20\nrevision: 1\n---\nA wall.\n"
    );
    let mut again = NotesStore::open(&d).unwrap();
    assert_eq!(again.list(Book::Diary).unwrap()[0].body, "Bigger.");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bom_and_crlf_are_accepted() {
    let d = folder("bom-crlf");
    std::fs::create_dir_all(d.join("proposals")).unwrap();
    std::fs::write(
        d.join("diary.md"),
        "\u{feff}# Diary\r\n\r\n## tick 5 - Start\r\nLine one\r\nLine two\r\n",
    )
    .unwrap();
    std::fs::write(
        d.join("proposals/001-t.md"),
        "\u{feff}title: T\r\nstatus: Pursuing\r\nwritten: tick 7, d\r\n\r\nBody\r\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.join("memory")).unwrap();
    std::fs::write(
        d.join("memory/M.md"),
        "\u{feff}---\r\nid: 3\r\ntitle: M\r\n---\r\nm body\r\n",
    )
    .unwrap();
    let mut store = NotesStore::open(&d).unwrap();
    let diary = store.list(Book::Diary).unwrap();
    assert_eq!(diary.len(), 1);
    assert_eq!(
        (diary[0].title.as_str(), diary[0].body.as_str()),
        ("Start", "Line one\nLine two")
    );
    assert_eq!(diary[0].created.tick, 5);
    let props = store.list(Book::Proposals).unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!((props[0].id, props[0].title.as_str()), (1, "T"));
    assert_eq!(props[0].status, Some(ProposalStatus::Pursuing));
    assert_eq!(props[0].body, "Body");
    assert_eq!(
        props[0].created,
        Stamp {
            tick: 7,
            date: "d".to_string()
        }
    );
    let memory = store.list(Book::Memory).unwrap();
    assert_eq!((memory[0].id, memory[0].body.as_str()), (3, "m body"));
    // The migrated files keep their CRLF line ends.
    store
        .add(Book::Diary, Writer::Agent, new("Second", "a\nb"), at(8))
        .unwrap();
    store
        .update(
            Book::Memory,
            Writer::Agent,
            3,
            &memory[0].updated,
            body_change("c\nd"),
            at(9),
        )
        .unwrap();
    for p in [
        d.join("diary/Start.md"),
        d.join("proposals/T.md"),
        d.join("memory/M.md"),
    ] {
        let t = read(&p);
        assert!(!t.starts_with('\u{feff}'), "{t:?}");
        assert!(
            t.match_indices('\n')
                .all(|(i, _)| i > 0 && t.as_bytes()[i - 1] == b'\r'),
            "{t:?}"
        );
    }
    assert_eq!(store.list(Book::Diary).unwrap()[1].body, "a\nb");
    assert_eq!(store.list(Book::Memory).unwrap()[0].body, "c\nd");
    let _ = std::fs::remove_dir_all(&d);
}
