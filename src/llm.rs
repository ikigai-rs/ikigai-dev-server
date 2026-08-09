//! `urn:llm:*` wiring — the explain seam's derivation engine.
//!
//! **Why an LLM module is linked in a linkage-gated binary** (decided, not
//! drifted into): the explain family (`urn:repo:{repo}:explain`) derives
//! orientation through `urn:llm:*`, and `ikigai-llm` is an outbound HTTP
//! client to local inference servers (Ollama, mlx_lm.server, …) — not the
//! EventKit/TCC class of platform authority the linkage-gating posture exists
//! to exclude. The rejected alternative — mounting `urn:llm:` from the main
//! host — would couple every fresh explanation to the main host's uptime,
//! putting the dev seam behind the very process this server exists to stand
//! apart from. The seam is rate-limited like exec (see `main`): derivations
//! are expensive.
//!
//! The space is only mounted when the browse family is configured — an
//! unconfigured server serves exactly the original curated surface, and this
//! socket has no business offering general inference on its own.
//!
//! The provider registry is `~/.config/ikigai/llm.json` — the SAME file the
//! main host reads, so a machine's LLM setup is declared once (config home
//! only; no environment-variable channel). Absent, a local Ollama default;
//! present but unparseable, LOUD — a config that exists but does not parse
//! must never look like a config that is not there.

use std::sync::Arc;

use ikigai_core::EndpointSpace;

use crate::config;

/// The LLM module space (`urn:llm:ask` + `urn:llm:{provider}:ask` + `:models`
/// etc.) on the native ureq transport.
pub fn space() -> EndpointSpace {
    ikigai_llm::space(Arc::new(UreqTransport), registry())
}

/// The declared registry: the config-home `llm.json`, else the Ollama default.
fn registry() -> ikigai_llm::Registry {
    let path = config::config_dir().join("llm.json");
    match std::fs::read_to_string(&path) {
        Ok(json) => ikigai_llm::Registry::from_json(&json).unwrap_or_else(|e| {
            panic!(
                "ikigai-dev: {} parse error: {e:?} — fix the file (explanations \
                 derived against a silently-defaulted model would be mislabeled)",
                path.display()
            )
        }),
        // Absent is genuinely unconfigured: the same local-Ollama default the
        // main host uses, declared trait profile included.
        Err(_) => {
            let mut ollama = ikigai_llm::OpenAiConfig::ollama("llama3.2:3b");
            ollama.caps.context = Some(131_072);
            ollama.caps.modalities = vec!["text".to_string()];
            ollama.caps.params = Some("3B".to_string());
            ikigai_llm::Registry::single(ollama)
        }
    }
}

/// The native HTTP transport backing `urn:llm:*`: a blocking `ureq` client
/// under the async trait. Runtime-free — no Tokio; the executor stays chosen
/// at the edge (the IPC server's per-connection block_on).
struct UreqTransport;

#[async_trait::async_trait]
impl ikigai_http::HttpTransport for UreqTransport {
    async fn send(
        &self,
        request: ikigai_http::HttpRequest,
    ) -> std::result::Result<ikigai_http::HttpResponse, String> {
        use std::io::Read;
        // The HttpTransport contract (ikigai-http ≥ 0.1.7) forbids following
        // redirects here: the ENDPOINT follows them, re-running the
        // net-capability ACL against every hop. `redirects(0)` returns the
        // 3xx as-is.
        let agent = ureq::builder().redirects(0).build();
        let mut req = agent.request(request.method.as_str(), &request.url);
        for (name, value) in &request.headers {
            req = req.set(name, value);
        }
        let outcome = if request.body.is_empty() {
            req.call()
        } else {
            req.send_bytes(&request.body)
        };
        // A 4xx/5xx is still a response (with a body), not a transport failure.
        let resp = match outcome {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_, resp)) => resp,
            Err(e) => return Err(e.to_string()),
        };
        let status = resp.status();
        let headers = resp
            .headers_names()
            .into_iter()
            .filter_map(|name| resp.header(&name).map(|v| (name.clone(), v.to_string())))
            .collect();
        // A HEAD response carries headers only — no body to read.
        let mut body = Vec::new();
        if request.method != ikigai_http::Method::Head {
            resp.into_reader()
                .read_to_end(&mut body)
                .map_err(|e| format!("reading response body: {e}"))?;
        }
        Ok(ikigai_http::HttpResponse {
            status,
            headers,
            body,
        })
    }
}
