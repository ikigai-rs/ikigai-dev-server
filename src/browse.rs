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

    let explain = explain_config(settings, &store);

    Browse {
        // `.app("dev-server")` is what makes the a11y layering mean anything here:
        // without it the mount reads only the machine-wide `a11y.toml`, and a
        // `dev-server.a11y.toml` override would sit on disk doing nothing — the
        // quietest kind of wrong, since the file exists and is simply never
        // consulted. The name is this PROCESS's identity, which is why the
        // library takes it at mount time rather than per request: a caller does
        // not get to choose whose accessibility settings apply.
        space: ikigai_browse::Mount::new(settings.roots.clone())
            .explain(explain)
            .app("dev-server")
            .space(),
        store,
    }
}

/// The explain seam's configuration: the four provider tiers, their ceilings
/// and labels, and the operator's selectable set.
///
/// Split out from [`wire`] so it can be built over any store — the wiring is
/// what the tests need to see, and opening the real RocksDB archive to read
/// back an ArgSpec would be a lot of lock for one assertion.
fn explain_config(settings: &BrowseSettings, store: &Arc<Store>) -> ikigai_browse::ExplainConfig {
    let mut explain = ikigai_browse::ExplainConfig::new(Arc::clone(store))
        .file_provider(settings.file_model.clone())
        .dir_provider(settings.dir_model.clone())
        .review_provider(settings.review_model.clone())
        .pr_provider(settings.pr_model.clone())
        // The operator's answer to "which backends may a `provider=` request
        // name?". Browse always keeps the two explain tiers selectable — those
        // grant no reach a caller did not already have, since a plain explain
        // already asks them — so this call is the whole of the widening, and
        // an empty list leaves the menu offering exactly those two. It is the
        // reason the menu is worth having at all: without it every option row
        // but the tiers' own would be a click that comes back `Denied`.
        .allow_providers(settings.allow_models.iter().cloned());
    if let Some(tokens) = settings.file_max_tokens {
        explain = explain.file_max_tokens(tokens);
    }
    if let Some(tokens) = settings.dir_max_tokens {
        explain = explain.dir_max_tokens(tokens);
    }
    if let Some(tokens) = settings.review_max_tokens {
        explain = explain.review_max_tokens(tokens);
    }
    if let Some(tokens) = settings.pr_max_tokens {
        explain = explain.pr_max_tokens(tokens);
    }
    if let Some(label) = &settings.review_model_label {
        explain = explain.review_model_label(label.clone());
    }
    if let Some(label) = &settings.pr_model_label {
        explain = explain.pr_model_label(label.clone());
    }
    explain
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use ikigai_core::{Iri, Request, Resolution, Scope, Space, Verb};
    use ikigai_sparql::Store;

    use super::explain_config;
    use crate::config::BrowseSettings;

    /// The tiers the live config already uses, so the assertions below read as
    /// "what this server offers today" rather than as invented values.
    const FILE_TIER: &str = "urn:llm:coder:ask";
    const DIR_TIER: &str = "urn:llm:ask";
    /// Deliberately distinct from the two explain tiers, so the assertions can
    /// see that configuring the review / PR passes does NOT widen what an
    /// explain request may name.
    const REVIEW_PROVIDER: &str = "urn:llm:review-only:ask";
    const PR_PROVIDER: &str = "urn:llm:pr-only:ask";

    fn settings(root: PathBuf, allow: &[&str]) -> BrowseSettings {
        BrowseSettings {
            roots: vec![("demo".to_string(), root)],
            store: PathBuf::new(),
            file_model: FILE_TIER.to_string(),
            dir_model: DIR_TIER.to_string(),
            review_model: REVIEW_PROVIDER.to_string(),
            pr_model: PR_PROVIDER.to_string(),
            file_max_tokens: None,
            dir_max_tokens: None,
            review_max_tokens: None,
            pr_max_tokens: None,
            review_model_label: None,
            pr_model_label: None,
            allow_models: allow.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    /// Every provider a `provider=` request may name here, read back the way a
    /// client reads it: browse publishes the selectable set as the explain
    /// action's `provider` ArgSpec `one_of`, denies anything outside that same
    /// set, and builds the option menu from it — so one assertion covers what
    /// is offered, what is accepted, and what is refused.
    ///
    /// An in-memory store: nothing here touches the archive, and opening the
    /// real RocksDB one would take the exclusive lock the running server holds.
    fn offered(allow: &[&str]) -> Vec<String> {
        let root = std::env::temp_dir().join("ikigai-dev-menu-test");
        std::fs::create_dir_all(&root).expect("temp root");
        let settings = settings(root, allow);
        let store = Arc::new(Store::new().expect("in-memory store"));
        let space = ikigai_browse::Mount::new(settings.roots.clone())
            .explain(explain_config(&settings, &store))
            .app("dev-server")
            .space();

        let request = Request::new(
            Verb::Source,
            Iri::parse("urn:repo:demo:explain:src/main.rs").expect("iri"),
        );
        let Resolution::Hit(hit) = space.resolve(&request, &Scope::empty()) else {
            panic!("the explain grammar no longer matches urn:repo:demo:explain:{{path}}")
        };
        hit.endpoint
            .describe()
            .inputs
            .iter()
            .find(|input| input.name == "provider")
            .expect("explain publishes a `provider` ArgSpec")
            .one_of
            .clone()
    }

    /// Unconfigured, the menu offers exactly the two tiers this server already
    /// asks — the bump changes no behaviour until an operator says so.
    #[test]
    fn default_offers_only_the_two_tiers() {
        let set = offered(&[]);
        assert_eq!(set, [DIR_TIER, FILE_TIER]);
        // Configured, asked by this host, and still not selectable: the review
        // and PR providers derive OTHER actions, and folding them in would
        // widen explain's authority as a side effect of an unrelated key.
        assert!(!set.contains(&REVIEW_PROVIDER.to_string()), "{set:?}");
        assert!(!set.contains(&PR_PROVIDER.to_string()), "{set:?}");
    }

    /// A configured backend joins the set; anything the operator did NOT name
    /// stays out of it, which is the same set browse denies from.
    #[test]
    fn allowed_models_widen_the_set_and_nothing_else_does() {
        let set = offered(&["urn:llm:big:ask", "urn:llm:mlx:ask"]);
        assert_eq!(
            set,
            [
                "urn:llm:ask",
                "urn:llm:big:ask",
                "urn:llm:coder:ask",
                "urn:llm:mlx:ask"
            ]
        );
        // A backend the registry holds but the operator did not name is out —
        // and out of this list is exactly what browse answers `Denied` for.
        assert!(!set.contains(&"urn:llm:ollama:ask".to_string()), "{set:?}");
    }
}
