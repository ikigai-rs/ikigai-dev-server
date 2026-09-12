//! `ikigai-conformance` over the kernel this binary actually serves.
//!
//! ## This crate binds ZERO endpoints, and that is the whole shape of the problem
//!
//! Everything behind `ikigai-dev`'s socket comes from a module crate with its own repo
//! and its own suite: `ikigai-repo` (`urn:system:exec`, `urn:repo:*`), `ikigai-rdf`,
//! `ikigai-sparql`, `ikigai-browse` (the browse family + `urn:iki:annotation:*`) and
//! `ikigai-llm`. So no finding below is this crate's to FIX — they are recorded, attributed
//! to the crate that owns them, and left. What IS this crate's is the composition: which
//! spaces, in which order, under which overlay, behind which renderer. [`SERVED`] pins that
//! catalog entry by entry, and every finding is required to name an id in it — so the day
//! this server binds something of its own, the walk goes red and whoever added it has to
//! classify it here, decide its cacheability, and say whether it may be reached over a
//! mount.
//!
//! The walk runs over [`ikigai_dev_server::compose`] — the same function `main` calls, not
//! a re-creation of it. `ikigai-web` shipped the opposite arrangement (its HTTP tests built
//! their kernels by hand, the one function with the defect was never under test, and the
//! daemon answered 500 on a link it emitted itself); the composition is the only thing here
//! worth testing, so the test has to hold the real one.
//!
//! ## This process is the PEER on the other end of everyone else's mount
//!
//! `mount = "prefer urn:repo:=~/.ikigai/dev.sock"` in some other host's config makes this
//! server's catalog part of THAT kernel's manifold. Three properties follow, and each has a
//! test here because the failure mode of each is silence somewhere else:
//!
//! - [`every_served_entry_answers_meta_with_a_real_contract`] — the Meta renderer is what a
//!   mounting client reads a contract through, and losing it degrades rather than fails.
//! - [`only_the_style_face_carries_a_thread_a_mount_would_erase`] — golden threads do not
//!   cross a wire, so this is the blast radius of that hole, enumerated.
//! - [`the_bare_annotation_minting_iri_is_bound_but_not_enumerated`] — the only write path
//!   of the annotation overlay is not in the catalog, which is why the operator's mount
//!   line is written without a trailing colon.
//!
//! ## What the walk is not allowed to do — and the one thing it now IS allowed to do
//!
//! Most checks FIRE endpoints under `Capability::root()`, and this composition is a
//! development seam: the first bare run really did execute `git` through `urn:system:exec`,
//! shell out to `gh` over the network through six PR facades, and POST to a local inference
//! server through `urn:llm:ollama:ask`. [`WAIVED`] names those endpoints and, per endpoint,
//! the checks that fire them.
//!
//! ★ **`ENFORCED` is deliberately NOT among them.** It is the one invoking check that
//! resolves under `Capability::scoped([])`, so on an action declaring a `requires` the
//! kernel refuses before dispatch and nothing is entered. Until `ikigai-conformance` 0.2.0
//! there was no way to say that — `Suite::opt_out` drops every invoking check — so the
//! twenty-two whole-endpoint waivers here bought silence on exactly the actions whose
//! capability gate matters most. 0.2.0's `Suite::opt_out_check` narrows twenty-one of them
//! to the checks that really fire, and the result is that this composition's subprocess
//! seam, its ten `gh`-backed PR facades and its outbound-inference family have their
//! declared gates under test for the first time. The twenty-second came off entirely; see
//! [`WAIVED`].
//!
//! ⚠ A typed `Denied` is not proof that nothing ran first (conformance PENDING #117), so
//! the narrowing is licensed by `tests/enforced.rs` — a separate test binary that fires
//! every gated action under no grants while a spawn log, a loopback connection counter,
//! the annotation store's quad count and the scratch tree watch for effects, each witness
//! proved live first.
//!
//! ## Fixtures and declarations, stated here because 0.1.0 printed neither
//!
//! (conformance PENDING #3/#57.) Real Turtle for `rdf-union` / `rdf-diff` /
//! `rdf-transrept`, real queries for the four `sparql-*` reads, `path` bindings into the
//! scratch tree for `browse-tree` / `browse-file` / `browse-hash` / `browse-explain-versions`
//! / `browse-annotations`, `needs=text` for `llm-select`, and an `id` binding onto a seeded
//! annotation. Without them those endpoints report "did not resolve with the minimal inputs"
//! and every RDF face they serve goes unprobed — a walk that looks like a verdict and saw
//! nothing. The fixtures took the bare run from 124 findings to 103, and every one of the 21
//! it removed was the walk failing to call rather than the module failing to conform.
//!
//! Two declarations, each an assertion this file is making on another crate's behalf:
//! `pure("rdf-transrept")` (transreption is a function of its input bytes — the endpoint's
//! own source says so) and `namespace("http://www.w3.org/ns/oa#")` (the W3C Web Annotation
//! Data Model, which the suite's well-known list omits). Both are argued at their call site.
//!
//! ## What the walk still reports, after all that
//!
//! 28 findings, none of them this crate's, and 23 of them `ikigai-llm`'s — which is the
//! whole shape of the number. When this file was written the count was 103 across five
//! module crates; `ikigai-repo`, `ikigai-rdf`, `ikigai-sparql` and `ikigai-browse` have
//! since adopted the suite themselves and a fresh resolve picks up their fixes, so what is
//! left is concentrated in the one dependency this manifest still pins BELOW the
//! ecosystem's line (`ikigai-llm = "0.10.0"`; see the manifest and this repo's PENDING §2).
//! Nineteen `ARGSPECS` (untyped `urn:llm:*` inputs), one `OUTPUTS` on `browse-file`
//! (`text/markdown` served, not declared), two failed minimal resolutions the suite asks
//! for fixtures for (`sparql-update`, `annotation`'s Sink), and six `CACHEABLE`
//! (`Expiry::Never` with no thread). Zero `ENFORCED`, zero `DECLARATIONS`, zero `NAMES`,
//! zero `REQUIRES-VERB`, zero `SKOLEM-RDF`, zero `VOCABULARY`, zero `PIPELINE`.
//!
//! The count is printed, never pinned: it drops on its own as each module's own adoption
//! lands, and pinning it would make another crate's improvement a failure here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ikigai_conformance::{Check, Fixture, Report, Suite};
use ikigai_core::{ArgRef, Capability, Error, Expiry, Iri, Kernel, Representation, Request, Verb};
use ikigai_dev_server::{browse, compose, config::BrowseSettings, LIMITS};
use ikigai_sparql::Store;

/// The browse root name every fixture below uses.
const ROOT: &str = "demo";

// ---------------------------------------------------------------------------
// The served catalog, pinned entry by entry.
// ---------------------------------------------------------------------------

/// Every pattern this server binds, with the description id it answers under and the crate
/// that owns it. This is the linkage-gating claim as a machine-checked fact rather than a
/// README sentence: no calendar, no EventKit, no filesystem, no outbound HTTP resource —
/// only these, because only these are linked.
///
/// `urn:kernel:*` is core's own and excluded here exactly as the suite's walk excludes it.
const SERVED: &[(&str, &str, &str)] = &[
    ("urn:system:exec", "system-exec", "ikigai-repo"),
    ("urn:repo:status", "repo-status", "ikigai-repo"),
    ("urn:repo:log", "repo-log", "ikigai-repo"),
    ("urn:repo:branch", "repo-branch", "ikigai-repo"),
    ("urn:repo:list", "repo-list", "ikigai-repo"),
    ("urn:repo:pr:checks", "repo-pr-checks", "ikigai-repo"),
    ("urn:repo:pr:view", "repo-pr-view", "ikigai-repo"),
    ("urn:repo:pr:list", "repo-pr-list", "ikigai-repo"),
    ("urn:repo:pr:files", "repo-pr-files", "ikigai-repo"),
    ("urn:repo:pr:diff", "repo-pr-diff", "ikigai-repo"),
    ("urn:rdf:union", "rdf-union", "ikigai-rdf"),
    ("urn:rdf:diff", "rdf-diff", "ikigai-rdf"),
    ("urn:rdf:transrept", "rdf-transrept", "ikigai-rdf"),
    ("urn:sparql:select", "sparql-select", "ikigai-sparql"),
    ("urn:sparql:ask", "sparql-ask", "ikigai-sparql"),
    ("urn:sparql:describe", "sparql-describe", "ikigai-sparql"),
    ("urn:sparql:construct", "sparql-construct", "ikigai-sparql"),
    ("urn:sparql:update", "sparql-update", "ikigai-sparql"),
    ("urn:repo:demo:tree", "browse-tree", "ikigai-browse"),
    ("urn:repo:demo:tree:{path}", "browse-tree", "ikigai-browse"),
    ("urn:repo:demo:file:{path}", "browse-file", "ikigai-browse"),
    ("urn:repo:demo:state", "browse-state", "ikigai-browse"),
    ("urn:repo:demo:hash", "browse-hash", "ikigai-browse"),
    ("urn:repo:demo:hash:{path}", "browse-hash", "ikigai-browse"),
    ("urn:repo:style", "browse-style", "ikigai-browse"),
    ("urn:repo:demo:prs", "browse-prs", "ikigai-browse"),
    (
        "urn:repo:demo:prs:{path}",
        "browse-prs-scoped",
        "ikigai-browse",
    ),
    ("urn:repo:demo:pr:{n}", "browse-pr", "ikigai-browse"),
    (
        "urn:repo:demo:explain-versions",
        "browse-explain-versions",
        "ikigai-browse",
    ),
    (
        "urn:repo:demo:explain-versions:{path}",
        "browse-explain-versions",
        "ikigai-browse",
    ),
    ("urn:repo:demo:explain", "browse-explain", "ikigai-browse"),
    (
        "urn:repo:demo:explain:{path}",
        "browse-explain",
        "ikigai-browse",
    ),
    (
        "urn:repo:demo:review:{path}",
        "browse-review",
        "ikigai-browse",
    ),
    (
        "urn:repo:demo:pr:{n}:explain",
        "browse-pr-explain",
        "ikigai-browse",
    ),
    (
        "urn:repo:demo:pr:{n}:review",
        "browse-pr-review",
        "ikigai-browse",
    ),
    ("urn:iki:annotation:{id}", "annotation", "ikigai-browse"),
    (
        "urn:repo:demo:annotations",
        "browse-annotations",
        "ikigai-browse",
    ),
    (
        "urn:repo:demo:annotations:{path}",
        "browse-annotations",
        "ikigai-browse",
    ),
    ("urn:llm:ask", "llm-ask", "ikigai-llm"),
    ("urn:llm:config", "llm-config", "ikigai-llm"),
    ("urn:llm:models", "llm-models", "ikigai-llm"),
    ("urn:llm:select", "llm-select", "ikigai-llm"),
    ("urn:llm:ollama:ask", "llm-ollama-ask", "ikigai-llm"),
    ("urn:llm:ollama:up", "llm-ollama-up", "ikigai-llm"),
    (
        "urn:llm:ollama:installed",
        "llm-ollama-installed",
        "ikigai-llm",
    ),
    ("urn:llm:ollama:model", "llm-ollama-model", "ikigai-llm"),
];

/// The URN families this server occupies, and the reason the list is worth stating.
///
/// A client may mount this socket under an ALIAS, which re-prefixes the whole remote
/// catalog — the peer's `urn:kernel:catalog` arrives as `urn:<alias>:kernel:catalog`
/// (conformance PENDING #134). Nothing this server binds lives under a bare `urn:kernel:`,
/// so a re-prefixed kernel operation can only collide here if the alias prefix is one of
/// these families AND the next segment is a name this server already claims. The
/// `urn:repo:` family is the one that matters, because `urn:repo:=` is the mount line the
/// README tells operators to write: a re-prefixed op would arrive as `urn:repo:kernel:*`,
/// and `urn:repo:{root}:{op}` is exactly the browse grammar's shape. ⚠ `kernel` is NOT in
/// `config`'s reserved-root list (`status`, `log`, `branch`, `list`, `pr`), so an operator
/// with a directory named `kernel` would bind `urn:repo:kernel:tree` and put a real
/// endpoint in the space a re-prefixed kernel op lands in. No such root exists today and no
/// alias mount points here today; recorded rather than fixed.
const FAMILIES: &[&str] = &[
    "urn:system:exec",
    "urn:repo:",
    "urn:rdf:",
    "urn:sparql:",
    "urn:llm:",
    "urn:iki:annotation",
];

// ---------------------------------------------------------------------------
// The fixture composition: hermetic, and the same `compose` main calls.
// ---------------------------------------------------------------------------

/// A scratch config home for the whole test binary.
///
/// `ikigai-browse`'s `.app("dev-server")` layering reads `a11y.toml` out of the config home,
/// so `urn:repo:style` would otherwise resolve against the developer's real
/// `~/.config/ikigai` — a test that reads the real home and asserts only type-shape reads as
/// clean while being about the machine it ran on. Set once for the process: every test here
/// calls this before it builds a kernel, and `OnceLock` makes the write happen exactly once
/// however many threads arrive.
fn scratch_config_home() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("ikigai-dev-conformance-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ikigai")).expect("scratch config home");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        dir
    })
    .as_path()
}

/// A small real tree for the browse family to serve: two files and a directory, so `tree`,
/// `file` and `hash` have something that exists. Deliberately NOT a git repository — the PR
/// facades are opted out, and a scratch tree that were one would make their behaviour
/// depend on whatever `gh` found.
fn fixture_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "# demo\n\nA fixture tree.\n").expect("README");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("lib.rs");
    dir
}

fn browse_settings(root: PathBuf) -> BrowseSettings {
    BrowseSettings {
        roots: vec![(ROOT.to_string(), root)],
        // Unused: the store is handed to `wire_with_store` already open.
        store: PathBuf::new(),
        file_model: "urn:llm:ollama:ask".to_string(),
        dir_model: "urn:llm:ollama:ask".to_string(),
        review_model: "urn:llm:ollama:ask".to_string(),
        pr_model: "urn:llm:ollama:ask".to_string(),
        file_max_tokens: None,
        dir_max_tokens: None,
        review_max_tokens: None,
        pr_max_tokens: None,
        review_model_label: None,
        pr_model_label: None,
        allow_models: Vec::new(),
    }
}

/// The served kernel over a scratch tree and an in-memory store.
///
/// The store is `Store::new()` rather than the RocksDB archive: opening the real one would
/// take the exclusive lock the running daemon holds, and a test that needs the machine's
/// server stopped is a test nobody runs. The registry is declared here rather than read from
/// `llm.json`, because the registry decides the SHAPE of the catalog — one
/// `urn:llm:{provider}:*` quartet per declared provider — so a walk over the machine's own
/// registry would pin whatever Brian last configured.
struct Served {
    _dir: tempfile::TempDir,
    kernel: Kernel,
}

/// The annotation seeded into every fixture store, so the overlay's read faces have a row.
///
/// A Turtle face that parses to NOTHING passes SKOLEM-RDF and VOCABULARY without seeing
/// anything (conformance PENDING #26), so an empty store would make the annotation overlay —
/// the one graph this server persists and hands to mounting clients — look checked when it
/// was not. Seeding it also fires the family's only write path, which the walk cannot reach
/// on its own (the minting IRI is not enumerated).
const SEED_ID: &str = "seed";

fn served() -> Served {
    scratch_config_home();
    let dir = fixture_root();
    let settings = browse_settings(dir.path().to_path_buf());
    let store = Arc::new(Store::new().expect("in-memory store"));
    let browse = browse::wire_with_store(&settings, store);
    let kernel = compose(Some(browse), || {
        let mut ollama = ikigai_llm::OpenAiConfig::ollama("fixture-model");
        // ⚠ `OpenAiConfig::ollama`'s default base URL is `http://localhost:11434/v1` —
        // on a developer's machine that is a LIVE Ollama, and on Brian's it is the one
        // the reading room derives against. Every endpoint that would reach it is waived
        // below, but a waiver is a decision and this is the backstop under it: port 1 on
        // loopback is refused instantly, so a narrowing mistake becomes a connection
        // error in a test rather than a request against somebody's inference server.
        // `tests/enforced.rs` does the same with a listener it can count.
        ollama.base_url = "http://127.0.0.1:1/v1".to_string();
        ollama.caps.context = Some(4096);
        ollama.caps.modalities = vec!["text".to_string()];
        ikigai_llm::Registry::single(ollama)
    });
    issue(
        &kernel,
        request(
            Verb::Sink,
            &format!("urn:iki:annotation:{SEED_ID}"),
            &[
                ("target", "urn:repo:demo:file:README.md"),
                ("body", "a seeded note"),
                ("exact", "A fixture tree."),
            ],
        ),
    )
    .expect("the annotation overlay's write path");
    Served { _dir: dir, kernel }
}

// ---------------------------------------------------------------------------
// The suite: what is fired, what is not, and why.
// ---------------------------------------------------------------------------

/// A graph small enough to read and real enough to parse — the `content` every `urn:rdf:*`
/// fixture hands over. `dcterms:title` rather than an invented predicate, because
/// VOCABULARY probes what the FACE contains and a fixture's own made-up term is reported
/// against the module that merely echoed it.
///
/// ⚠ `http://`, and NOT the `urn:` every ikigai resource is named with — reported for
/// ikigai-rdf rather than worked around silently. `ikigai-rdf`'s `sniff` tells an IRI from
/// an XML element tag by looking for `://` in the first `<…>` token, so a document whose
/// first subject is `<urn:demo:a>` is sniffed as **RDF/XML** and fails to parse
/// (`RDF parse error: Unknown prefix urn:` — an XML namespace-prefix error, from a Turtle
/// document). The one URI scheme ikigai uses for everything is the one that heuristic
/// misclassifies, so `source <any urn: graph> | urn:rdf:transrept` cannot work.
const TURTLE: &str = "<http://example.org/a> <http://purl.org/dc/terms/title> \"demo\" .\n";

/// ★ **The endpoints the walk may not FIRE, waived check by check so `ENFORCED` still
/// runs** — `(Description::id, reason)`.
///
/// Until `ikigai-conformance` 0.2.0 these were twenty-two `Suite::opt_out` calls, and
/// `opt_out` drops **every** invoking check for an id. That cost `ENFORCED` on exactly
/// the endpoints whose capability gate matters most: the subprocess seam, ten `gh`-backed
/// PR facades, and the outbound-inference family. `Suite::opt_out_check` waives one rule
/// for one endpoint, so each waiver below names the checks that RESOLVE the endpoint
/// under root — `OUTPUTS` and `CACHEABLE` everywhere, plus the two RDF checks on the four
/// entries declaring a `text/turtle` face ([`RDF_FACED`]) — and leaves `ENFORCED`, the
/// one invoking check that resolves under `Capability::scoped([])` instead.
///
/// ⚠ **That is safe only because the gate precedes the effect, and a typed `Denied` is
/// not proof of it** (ikigai-meeting #2 found an endpoint that read three secrets and
/// then refused; conformance PENDING #117). `tests/enforced.rs` is the proof: it fires
/// every gated action under no grants with a spawn log, a loopback connection counter,
/// the store's quad count and the scratch tree watching, and each witness is moved on
/// purpose first so a vacuous pass is a red test. Narrow nothing here without adding to
/// that file.
///
/// `PIPELINE` is absent by design rather than by omission: no action below declares a
/// `content` argument, so the pipeline probe returns before firing anything. (⚠ The suite
/// would not have told us — `DECLARATIONS` has no structural inertness rule for a
/// `PIPELINE` or `OUTPUTS` waiver, so a needless one prints as a real waiver. Reported.)
///
/// One id came OFF the list entirely rather than being narrowed: `llm-ollama-model`'s
/// opt-out claimed it "queries a live inference server", and it does not — ikigai-llm's
/// `ModelEndpoint::invoke` returns `config.default_model` and touches nothing. It is
/// fired in full now, and `tests/enforced.rs` pins it among the actions that declare no
/// capability at all, because `ENFORCED` resolves those for real.
const WAIVED: &[(&str, &str)] = &[
    // The exec seam. Not hypothetical: the first bare run of this file resolved
    // `urn:system:exec` under root with the suite's minimal inputs and really executed
    // `git` (`exec: `git` exited 1`). A conformance walk must not be a way to run
    // subprocesses.
    (
        "system-exec",
        "spawns a subprocess: the bare walk executed `git`",
    ),
    // git against the invoking working tree, which this test does not own. CI checkouts
    // are shallow (field guide §9), so what these read differs between machines.
    ("repo-status", "runs git in the invoking working tree"),
    ("repo-log", "runs git in the invoking working tree"),
    ("repo-branch", "runs git in the invoking working tree"),
    ("repo-list", "enumerates repositories on the machine"),
    // `gh`: network and GitHub auth. Five facades in ikigai-repo, three in the browse
    // family on top of them.
    ("repo-pr-checks", "shells out to `gh`: network and auth"),
    ("repo-pr-view", "shells out to `gh`: network and auth"),
    ("repo-pr-list", "shells out to `gh`: network and auth"),
    ("repo-pr-files", "shells out to `gh`: network and auth"),
    ("repo-pr-diff", "shells out to `gh`: network and auth"),
    ("browse-prs", "shells out to `gh`: network and auth"),
    ("browse-prs-scoped", "shells out to `gh`: network and auth"),
    ("browse-pr", "shells out to `gh`: network and auth"),
    // The derivation seam: an explain or a review POSTs to a live inference server. The
    // two PR-scoped ones do both — `gh` first, then the derivation.
    (
        "browse-explain",
        "derives through urn:llm:*: a live inference server",
    ),
    (
        "browse-review",
        "derives through urn:llm:*: a live inference server",
    ),
    (
        "browse-pr-explain",
        "shells out to `gh`, then derives through urn:llm:*",
    ),
    (
        "browse-pr-review",
        "shells out to `gh`, then derives through urn:llm:*",
    ),
    ("llm-ask", "POSTs to a live inference server"),
    ("llm-ollama-ask", "POSTs to a live inference server"),
    ("llm-ollama-up", "probes a live inference server"),
    ("llm-ollama-installed", "queries a live inference server"),
];

/// The [`WAIVED`] ids that declare a `text/turtle` face, and so need the two RDF checks
/// waived as well — every other one declares only `text/plain` / `application/json`, and
/// `SKOLEM-RDF` / `VOCABULARY` probe declared RDF outputs only. A waiver for a check that
/// could not have run is itself a `DECLARATIONS` finding in 0.2.0, so this list is not
/// tidiness: it is what keeps the waivers honest.
const RDF_FACED: &[&str] = &[
    "browse-explain",
    "browse-review",
    "browse-pr-explain",
    "browse-pr-review",
];

fn suite() -> Suite {
    let mut suite = Suite::new()
        // ---- fired, with inputs that work -------------------------------------------
        .fixture(
            Fixture::new("rdf-union", Verb::Source)
                .arg("content", TURTLE)
                .arg("with", TURTLE),
        )
        .fixture(
            Fixture::new("rdf-diff", Verb::Source)
                .arg("content", TURTLE)
                .arg("with", TURTLE)
                .arg("mode", "added"),
        )
        .fixture(
            Fixture::new("rdf-transrept", Verb::Source)
                .arg("content", TURTLE)
                .arg("as", "application/n-triples"),
        )
        .fixture(
            Fixture::new("sparql-select", Verb::Source)
                .arg("query", "SELECT * WHERE { ?s ?p ?o } LIMIT 1"),
        )
        .fixture(Fixture::new("sparql-ask", Verb::Source).arg("query", "ASK { ?s ?p ?o }"))
        .fixture(
            Fixture::new("sparql-describe", Verb::Source).arg("query", "DESCRIBE <urn:demo:a>"),
        )
        .fixture(
            Fixture::new("sparql-construct", Verb::Source)
                .arg("query", "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"),
        )
        // Bindings are per ENTRY and the fixture's verb is ignored (PENDING #2): one
        // binding per template variable is all the walk reads.
        .fixture(Fixture::new("browse-tree", Verb::Source).binding("path", "src"))
        .fixture(Fixture::new("browse-file", Verb::Source).binding("path", "README.md"))
        .fixture(Fixture::new("browse-hash", Verb::Source).binding("path", "README.md"))
        .fixture(Fixture::new("browse-explain-versions", Verb::Source).binding("path", "README.md"))
        .fixture(Fixture::new("browse-annotations", Verb::Source).binding("path", "README.md"))
        // `needs` has no `one_of`, so the suite's sample (`x`) is an unknown requirement;
        // the fixture registry declares the `text` modality.
        .fixture(Fixture::new("llm-select", Verb::Source).arg("needs", "text"))
        // The seeded annotation, so the overlay's `oa:Annotation` face is probed over a row
        // rather than over an empty graph.
        .fixture(Fixture::new("annotation", Verb::Source).binding("id", SEED_ID))
        // ---- declared ----------------------------------------------------------------
        // `urn:rdf:transrept` is a pure function of `content` and `as` — its own source
        // says so ("Transreption is a pure function of its input bytes"), and the kernel
        // folds in a piped input's expiry rather than the endpoint asserting one. So its
        // empty thread set is by design, not a representation cached with nothing to cut.
        // The other three empty-thread findings are NOT of that kind and are left
        // standing: `llm-config`, `llm-models` and `llm-select` read the provider registry
        // (`llm.json`), a file no thread names, so they really are cached for the life of
        // the process.
        .pure("rdf-transrept")
        // The W3C Web Annotation Data Model, which the browse overlay's `oa:Annotation`
        // face speaks. Registered even though this crate defines nothing under it — and
        // that is a deviation from `Suite::namespace`'s documented use, stated rather than
        // slipped in. `oa:` is a published W3C vocabulary; the suite's well-known list
        // (rdf, rdfs, xsd, owl, dcterms, foaf, schema, prov, ical, skos, sh) simply does
        // not carry it, so eleven standard terms are reported as "invented with no
        // definition" on both endpoints that serve the overlay — 31 findings of pure
        // noise, against a report whose whole non-ARGSPECS content is four lines. Reported
        // for the conformance PENDING as
        // a well-known-list gap, not as an ikigai-browse defect.
        .namespace("http://www.w3.org/ns/oa#")
        // ---- fired only by ENFORCED, and why -----------------------------------------
        // (see `WAIVED` and `tests/enforced.rs`)
        ;
    for (id, reason) in WAIVED {
        suite = suite
            .opt_out_check(*id, Check::Outputs, *reason)
            .opt_out_check(*id, Check::Cacheable, *reason);
        if RDF_FACED.contains(id) {
            suite = suite
                .opt_out_check(*id, Check::SkolemRdf, *reason)
                .opt_out_check(*id, Check::Vocabulary, *reason);
        }
    }
    suite
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn iri(s: &str) -> Iri {
    Iri::parse(s.to_string()).unwrap_or_else(|e| panic!("`{s}` is a valid IRI: {e}"))
}

fn request(verb: Verb, target: &str, args: &[(&str, &str)]) -> Request {
    let mut request = Request::new(verb, iri(target));
    for (name, value) in args {
        request = request.with_arg(*name, ArgRef::Inline(value.as_bytes().to_vec()));
    }
    request
}

fn issue(kernel: &Kernel, request: Request) -> Result<Representation, Error> {
    futures::executor::block_on(kernel.issue(request, &Capability::root()))
}

fn text(repr: &Representation) -> String {
    String::from_utf8(repr.bytes.clone()).expect("UTF-8")
}

/// Every non-kernel pattern the served kernel binds, with the id it describes itself under.
fn walked(kernel: &Kernel) -> BTreeMap<String, String> {
    kernel
        .entries()
        .expect("an enumerable root")
        .iter()
        .filter(|e| !e.pattern.starts_with("urn:kernel:"))
        .map(|e| {
            let id = kernel
                .describe_pattern(&e.pattern)
                .unwrap_or_else(|| panic!("`{}` describes itself", e.pattern))
                .id;
            (e.pattern.clone(), id)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// **The catalog is exactly what the manifest composes** — linkage-gating, checked.
///
/// The claim "this binary links only what it serves" is a security posture, and a posture
/// that only a README states is one a dependency bump can quietly widen: a module crate that
/// grows an endpoint puts it on this socket without anything here saying so. Pinned entry by
/// entry, a widened surface is a red test, and so is a narrowed one (the browse family
/// silently not binding is the failure the store lock exists to make loud).
#[test]
fn the_served_catalog_is_exactly_what_the_manifest_composes() {
    let served = served();
    let expected: BTreeMap<String, String> = SERVED
        .iter()
        .map(|(pattern, id, _)| (pattern.to_string(), id.to_string()))
        .collect();
    assert_eq!(
        walked(&served.kernel),
        expected,
        "the served catalog changed: add the new entry to SERVED with the crate that owns it"
    );
    for (pattern, _, _) in SERVED {
        assert!(
            FAMILIES.iter().any(|f| pattern.starts_with(f)),
            "`{pattern}` is outside the URN families this server occupies: {FAMILIES:?}"
        );
    }
    // Unconfigured, browse and its derivation engine are simply absent — feature-off, not
    // an error, and the catalog must not hint at them.
    let bare = compose(None, || unreachable!("no browse, no registry read"));
    let bare_ids: BTreeSet<String> = walked(&bare).into_values().collect();
    assert!(
        !bare_ids
            .iter()
            .any(|id| id.starts_with("browse-") || id.starts_with("llm-")),
        "an unconfigured server serves the curated surface only: {bare_ids:?}"
    );
}

/// The suite, over the composition, with every finding attributed.
///
/// This crate binds nothing, so the assertion is not "clean" but "**nothing unattributed**":
/// every finding names an id in [`SERVED`], and [`the_served_catalog_is_exactly_what_the_manifest_composes`]
/// holds that table to the catalog. A local endpoint added later therefore fails one of the
/// two. The inherited count is printed rather than pinned — it drops on its own as each
/// module's own adoption lands, and pinning it would make another crate's improvement a
/// failure here.
#[test]
fn conforms() {
    let served = served();
    let report = suite().run_blocking(&served.kernel);
    eprintln!("--- ikigai-dev-server, browse configured ---\n{report}");

    let owners: BTreeMap<&str, &str> = SERVED.iter().map(|(_, id, owner)| (*id, *owner)).collect();
    let unattributed: Vec<String> = report
        .findings
        .iter()
        .filter(|f| !owners.contains_key(f.endpoint.as_str()))
        .map(|f| format!("{} {}", f.endpoint, f.check.label()))
        .collect();
    assert!(
        unattributed.is_empty(),
        "every finding belongs to a module this crate composes; these name nothing in \
         SERVED: {unattributed:?}\n{report}"
    );

    // A per-crate tally, so the report says whose findings these are rather than only how
    // many there are.
    let mut by_owner: BTreeMap<&str, usize> = BTreeMap::new();
    for finding in &report.findings {
        *by_owner
            .entry(owners[finding.endpoint.as_str()])
            .or_default() += 1;
    }
    eprintln!("inherited findings by owning crate: {by_owner:?}");
    assert!(
        !by_owner.is_empty(),
        "the walk found nothing at all, which means it walked nothing: {report}"
    );
    assert_eq!(
        report.checks.skipped().count(),
        0,
        "every check runs:\n{report}"
    );
}

/// ★ **Every entry answers `Verb::Meta` with a real contract, in the face a mount reads.**
///
/// This process is the peer on the other end of everyone else's `mount` line, and
/// `ikigai_resolve::ForwardingEndpoint::describe` gets a mounted endpoint's contract by
/// round-tripping a `Verb::Meta` request in the `application/json` face. That describe is
/// **best-effort**: on any error it returns `Description::new("remote")` and says nothing.
/// A peer built with a bare `Kernel::new` has no Meta renderer, answers every Meta with `no
/// Meta renderer configured`, and every endpoint it serves arrives at the client as one
/// anonymous, action-less row — a whole federated kernel collapsing into a catalog that
/// reads as SMALL rather than broken (conformance PENDING #133, observed in ikigai-web #14).
///
/// So the renderer in `compose` is not a formatting choice, and this is the red line under
/// it. Three things, because the JSON face is the one that degrades silently and the Turtle
/// one is the one core falls back to:
///
/// 1. every entry describes itself with its own id and at least one action;
/// 2. `Verb::Meta` with `as=application/json` returns parseable JSON naming that id — the
///    exact call `MountedRemote` makes, and the call that silently returned Turtle bytes
///    before `ikigai-vocab` 0.1.47;
/// 3. `urn:kernel:catalog` renders, and names every id in [`SERVED`].
#[test]
fn every_served_entry_answers_meta_with_a_real_contract() {
    let served = served();
    for (pattern, id, _) in SERVED {
        let description = served
            .kernel
            .describe_pattern(pattern)
            .unwrap_or_else(|| panic!("`{pattern}` describes itself"));
        assert_eq!(&description.id, id, "{pattern}");
        assert_ne!(
            description.id, "remote",
            "`{pattern}` is the anonymous fallback a renderer-less peer produces"
        );
        assert!(
            !description.action_specs().is_empty(),
            "`{pattern}` declares no action: a contract with no verbs is not one"
        );
        // The JSON face, on a concrete IRI (a template pattern is not resolvable as-is).
        if pattern.contains('{') {
            continue;
        }
        let json = issue(
            &served.kernel,
            request(Verb::Meta, pattern, &[("as", "application/json")]),
        )
        .unwrap_or_else(|e| panic!("`{pattern}` renders its JSON Meta face: {e}"));
        let body = text(&json);
        assert!(
            body.trim_start().starts_with('{'),
            "`{pattern}`: the JSON Meta face must be JSON, not the canonical Turtle a \
             missing transreptor falls back to — that fallback is parsed with `.ok()` by \
             `MountedRemote` and degrades to `Description::new(\"remote\")`:\n{body}"
        );
        assert!(body.contains(id), "`{pattern}`: {body}");
    }

    let catalog = issue(
        &served.kernel,
        request(Verb::Source, "urn:kernel:catalog", &[]),
    )
    .expect("the served kernel renders its own catalog");
    let turtle = text(&catalog);
    assert!(turtle.contains("ik:Endpoint"), "{turtle}");
    for (_, id, _) in SERVED {
        assert!(
            turtle.contains(id),
            "`{id}` is in the served catalog:\n{turtle}"
        );
    }
}

/// ★ **The blast radius of "golden threads do not cross a mount", enumerated.**
///
/// `Representation::threads` is `#[serde(skip)]`, so a representation that arrives over an
/// IPC mount is cacheable with an EMPTY thread set — served forever with nothing to cut it
/// (conformance PENDING #132; ikigai-cli PENDING §6 owns the fix, whose shape shipped as
/// `ResolvedThreaded` in ikigai-module 0.3.0). That is the CLIENT's bug, not this server's.
/// What this server can state is the list of its representations that have anything to lose,
/// and it is exactly one:
///
/// | resource | expiry | threads |
/// |---|---|---|
/// | `urn:repo:style` | `Never` | `urn:file:{config home}/a11y.toml`, `…/dev-server.a11y.toml` |
///
/// Everything else the browse family serves over a working tree is `Expiry::Always` — LIVE,
/// which is the honest spelling for a read of a tree nothing here watches (core PENDING §18:
/// thread names are host-relative, so a server must not mint a thread no host keeps).
///
/// ⚠ Two things follow that are worth saying out loud rather than leaving to the table:
///
/// * `urn:llm:config` and `urn:llm:models` are `Expiry::Never` with NO thread at all — they
///   already have the disease a mount inflicts, in-process, and the suite reports them
///   (`llm-config CACHEABLE`, `llm-models CACHEABLE`). A mount cannot make them worse.
/// * `urn:repo:style`'s threads name files **nothing in this process watches**. There is no
///   filesystem watcher here, so editing `a11y.toml` does not cut them and the style face is
///   already cached for the life of the daemon; the mount merely removes the last way a
///   client could have cut it by hand. Both halves are recorded rather than fixed — minting
///   the thread is `ikigai-a11y`'s, keeping it is a host's.
#[test]
fn only_the_style_face_carries_a_thread_a_mount_would_erase() {
    let served = served();
    let probes = [
        "urn:repo:demo:tree",
        "urn:repo:demo:tree:src",
        "urn:repo:demo:file:README.md",
        "urn:repo:demo:state",
        "urn:repo:demo:hash",
        "urn:repo:demo:hash:README.md",
        "urn:repo:demo:annotations",
        "urn:repo:demo:annotations:README.md",
        "urn:repo:demo:explain-versions",
        "urn:repo:style",
        "urn:llm:config",
        "urn:llm:models",
    ];
    let mut threaded: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut cached_without_a_thread: Vec<&str> = Vec::new();
    for target in probes {
        let repr = issue(&served.kernel, request(Verb::Source, target, &[]))
            .unwrap_or_else(|e| panic!("`{target}` resolves in the scratch composition: {e}"));
        let threads: Vec<String> = repr.threads().iter().map(|t| t.to_string()).collect();
        if !threads.is_empty() {
            threaded.insert(target, threads);
        } else if repr.expiry != Expiry::Always {
            cached_without_a_thread.push(target);
        }
    }
    assert_eq!(
        threaded.keys().copied().collect::<Vec<_>>(),
        vec!["urn:repo:style"],
        "the list of representations a mount would strip of their threads changed; \
         update the table in this test's docs and tell the hub — it is the blast radius"
    );
    let home = scratch_config_home().join("ikigai");
    assert_eq!(
        threaded["urn:repo:style"],
        vec![
            format!("urn:file:{}/a11y.toml", home.display()),
            format!("urn:file:{}/dev-server.a11y.toml", home.display()),
        ],
        "and they are the layered a11y config files, under the config home"
    );
    assert_eq!(
        cached_without_a_thread,
        vec!["urn:llm:config", "urn:llm:models"],
        "cacheable with nothing to cut, already, in-process"
    );
}

/// The bare minting IRI is bound and is NOT in the catalog — which is why the operator's
/// mount line has no trailing colon.
///
/// `urn:iki:annotation` (no id) is the annotation overlay's only WRITE path: a Sink there
/// mints a new annotation. It resolves, but `kernel.entries()` does not list it, so it is in
/// no catalog, no manifold, and no conformance walk — the write path of a family whose read
/// paths are all enumerated. A mount written `urn:iki:annotation:=` matches the slug family
/// and misses the minting IRI, and every new annotation then fails to route, silently and
/// only on the hosts that mount rather than bind. Pinned here because the failure is on
/// another machine, in another process, in a config file this repo does not contain.
#[test]
fn the_bare_annotation_minting_iri_is_bound_but_not_enumerated() {
    let served = served();
    assert!(
        !walked(&served.kernel).contains_key("urn:iki:annotation"),
        "the bare minting IRI is not an enumerated entry (it is why the mount line omits \
         the trailing colon); if it became one, say so in the README"
    );
    // Bound all the same: a Meta resolution reaches the annotation endpoint's description.
    let description = served
        .kernel
        .describe(&iri("urn:iki:annotation"))
        .expect("the bare minting IRI is bound");
    assert_eq!(description.id, "annotation");
    assert!(
        description
            .action_specs()
            .iter()
            .any(|spec| spec.verb == Verb::Sink),
        "and it is the write path"
    );
}

/// The rate ceilings this server imposes bound how wide a catalog walk can be.
///
/// `urn:repo:` is capped at 120 calls a minute and the browse family lives entirely under it,
/// so a client that enumerates and probes everything — a conformance run, a manifold sweep,
/// an agent building its tool list — spends that budget and then gets `Denied` on endpoints
/// that are perfectly well-formed. This walk stays inside it because most of the family is
/// opted out; the assertion is here so that removing an opt-out without thinking about the
/// ceiling fails as a ceiling problem rather than as a mysterious `Denied`.
#[test]
fn the_walk_stays_inside_the_rate_ceilings() {
    let (_, repo_calls, _) = LIMITS[1];
    let served = served();
    let report = suite().run_blocking(&served.kernel);
    let denied: Vec<String> = report
        .findings
        .iter()
        .filter(|f| f.detail.contains("rate limit") || f.detail.contains("RateLimit"))
        .map(|f| format!("{} {}", f.endpoint, f.check.label()))
        .collect();
    assert!(
        denied.is_empty(),
        "the walk exhausted the `urn:repo:` budget ({repo_calls}/min): {denied:?}\n{report}"
    );
}

/// A last, cheap guard on the partition: the report's own counts.
///
/// `Report.endpoints` counts distinct ids and `Report.actions` one per bound entry per verb.
/// Neither is pinned to a number (a module gaining an action is not this crate's failure),
/// but a walk that suddenly sees FEWER endpoints than [`SERVED`] has ids is the shape a
/// renderer-less peer produces (PENDING #133) and the shape a broken composition produces,
/// and both look like a small clean report rather than a failure.
#[test]
fn the_walk_sees_every_endpoint_in_the_table() {
    let served = served();
    let report: Report = suite().run_blocking(&served.kernel);
    let ids: BTreeSet<&str> = SERVED.iter().map(|(_, id, _)| *id).collect();
    assert_eq!(
        report.endpoints,
        ids.len(),
        "the walk saw {} of {} endpoints:\n{report}",
        report.endpoints,
        ids.len()
    );
}
