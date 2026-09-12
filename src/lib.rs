//! The composition this binary serves, as a library function.
//!
//! [`compose`] builds the exact kernel `ikigai-dev` puts behind its socket:
//! the curated space (the dev seam + graph ops + — when configured — the
//! browse family and its derivation engine), under the rate-limit overlay,
//! behind a Meta renderer. `main` calls it; so does `tests/conformance.rs`.
//!
//! **That sharing is the point, not a convenience.** This crate binds no
//! endpoints of its own — everything it serves comes from a module crate with
//! its own test suite — so the only property that is THIS crate's to get wrong
//! is the composition: which spaces, in which order, under which overlay,
//! behind which renderer. A test that rebuilds that by hand would pass while
//! the served kernel was broken. `ikigai-web` shipped exactly that defect: its
//! HTTP tests built their kernels directly, the one function with the bug was
//! never under test, and the daemon answered 500 on a link it emitted itself.
//!
//! The two seams a test needs are threaded through as parameters rather than
//! read from the machine, so a walk over this kernel touches no home directory:
//! the browse store (an in-memory [`ikigai_sparql::Store`] instead of the
//! RocksDB archive — see [`browse::wire_with_store`]) and the LLM registry
//! (lazily, so an unconfigured server still never reads `llm.json`).
//!
//! ## ⚠ This library is INTERNAL: no stability promise
//!
//! The crate publishes a binary (`ikigai-dev`). The library exists so the tests
//! can hold the real composition rather than a re-creation of it, and every item
//! it exports — [`compose`], [`LIMITS`], [`browse::wire_with_store`],
//! [`llm::space_with`], [`config`] — is here for that reason and no other. Treat
//! the whole surface as private to this repo: it changes with the composition,
//! without a MINOR bump and without a deprecation. **What this crate versions is
//! the set of RESOURCE NAMES behind the socket** (0.3.0 was a MINOR because
//! `urn:annotation:` became `urn:iki:annotation:`, with the Rust API untouched) —
//! a mount line is the public interface here, not a `use`. Nothing outside this
//! repo depends on the library today, and nothing should start without moving
//! the item it wants into a module crate with a suite of its own.
//! (This repo's PENDING §4 asked for the decision; this is it.)

pub mod browse;
pub mod config;
pub mod llm;

use std::sync::Arc;
use std::time::Duration;

use ikigai_core::{Fallback, Kernel, Space};
use ikigai_throttle::{Rate, RateLimit};
use ikigai_vocab::TurtleRenderer;

pub use browse::Browse;

/// The rate ceilings this server imposes, as `(prefix, calls, per)`.
///
/// Public because they are a served fact: a client that walks this catalog
/// exhaustively (a conformance run, a manifold sweep, an agent enumerating
/// tools) spends the `urn:repo:` budget, and a walk wide enough to exhaust it
/// gets `Denied` on endpoints that are perfectly well-formed. Naming them here
/// lets a test say how much room it has instead of discovering the ceiling as
/// a flake.
pub const LIMITS: [(&str, u32, Duration); 3] = [
    ("urn:system:exec", 30, Duration::from_secs(60)),
    ("urn:repo:", 120, Duration::from_secs(60)),
    ("urn:llm:", 30, Duration::from_secs(60)),
];

/// The served kernel.
///
/// `browse` is the wired browse family or `None` (absence of config is
/// feature-off, not an error). `registry` is called **only** when browse is
/// configured — the LLM space is mounted alongside browse and nowhere else, so
/// an unconfigured server must not even read the registry file.
pub fn compose<R>(browse: Option<Browse>, registry: R) -> Kernel
where
    R: FnOnce() -> ikigai_llm::Registry,
{
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
        spaces.push(Arc::new(llm::space_with(registry())) as Arc<dyn Space>);
    }
    let curated = Fallback::new(spaces);

    // Rate-limit the expensive seams: subprocess spawns, repo reads, and LLM
    // derivations are the calls a runaway loop would abuse. The llm limit
    // governs explain's internal asks too — subrequests resolve back through
    // this overlay. Graph ops (rdf/sparql) are pure and local, so an
    // unmatched prefix passes through unlimited.
    let mut space = RateLimit::new(curated);
    for (prefix, calls, per) in LIMITS {
        space = space.limit(prefix, Rate::new(calls, per));
    }

    // A meta renderer so describe/catalog work and the engine can route named
    // args (e.g. dir=) to a remote endpoint by its self-description.
    //
    // ⚠ Not a formatting choice, and not optional. `ikigai-resolve`'s
    // `ForwardingEndpoint::describe` round-trips a `Verb::Meta` request to get
    // a mounted endpoint's contract, and that describe is BEST-EFFORT: on any
    // error it falls back to `Description::new("remote")` and says nothing. A
    // peer built with a bare `Kernel::new` answers every Meta with `no Meta
    // renderer configured`, so every endpoint it serves arrives at the client
    // as one anonymous, action-less row — a whole federated kernel collapsing
    // to a catalog that reads as SMALL rather than broken. This process is the
    // peer on the other end of everyone else's `mount` line; dropping this
    // renderer would degrade THEIR catalogs, silently.
    // `tests/conformance.rs::every_served_entry_answers_meta_with_a_real_contract`
    // is the red line under that.
    Kernel::with_meta_renderer(Arc::new(space), Arc::new(TurtleRenderer))
}
