//! Browse-family wiring: configured roots + the persistent shared store →
//! [`ikigai_browse::space_with_explain`].
//!
//! Decision of record: **the dev server owns the browse store on a machine.**
//! The store below (RocksDB via oxigraph) takes an exclusive lock — one
//! process holds it, and everyone else reaches the family through this
//! server's socket with prefer-mounts:
//!
//! ```toml
//! # in the MAIN host's ~/.config/ikigai/config.toml (two lines because the
//! # family spans two URN prefixes):
//! mount = "prefer urn:repo:=~/.ikigai/dev.sock"
//! mount = "prefer urn:annotation:=~/.ikigai/dev.sock"
//! ```
//!
//! The default store path is `~/.ikigai/browse-store` — the SAME default the
//! cli's serving instance used, deliberately: the archive already derived on a
//! machine carries over when ownership moves here; the lock arbitrates any
//! overlap loudly.
//!
//! The store opens or fails LOUD — a persistent archive that silently fell
//! back to memory would "work" while quietly forgetting every explanation and
//! annotation, the worst failure shape. The bundled ikigai vocabulary is
//! loaded on every start (idempotent — same triples into the same named
//! graph), so schema joins (`?e a/rdfs:subClassOf* ik:Endpoint`) keep working
//! over the shared graph.

use std::sync::Arc;

use ikigai_core::EndpointSpace;
use ikigai_sparql::Store;

use crate::config::BrowseSettings;

/// The wired browse family: the space to mount plus the persistent store
/// handle the server shares onward (the sparql space joins the same graph).
pub struct Browse {
    pub space: EndpointSpace,
    pub store: Arc<Store>,
}

/// Open the store, load the vocabulary, and bind the family.
///
/// # Panics
///
/// Fails loud when the store cannot open (path, permissions, or another
/// process holding the exclusive lock) or the vocabulary cannot load.
pub fn wire(settings: &BrowseSettings) -> Browse {
    let store = Arc::new(Store::open(&settings.store).unwrap_or_else(|e| {
        panic!(
            "ikigai-dev: browse store `{}` cannot open: {e} — refusing to run with an \
             in-memory archive (explanations and annotations would be silently lost). \
             ONE process holds the store at a time; if another ikigai holds this lock \
             (e.g. a main-host serve instance with `serve.browse.root` lines), move \
             browse ownership HERE: drop those lines and prefer-mount this server \
             instead — mount = \"prefer urn:repo:=<this socket>\" and \
             mount = \"prefer urn:annotation:=<this socket>\". Otherwise fix the \
             path/permissions.",
            settings.store.display()
        )
    }));
    ikigai_sparql::load_vocabulary(&store).unwrap_or_else(|e| {
        panic!("ikigai-dev: loading the vocabulary into the browse store: {e:?}")
    });

    let mut explain = ikigai_browse::ExplainConfig::new(Arc::clone(&store))
        .file_provider(settings.file_model.clone())
        .dir_provider(settings.dir_model.clone());
    if let Some(tokens) = settings.file_max_tokens {
        explain = explain.file_max_tokens(tokens);
    }
    if let Some(tokens) = settings.dir_max_tokens {
        explain = explain.dir_max_tokens(tokens);
    }

    Browse {
        space: ikigai_browse::space_with_explain(settings.roots.clone(), explain),
        store,
    }
}
