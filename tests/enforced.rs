//! **The declared capability gate precedes the effect — witnessed, not assumed.**
//!
//! `tests/conformance.rs` used to waive every invoking check on twenty-two endpoints
//! with `Suite::opt_out`, because a conformance walk must not spawn `git`, shell out to
//! `gh`, or POST to a live inference server. `ikigai-conformance` 0.2.0 added
//! `Suite::opt_out_check`, so those waivers can now name the checks that FIRE the
//! endpoint and leave `ENFORCED` running — the one invoking check that resolves under
//! `Capability::scoped([])` rather than under root.
//!
//! That narrowing is only safe if the gate really does precede the effect, and a typed
//! `Denied` is **not** proof of it: ikigai-meeting #2 found an endpoint that read three
//! secrets and *then* refused (conformance PENDING #117). So this file is the proof,
//! and it is a separate test binary on purpose — it puts shims for `git` and `gh` on
//! `PATH` and points the LLM registry at a listener it owns, and neither may leak into
//! the process `tests/conformance.rs` runs in.
//!
//! Four witnesses, one per effect class this composition can have:
//!
//! | witness | what it sees | the endpoints it covers |
//! |---|---|---|
//! | a spawn log | every `git` / `gh` the process executes, with its argv | `system-exec`, the four `repo-*` git readers, the five `repo-pr-*` and five browse PR facades |
//! | a loopback accept counter | every TCP connection the HTTP transport opens | `llm-ask`, `llm-ollama-{ask,up,installed}`, `browse-explain`, `browse-review`, `browse-pr-{explain,review}` |
//! | the store's quad count | every triple the annotation overlay writes | `annotation`'s Sink and Delete, `browse-review`, `browse-pr-review` |
//! | the scratch tree | any file created, removed or rewritten | everything (nothing here is supposed to write to a working tree at all) |
//!
//! Each witness is proved live before it is trusted ([`the_witnesses_see_the_effects_they_are_watching_for`]):
//! under a capability that grants the scope, the effect is observed. Then every action in
//! the catalog that declares a `requires` is fired under a capability holding **no**
//! grants and asserted to be refused with `Error::Denied` *and* to have moved no witness
//! ([`no_declared_gate_is_crossed_before_it_is_checked`]). That second call is
//! byte-for-byte what the suite's `ENFORCED` check makes, so witnessing it here is
//! witnessing the narrowed walk.
//!
//! ⚠ The complement is the interesting half, and it has its own test
//! ([`the_ungated_actions_are_pinned_because_enforced_really_fires_them`]): an action
//! declaring NO `requires` is not stopped by anything, so `ENFORCED` resolves it for
//! real. Narrowing a waiver on one of those would make the walk do the very thing the
//! waiver existed to prevent. The list is pinned, so a module that grows an ungated
//! action makes this test red rather than making the next walk act.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ikigai_core::{
    ArgRef, Bindings, Capability, Error, Iri, Kernel, Representation, Request, UriTemplate, Verb,
};
use ikigai_dev_server::{browse, compose, config::BrowseSettings};
use ikigai_sparql::Store;

/// The browse root name, matching `tests/conformance.rs`.
const ROOT: &str = "demo";

/// Bindings for the template patterns, so every entry can be fired at a concrete IRI.
/// `path` and `n` are the only template variables this catalog has.
const BINDINGS: &[(&str, &str)] = &[("path", "README.md"), ("n", "1"), ("id", "seed")];

// ---------------------------------------------------------------------------
// The witnesses
// ---------------------------------------------------------------------------

/// Where the `git` / `gh` shims append one line per invocation.
///
/// Installed once for this test binary: a scratch directory holding two executables
/// named exactly what the modules spawn, prepended to `PATH`. They log and exit without
/// running anything — this binary never wants a real `git`, and a real `gh` would reach
/// the network, which is the whole point.
fn spawn_log() -> &'static Path {
    static LOG: OnceLock<PathBuf> = OnceLock::new();
    LOG.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("ikigai-dev-enforced-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).expect("shim bin dir");
        let log = dir.join("spawns.log");
        for tool in ["git", "gh"] {
            let path = bin.join(tool);
            // One line per invocation, whatever the argv contains: `gh pr list` passes a
            // `--template` with embedded newlines, and a witness that logged those raw
            // would report one spawn as several lines.
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\na=$(printf '%s ' \"$@\" | tr '\\n' ' ')\n\
                     printf '{tool} %s\\n' \"$a\" >> '{}'\nexit 0\n",
                    log.display()
                ),
            )
            .expect("shim");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .expect("shim is executable");
            }
        }
        let existing = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{existing}", bin.display()));
        // A scratch config home too: `urn:repo:style` layers `a11y.toml` out of it, and a
        // test that reads the developer's real home is about the machine it ran on.
        let config = dir.join("config");
        std::fs::create_dir_all(config.join("ikigai")).expect("scratch config home");
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::fs::write(&log, "").expect("empty log");
        log
    })
    .as_path()
}

/// Every line the shims have written so far.
fn spawns() -> Vec<String> {
    std::fs::read_to_string(spawn_log())
        .expect("the spawn log")
        .lines()
        .map(str::to_string)
        .collect()
}

/// A loopback listener that counts connections and answers nothing useful.
///
/// The fixture registry's `base_url` points here, so **no test in this binary can reach
/// the machine's real inference server** — the default `OpenAiConfig::ollama` base URL is
/// `http://localhost:11434/v1`, which on a developer's machine is a live Ollama. The
/// count is the network witness.
struct Listener {
    base_url: String,
    connections: Arc<AtomicUsize>,
}

fn listener() -> &'static Listener {
    static LISTENER: OnceLock<Listener> = OnceLock::new();
    LISTENER.get_or_init(|| {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = socket.local_addr().expect("the bound address");
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connections);
        std::thread::spawn(move || {
            for stream in socket.incoming() {
                counter.fetch_add(1, Ordering::SeqCst);
                // Read what arrived and answer a bare 503, so the client sees a
                // reachable-but-useless server rather than hanging.
                if let Ok(mut stream) = stream {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = std::io::Write::write_all(
                        &mut stream,
                        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
                    );
                }
            }
        });
        Listener {
            base_url: format!("http://127.0.0.1:{}/v1", addr.port()),
            connections,
        }
    })
}

/// Everything observable about the process and its fixtures at one instant.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Witness {
    spawns: Vec<String>,
    connections: usize,
    quads: usize,
    tree: Vec<(String, u64, std::time::SystemTime)>,
}

fn snapshot(served: &Served) -> Witness {
    Witness {
        spawns: spawns(),
        connections: listener().connections.load(Ordering::SeqCst),
        quads: served.store.len().expect("the store's quad count"),
        tree: tree_state(served.dir.path()),
    }
}

/// Path, size and mtime of every file under `root`, sorted — a rewrite in place is a
/// change in size or mtime, and a create or delete is a change in the set.
fn tree_state(root: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, u64, std::time::SystemTime)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let meta = entry.metadata().expect("metadata");
            if meta.is_dir() {
                walk(&path, root, out);
            } else {
                out.push((
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                    meta.len(),
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// The composition under test — `compose`, the same function `main` calls
// ---------------------------------------------------------------------------

struct Served {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    kernel: Kernel,
}

fn served() -> Served {
    spawn_log();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "# demo\n\nA fixture tree.\n").expect("README");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("lib.rs");
    let settings = BrowseSettings {
        roots: vec![(ROOT.to_string(), dir.path().to_path_buf())],
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
    };
    let store = Arc::new(Store::new().expect("in-memory store"));
    let browse = browse::wire_with_store(&settings, Arc::clone(&store));
    let kernel = compose(Some(browse), || {
        let mut ollama = ikigai_llm::OpenAiConfig::ollama("fixture-model");
        ollama.base_url = listener().base_url.clone();
        ollama.caps.context = Some(4096);
        ollama.caps.modalities = vec!["text".to_string()];
        ikigai_llm::Registry::single(ollama)
    });
    Served { dir, store, kernel }
}

/// The witnesses are process-global (`PATH`, one listener), so the tests that read them
/// take turns. Three tests, so a mutex is the whole mechanism.
fn serialized() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
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

fn issue(
    kernel: &Kernel,
    request: Request,
    capability: &Capability,
) -> Result<Representation, Error> {
    futures::executor::block_on(kernel.issue(request, capability))
}

/// The concrete IRI to fire a pattern at: itself when it is one, else expanded with
/// [`BINDINGS`].
fn concrete(pattern: &str) -> String {
    if Iri::parse(pattern).is_ok() {
        return pattern.to_string();
    }
    let template = UriTemplate::parse(pattern).expect("a pattern is an IRI or a URI template");
    let mut bindings = Bindings::new();
    for (name, value) in BINDINGS {
        bindings.insert(*name, *value);
    }
    template
        .expand(&bindings)
        .unwrap_or_else(|| panic!("`{pattern}` has a variable BINDINGS does not name"))
}

/// Every `(pattern, id, verb, requires)` this kernel binds outside `urn:kernel:`.
fn actions(kernel: &Kernel) -> Vec<(String, String, Verb, Vec<String>)> {
    kernel
        .entries()
        .expect("an enumerable root")
        .iter()
        .filter(|e| !e.pattern.starts_with("urn:kernel:"))
        .flat_map(|e| {
            let description = kernel
                .describe_pattern(&e.pattern)
                .unwrap_or_else(|| panic!("`{}` describes itself", e.pattern));
            let id = description.id.clone();
            description
                .action_specs()
                .into_iter()
                .map(move |spec| (e.pattern.clone(), id.clone(), spec.verb, spec.requires))
                .collect::<Vec<_>>()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// ★ **The witnesses are live.** A witness that cannot see the effect it watches for
/// makes every "nothing happened" below vacuous — the same failure shape as a
/// conformance walk that resolves nothing and reports clean. So each one is moved on
/// purpose first, through the same kernel, under a capability that grants the scope.
#[test]
fn the_witnesses_see_the_effects_they_are_watching_for() {
    let _serial = serialized();
    let served = served();

    // A subprocess, through `urn:system:exec`.
    let before = snapshot(&served);
    let exec = Capability::scoped(["urn:cap:exec:git"]);
    issue(
        &served.kernel,
        request(
            Verb::Source,
            "urn:system:exec",
            &[("tool", "git"), ("args", "--version")],
        ),
        &exec,
    )
    .expect("the shim exits 0");
    let after = snapshot(&served);
    assert_eq!(
        after.spawns.len(),
        before.spawns.len() + 1,
        "the spawn witness saw nothing: {after:?}"
    );
    assert!(after.spawns.last().expect("a line").starts_with("git "));

    // A subprocess through a `gh` facade, which is a different code path (ikigai-repo
    // builds the argv rather than taking it from the caller).
    let before = snapshot(&served);
    let gh = Capability::scoped(["urn:cap:exec:gh"]);
    let _ = issue(
        &served.kernel,
        request(
            Verb::Source,
            "urn:repo:pr:list",
            &[("repo", "ikigai-rs/ikigai-dev-server")],
        ),
        &gh,
    );
    let after = snapshot(&served);
    assert!(
        after.spawns.len() > before.spawns.len()
            && after.spawns.iter().any(|line| line.starts_with("gh ")),
        "the `gh` facade did not reach the shim: {after:?}"
    );

    // An outbound connection, through the LLM liveness probe.
    let before = snapshot(&served);
    let net = Capability::scoped(["urn:cap:net:127.0.0.1"]);
    issue(
        &served.kernel,
        request(Verb::Source, "urn:llm:ollama:up", &[]),
        &net,
    )
    .expect("liveness is a boolean, not an error");
    let after = snapshot(&served);
    assert_eq!(
        after.connections,
        before.connections + 1,
        "the connection witness saw nothing"
    );

    // A write into the annotation store, through the overlay's minting IRI.
    let before = snapshot(&served);
    let annotate = Capability::scoped(["urn:cap:annotate", "urn:cap:browse:read:*"]);
    issue(
        &served.kernel,
        request(
            Verb::Sink,
            "urn:iki:annotation",
            &[
                ("target", "urn:repo:demo:file:README.md"),
                ("body", "a witness note"),
                ("exact", "A fixture tree."),
            ],
        ),
        &annotate,
    )
    .expect("the annotation overlay's write path");
    let after = snapshot(&served);
    assert!(
        after.quads > before.quads,
        "the store witness saw nothing: {} quad(s) before, {} after",
        before.quads,
        after.quads
    );
}

/// ★ **Every declared gate refuses before anything happens.**
///
/// For each action in the catalog that declares a `requires`, the same call the suite's
/// `ENFORCED` check makes — the concrete IRI, `Capability::scoped([])` — must come back
/// `Err(Error::Denied)` with every witness unmoved. `ikigai-core`'s kernel checks the
/// declared floor **before dispatch** (`kernel.rs`: "enforcement happens before dispatch,
/// so the endpoint is never entered"), and `ikigai-repo` checks its own `urn:cap:exec:{tool}`
/// again before `Command::new`; this test is what turns those two readings into a fact
/// about the composed kernel, overlay included.
///
/// It is the licence for `tests/conformance.rs` to waive `OUTPUTS`, `CACHEABLE` and the
/// RDF checks per endpoint and leave `ENFORCED` running.
#[test]
fn no_declared_gate_is_crossed_before_it_is_checked() {
    let _serial = serialized();
    let served = served();
    let none = Capability::scoped(Vec::<String>::new());
    let before = snapshot(&served);
    let mut gated = 0usize;
    for (pattern, id, verb, requires) in actions(&served.kernel) {
        if requires.is_empty() {
            continue;
        }
        gated += 1;
        let target = concrete(&pattern);
        let result = issue(&served.kernel, request(verb, &target, &[]), &none);
        match result {
            Err(Error::Denied(_)) => {}
            other => panic!(
                "`{id}` ({target}, {verb:?}) declares requires {requires:?} but under no grants \
                 returned {other:?}: declared is not enforced"
            ),
        }
        let after = snapshot(&served);
        assert_eq!(
            after, before,
            "`{id}` ({target}, {verb:?}) refused, but something happened first — a typed \
             `Denied` is not proof that nothing ran (conformance PENDING #117). Restore the \
             whole-endpoint `Suite::opt_out` in tests/conformance.rs and report the endpoint."
        );
    }
    assert!(
        gated >= 20,
        "only {gated} gated action(s): the walk is not covering the composition"
    );
}

/// ⚠ **The ungated actions, pinned — because `ENFORCED` really fires those.**
///
/// `ENFORCED`'s other half asserts that an action declaring NO `requires` is *not*
/// refused, and it learns that by resolving it under `Capability::scoped([])`. Nothing
/// stops that call, so for an ungated action `ENFORCED` is a live invocation — which is
/// why a whole-endpoint `Suite::opt_out` is still the only safe waiver for one with real
/// side effects, however clearly it refuses.
///
/// The list below is therefore a safety pin, not a tally: every id on it is one a
/// `tests/conformance.rs` waiver may NOT be narrowed to `ENFORCED`, and a module that
/// grows an ungated action lands here as a red test rather than as a walk that acts.
///
/// Firing every one of them is harmless, in two groups:
///
/// * the `urn:rdf:*` and `urn:sparql:*` graph ops are pure, local and in-memory — the
///   composition's own comment says so, and it is why the rate overlay leaves them
///   unlimited. The suite already fires them all with fixtures.
/// * the four `urn:llm:` entries are local reads of the provider registry the host loaded
///   at start-up (ikigai-llm's own note groups `:config`, `:models` and `:model` as one
///   shared fact). `llm-ollama-model` in particular returns `config.default_model` and
///   touches no network at all — which is why its `tests/conformance.rs` opt-out, whose
///   stated reason was "queries a live inference server", came off entirely rather than
///   being narrowed. The reason was simply wrong.
#[test]
fn the_ungated_actions_are_pinned_because_enforced_really_fires_them() {
    let _serial = serialized();
    let served = served();
    let ungated: BTreeSet<String> = actions(&served.kernel)
        .into_iter()
        .filter(|(_, _, _, requires)| requires.is_empty())
        .map(|(_, id, verb, _)| format!("{id} {verb:?}"))
        .collect();
    assert_eq!(
        ungated,
        [
            "llm-config Source",
            "llm-models Source",
            "llm-ollama-model Source",
            "llm-select Source",
            "rdf-diff Source",
            "rdf-transrept Source",
            "rdf-union Source",
            "sparql-ask Source",
            "sparql-construct Source",
            "sparql-describe Source",
            "sparql-select Source",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>(),
        "the set of actions the kernel gates on nothing changed. ENFORCED resolves these \
         for real, so anything with a side effect that appears here needs a whole-endpoint \
         `Suite::opt_out` in tests/conformance.rs — and an undeclared capability an \
         endpoint enforces at runtime makes the manifold over-offer, which is a finding \
         about the owning module."
    );
}
