### Task 7: Content tree pipeline and per-path origins (C6, part 1)

**Track:** contract + content · **Refs:** ADR-0020 · **Lane 2** (`C:\fl\m2p07`, after 2 and 3)

**Files:**
- Create: `crates/fl_content/src/tree.rs`, `crates/fl_content/tests/tree.rs`
- Modify: `crates/fl_content/src/{object,inherit,mods,db,error,lib}.rs`
- Blessed: `xtask/snapshots/api/fl_content.txt`

**Interfaces:**
- Consumes: `load_dirs`, `parse_object`, `resolve_objects`, `DefRegistry` (component, kind and rules-section seeds), `ContentError`.
- Produces:
```rust
// crates/fl_content/src/tree.rs
#[derive(Clone, Debug, PartialEq)] pub struct Span { pub line: u32, pub col: u32 }
#[derive(Clone, Debug, PartialEq)]
pub enum ContentTree {
    Unit, Bool(bool), Int(i128), Float(f64), Str(String),
    Ident(String),                                            // a bare identifier: an enum unit variant (`Tracked`)
    Option(Option<Box<Node>>), Seq(Vec<Node>), Tuple(Vec<Node>),
    Map(Vec<(Node, Node)>),                                   // `{ k: v }`, in written order
    Struct { name: Option<String>, fields: Vec<(String, Node)> },        // `Name(a: 1)` or `(a: 1)`
    TupleStruct { name: Option<String>, items: Vec<Node> },              // `Name(1, 2)`
}
#[derive(Clone, Debug, PartialEq)] pub struct Node { pub value: ContentTree, pub span: Span }
pub fn parse_ron(file: &Path, text: &str) -> Result<Node, ContentError>
    // the RON subset our content uses: // and /* */ comments, named and unnamed structs and tuples, lists, maps, strings with \" \\ \n \t \u{…}
    // escapes, decimal integers, floats (digits with '.' or exponent; parsed with str::parse::<f64>), true/false, Some(..)/None, (), bare
    // identifiers, trailing commas; anything else is an error naming file, line and column
pub fn to_ron(node: &Node) -> String                         // a printer whose output parse_ron reads back equal (spans aside)
pub struct TreeDeserializer<'a> { .. }                       // serde::Deserializer over a &Node: enums from Ident/Str (unit), single-field
                                                             //   Struct/TupleStruct named as the variant, or one-entry Map; is_human_readable true;
                                                             //   errors carry the field path and the node's span
pub fn from_tree<T: DeserializeOwned>(node: &Node, file: &Path) -> Result<T, ContentError>
pub fn path_key(doc: &str, path: &[PathSeg]) -> String      // "unit:base.de.pz3/components.Hull.footprint.0", "rules/Movement.min_speed_frac"
pub enum PathSeg { Field(String), Index(usize) }
// crates/fl_content/src/db.rs
#[derive(Clone, Debug, PartialEq, Eq)] pub struct Origin { pub mod_id: String, pub file: PathBuf }
impl ContentDb { pub fn origins(&self) -> &BTreeMap<String, Origin>; }   // every leaf of every document: objects (abstract ones too), kinds, rules
```
- `load_dirs` keeps its signature. Its phases become: discover → `parse_ron` per file → inherit per component on trees (child replaces the parent's component whole; `remove` drops it; inherited leaves keep the parent's origin) → typed deserialize through the registry seeds over `TreeDeserializer` → link → freeze → hash. Migrate and patch arrive in Task 10, between parse and inherit.

- [ ] **Step 1: Write the failing tests** (`crates/fl_content/tests/tree.rs`):
```rust
#[test] fn identifiers_survive_parsing()          // "(class: Tracked, p: Box(size: (1.0, 2.0)))" → Ident("Tracked"), Struct { name: Some("Box") }
#[test] fn floats_parse_like_ron()                // for "0.05", "2.8", "1e-3", "-4.0", "40": from_tree::<Fx>() == ron::from_str::<Fx>()
#[test] fn printer_round_trips()                  // proptest (proptest_policy::config(256, 1024)): random trees → to_ron → parse_ron equal, spans aside
#[test] fn unsupported_syntax_names_position()    // "(a: 'c')" → error "line 1, column 5" and "char literals are not supported"
#[test] fn typed_errors_name_field_path()         // a Hull with "hp: -1" → error path "components.Hull.hp" with its line
#[test] fn tree_load_matches_typed_load()         // the base mod and every fixture mod (crates/fl_sim/tests/fixtures/**, fl_cli fixtures): the rules
                                                  //   hash equals the hash committed for it (base: data-check's rules hash at the task base)
#[test] fn origins_cover_every_leaf()             // base: "unit:base.de.pz3/components.Hull.hp" → mod "base", file …/pz3.unit.ron;
                                                  //   "unit:base.de.pz2/components.Selectable" absent (pz2 inherits it: origin under the template's key)
#[test] fn inherited_leaf_keeps_parent_origin()   // a fixture child without "Hull": its resolved Hull came from the template file
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content --test tree`. Expected: FAIL.
- [ ] **Step 3: Implement.** Before changing `load_dirs`, record the current rules hash of every fixture and base mod (`cargo run -q -p fl_cli -- data-check <dir> --json`) into the test's expectations; `tree_load_matches_typed_load` must then pass unchanged, which proves the new pipeline produces the same typed content.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_catalog -p fl_sim -p fl_test_utils -p fl_cli`. Expected: PASS; golden replays verify unchanged (the rules hash is unchanged).
- [ ] **Step 5: Regenerate** the `fl_content` API snapshot (`cargo xtask api`); the lead blesses it: `M2P Task 7: content tree and origins`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track content`. Expected: PASS, with the content tests run.
- [ ] **Step 7: Commit** `feat(fl_content): lossless RON tree, typed deserialize from trees, per-path origins` with trailers `Change-Class: R`, `Refs: ADR-0020`, `Blessed: …`.

---

## Phase II — provenance, tracing, acceptance pipeline, order queue

