//! A standalone, linkage-gated ikigai server for development tooling.
//!
//! The Cargo.toml is the module manifest: this binary composes a *curated*
//! space — the dev seam (`urn:system:exec`, `urn:repo:*`) plus graph ops
//! (`urn:rdf:*`, `urn:sparql:*`), and, when configured, the **browse family**
//! (`urn:repo:{repo}:tree/file/state/hash/explain/…` + `urn:iki:annotation:*`)
//! with its persistent explanation/annotation store — and serves it over a
//! Unix socket. It does NOT link EventKit or the calendar, so their code (and
//! any flaw in it) is simply not present.
//!
//! ```text
//! ikigai-dev [socket] [flags]      # default: ~/.ikigai/dev.sock
//! ikigai --connect <socket>        # drive it from the REPL
//! ikigai-dev --help                # the config-home grammar (~/.config/ikigai/dev.toml)
//! ```
//!
//! The composition itself lives in [`ikigai_dev_server::compose`] so that a
//! test walks the kernel this `main` serves rather than a re-creation of it;
//! this file is the process — flags, the socket's parent directory, the
//! banner, and `serve`.
//!
//! Decision of record: **this server owns the browse store on a machine** —
//! the store takes an exclusive lock, so one process serves the family and
//! every other process prefer-mounts this socket (`mount = "prefer
//! urn:repo:=~/.ikigai/dev.sock"` + the `urn:iki:annotation` twin — no
//! trailing colon, so it also covers the bare minting IRI). That is why
//! the default socket moved from `$TMPDIR` (which churns across reboots) to
//! the stable `~/.ikigai/dev.sock` mounts can name.
//!
//! Security posture: **linkage-gating** (only these modules exist) +
//! **rate-limiting** (the exec seam is capped, and so is `urn:llm:` — see
//! below). The socket is peercred-checked by the transport (owner-only). A
//! per-scope capability *ceiling* enforced server-side is the
//! "capability-on-the-wire" work — for now the local owner is trusted; the
//! reachable surface is bounded by what is linked and limited.
//!
//! On the LLM exception to the curated posture: browse's explain derivation
//! needs `urn:llm:*`, and `ikigai-llm` is an outbound HTTP client to local
//! inference — not the EventKit/TCC class linkage-gating exists to exclude.
//! See the [`ikigai_dev_server::llm`] module doc for the full argument
//! (including the rejected mount-from-the-main-host alternative) and the
//! registry rules. The space is mounted only when browse is configured;
//! unconfigured, the server is exactly the original curated surface.

use ikigai_dev_server::{browse, compose, config, llm};

fn main() {
    let settings = config::settings();

    // The browse family first (its store handle decides which sparql space
    // binds below), or None: absence of config is feature-off, not an error.
    let browse = settings.browse.as_ref().map(browse::wire);
    let browse_on = browse.is_some();

    // `llm::registry` is passed unevaluated: it is read only when browse is
    // configured, so an unconfigured server never touches `llm.json` (and a
    // malformed one is not its problem).
    let kernel = compose(browse, llm::registry);

    // The stable default lives under ~/.ikigai, which may not exist yet.
    if let Some(parent) = settings.socket.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("ikigai-dev: creating {}: {e}", parent.display());
            std::process::exit(1);
        }
    }

    eprintln!(
        "ikigai-dev: serving the dev seam (repo · rdf · sparql{}; exec ≤ 30/min) on {}",
        if browse_on {
            " · browse · llm ≤ 30/min"
        } else {
            ""
        },
        settings.socket.display()
    );
    eprintln!(
        "  connect with:  ikigai --connect {}",
        settings.socket.display()
    );
    if let Err(e) = ikigai_ipc::serve(kernel, &settings.socket) {
        eprintln!("ikigai-dev: serve error: {e}");
        std::process::exit(1);
    }
}
