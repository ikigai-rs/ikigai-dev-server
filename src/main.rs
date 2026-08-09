//! A standalone, linkage-gated ikigai server for development tooling.
//!
//! The Cargo.toml is the module manifest: this binary composes a *curated*
//! space — the dev seam (`urn:system:exec`, `urn:repo:*`) plus graph ops
//! (`urn:rdf:*`, `urn:sparql:*`), and, when configured, the **browse family**
//! (`urn:repo:{repo}:tree/file/state/hash/explain/…` + `urn:annotation:*`)
//! with its persistent explanation/annotation store — and serves it over a
//! Unix socket. It does NOT link EventKit or the calendar, so their code (and
//! any flaw in it) is simply not present.
//!
//!   ikigai-dev [socket] [flags]      # default: ~/.ikigai/dev.sock
//!   ikigai --connect <socket>        # drive it from the REPL
//!   ikigai-dev --help                # the config-home grammar (~/.config/ikigai/dev.toml)
//!
//! Decision of record: **this server owns the browse store on a machine** —
//! the store takes an exclusive lock, so one process serves the family and
//! every other process prefer-mounts this socket (`mount = "prefer
//! urn:repo:=~/.ikigai/dev.sock"` + the `urn:annotation:` twin). That is why
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
//! See the [`llm`] module doc for the full argument (including the rejected
//! mount-from-the-main-host alternative) and the registry rules. The space is
//! mounted only when browse is configured; unconfigured, the server is
//! exactly the original curated surface.

mod browse;
mod config;
mod llm;

use std::sync::Arc;
use std::time::Duration;

use ikigai_core::{Fallback, Kernel, Space};
use ikigai_throttle::{Rate, RateLimit};
use ikigai_vocab::TurtleRenderer;

fn main() {
    let settings = config::settings();

    // The browse family first (its store handle decides which sparql space
    // binds below), or None: absence of config is feature-off, not an error.
    let browse = settings.browse.as_ref().map(browse::wire);

    // urn:sparql:*. Two regimes, decided by configuration (the cli host's
    // convention, verbatim):
    // - browse unconfigured: `space()` — private per-query store, vocab
    //   pre-seeded, `graph=` federates per query, results cacheable.
    // - browse configured: `space_with_store` over the SAME `Arc<Store>` the
    //   explanation archive and annotations write — one shared graph, so
    //   `urn:sparql:select` joins ik:Explanation + oa:Annotation rows live.
    let sparql_space: Arc<dyn Space> = match &browse {
        Some(b) => Arc::new(ikigai_sparql::space_with_store(Arc::clone(&b.store))),
        None => Arc::new(ikigai_sparql::space()),
    };

    // The curated composition — exactly the dev surface, nothing else.
    let mut spaces: Vec<Arc<dyn Space>> = vec![
        Arc::new(ikigai_repo::space()) as Arc<dyn Space>,
        Arc::new(ikigai_rdf::space()) as Arc<dyn Space>,
        sparql_space,
    ];
    if let Some(b) = browse {
        // The browse grammar only matches configured root names (reserved
        // names refused at startup), so it composes with ikigai-repo's
        // urn:repo:* Exacts without shadowing.
        spaces.push(Arc::new(b.space) as Arc<dyn Space>);
        // The explain seam's derivation engine — mounted only alongside
        // browse; see the llm module doc for the posture argument.
        spaces.push(Arc::new(llm::space()) as Arc<dyn Space>);
    }
    let browse_on = spaces.len() > 3;
    let curated = Fallback::new(spaces);

    // Rate-limit the expensive seams: subprocess spawns, repo reads, and LLM
    // derivations are the calls a runaway loop would abuse. The llm limit
    // governs explain's internal asks too — subrequests resolve back through
    // this overlay. Graph ops (rdf/sparql) are pure and local, so an
    // unmatched prefix passes through unlimited.
    let space = RateLimit::new(curated)
        .limit("urn:system:exec", Rate::new(30, Duration::from_secs(60)))
        .limit("urn:repo:", Rate::new(120, Duration::from_secs(60)))
        .limit("urn:llm:", Rate::new(30, Duration::from_secs(60)));

    // A meta renderer so describe/catalog work and the engine can route named
    // args (e.g. dir=) to a remote endpoint by its self-description.
    let kernel = Kernel::with_meta_renderer(Arc::new(space), Arc::new(TurtleRenderer));

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
