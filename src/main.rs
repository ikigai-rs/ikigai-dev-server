//! A standalone, linkage-gated ikigai server for development tooling.
//!
//! The Cargo.toml is the module manifest: this binary composes a *curated*
//! space — the dev seam (`urn:system:exec`, `urn:repo:*`) plus graph ops
//! (`urn:rdf:*`, `urn:sparql:*`) — and serves it over a Unix socket. It does
//! NOT link EventKit, the LLM backend, or the calendar, so their code (and any
//! flaw in it) is simply not present. The exec seam is wrapped in a
//! [`RateLimit`](ikigai_throttle) so a runaway or buggy agent cannot hammer
//! git/GitHub through the substrate.
//!
//!   ikigai-dev [socket-path]         # default: $TMPDIR/ikigai-dev.sock
//!   ikigai --connect <socket-path>   # drive it from the REPL
//!
//! Security posture: **linkage-gating** (only these modules exist) +
//! **rate-limiting** (the exec seam is capped). The socket is peercred-checked
//! by the transport (owner-only). A per-scope capability *ceiling* enforced
//! server-side is the "capability-on-the-wire" work — for now the local owner
//! is trusted; the reachable surface is bounded by what is linked and limited.

use std::sync::Arc;
use std::time::Duration;

use ikigai_core::{Fallback, Kernel, Space};
use ikigai_throttle::{Rate, RateLimit};
use ikigai_vocab::TurtleRenderer;

fn main() {
    let socket = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let dir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
            std::path::Path::new(&dir).join("ikigai-dev.sock")
        });

    // The curated composition — exactly the dev surface, nothing else.
    let curated = Fallback::new(vec![
        Arc::new(ikigai_repo::space()) as Arc<dyn Space>,
        Arc::new(ikigai_rdf::space()) as Arc<dyn Space>,
        Arc::new(ikigai_sparql::space()) as Arc<dyn Space>,
    ]);

    // Rate-limit the dev seam: subprocess spawns and repo reads are the calls a
    // runaway loop would abuse. Graph ops (rdf/sparql) are pure and local, so an
    // unmatched prefix passes through unlimited.
    let space = RateLimit::new(curated)
        .limit("urn:system:exec", Rate::new(30, Duration::from_secs(60)))
        .limit("urn:repo:", Rate::new(120, Duration::from_secs(60)));

    // A meta renderer so describe/catalog work and the engine can route named
    // args (e.g. dir=) to a remote endpoint by its self-description.
    let kernel = Kernel::with_meta_renderer(Arc::new(space), Arc::new(TurtleRenderer));

    eprintln!(
        "ikigai-dev: serving the dev seam (repo · rdf · sparql; exec ≤ 30/min) on {}",
        socket.display()
    );
    eprintln!("  connect with:  ikigai --connect {}", socket.display());
    if let Err(e) = ikigai_ipc::serve(kernel, &socket) {
        eprintln!("ikigai-dev: serve error: {e}");
        std::process::exit(1);
    }
}
