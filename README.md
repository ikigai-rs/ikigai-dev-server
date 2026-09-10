# ikigai-dev-server

A standalone, **linkage-gated** [ikigai](https://github.com/ikigai-rs) server for
development tooling. Its `Cargo.toml` *is* the module manifest: the binary
composes a curated space and links **only** what it serves.

```
ikigai-dev [socket] [flags]      # default: ~/.ikigai/dev.sock
ikigai --connect <socket>        # drive it from the REPL
ikigai-dev --help                # flags + the config-file grammar
```

## What it serves

| resource | from |
|----------|------|
| `urn:system:exec`, `urn:repo:*` | `ikigai-repo` — git/gh/cargo as capability-gated resources |
| `urn:rdf:*` | `ikigai-rdf` — graph union/diff/transrept |
| `urn:sparql:*` | `ikigai-sparql` — query |
| `urn:repo:{repo}:tree/file/state/hash/explain/…`, `urn:iki:annotation:*` | `ikigai-browse` — the browse family, **when configured** (see below) |
| `urn:llm:*` | `ikigai-llm` — the explain seam's derivation engine, mounted **only alongside browse** |

Run it *in* a project directory and it serves that project's git state
(`source urn:repo:status`, `:log`, `:branch`), or pass `dir=` for another repo.

## The browse family (opt-in)

**This server owns the browse store on a machine** (decision of record): the
persistent explanation/annotation archive is RocksDB under an exclusive lock,
so ONE process holds it and everyone else prefer-mounts this server's socket —
which is why the default socket is the stable `~/.ikigai/dev.sock` rather than
something under `$TMPDIR` that churns across reboots.

Configure it in `~/.config/ikigai/dev.toml` (flags override the file; there is
deliberately no environment-variable channel):

```toml
socket = "~/.ikigai/dev.sock"                # this IS the default

# One line per root; the URN repo name is the directory's basename.
browse.root = "~/git-personal/ikigai-core"
browse.root = "~/git-personal/ikigai-cli"

# Optional once a root exists:
browse.store = "~/.ikigai/browse-store"      # this IS the default
browse.file_model = "coder"                  # urn:llm:coder:ask
browse.dir_model = "ask"                     # urn:llm:ask
browse.file_max_tokens = 400
browse.dir_max_tokens = 600
browse.review_model = "coder"                # review pass (urn:repo:{repo}:review:{path})
browse.review_max_tokens = 800               # findings carry quotes — raise for big files/PRs
browse.pr_model = "coder"                    # pull-request explain (the PR family)
browse.pr_max_tokens = 600
browse.review_model_label = "qwen3:30b"      # operator overrides folded into version tags;
browse.pr_model_label = "qwen3:30b"          # unset, the true model id resolves at derive time
browse.allow_model = "big"                   # ALSO selectable by an explain `provider=`
browse.allow_model = "ollama"                # (repeatable; same spellings; default: none)
```

`browse.allow_model` is the only key here that grants reach rather than tuning
it. Browse's explain takes `provider={iri}` — derive THIS explanation against a
named backend — and the option menu beside the explain button offers exactly the
set this host accepts: the two configured tiers (`file_model`, `dir_model`) plus
whatever is allow-listed. Anything else is `Denied`, never a silent fall back.
The list is the **operator's** because `explain` declares one capability
(`urn:cap:net:*`) and a capability cannot vary by argument value — it says "may
derive", not "may derive against the metered vendor" — so a request argument must
not be able to spend on the operator's behalf. Unset, the menu offers the two
tiers alone and this server behaves exactly as it did before the key existed.
Configuring the review or PR provider does **not** widen the set: those derive
other actions, and granting explain authority as a side effect of an unrelated
line is the surprise this key exists to avoid.

No `browse.root` ⇒ no browse family at all (and no `urn:llm:*`) — the original
curated surface, unchanged. A `browse.*` tuning **without** a root fails loud at
startup: half-configured must never come up looking healthy. With roots, the
store opens (or fails loud — never a silent in-memory archive), the bundled
vocabulary loads, and `urn:sparql:*` binds **over the same store**, so one
query joins `ik:Explanation` and `oa:Annotation` rows live.

Every other process on the machine reaches the family through this socket
(in the main host's `~/.config/ikigai/config.toml`):

```toml
mount = "prefer urn:repo:=~/.ikigai/dev.sock"
mount = "prefer urn:iki:annotation=~/.ikigai/dev.sock"
```

⚠ The annotation line carries **no trailing colon**, deliberately. Mounts
match by plain string prefix, and `urn:iki:annotation` covers the slug family
(`urn:iki:annotation:{id}`) *and* the bare `urn:iki:annotation` a Sink mints
under — the only write path the browse overlay has. Written
`urn:iki:annotation:=`, reads keep working and every new annotation silently
fails to route.

### Upgrading a pre-0.3.0 store

`ikigai-browse` 0.3.0 moved the annotation namespace from `urn:annotation:`
to `urn:iki:annotation:`. **A store written before that upgrade does not
migrate itself, and the failure is silent**: browse's listing paths
(`list_annotations`, `list_annotations_for_target`, `included_for_ids`)
`strip_prefix` the *stored subject IRI*, so an old-prefix annotation is
skipped rather than erroring — the panel comes back empty and nothing says
why. Review passes lose their minted findings the same way, through the
`prov:generated` objects that name them.

Count what a store holds before upgrading (this query runs over the store
this process owns, so ask it through the socket rather than opening the
RocksDB directory yourself — the lock is exclusive):

```sh
ikigai --connect ~/.ikigai/dev.sock --plain -c \
  'source urn:sparql:select query="SELECT (COUNT(*) AS ?c) WHERE { ?s ?p ?o . FILTER(STRSTARTS(STR(?s), \"urn:annotation:\")) }"'
```

A non-zero count means a one-time rename is owed, in three positions that
must all move together: annotation subjects, their `:selector:quote` /
`:selector:position` children, and every IRI *object* naming one
(`oa:hasSelector`, `prov:generated`). That is an operator step with this
server stopped — the store takes an exclusive lock, so nothing can rewrite
it while the socket is up.

The LLM provider registry is the shared `~/.config/ikigai/llm.json` (absent ⇒
a local Ollama default; present-but-unparseable ⇒ loud).

## Why linkage-gating

`ikigai mcp --grant dev` on the omnibus binary *config-gates* — it hides the
calendar tools, but their code is still linked and reachable in-process. This
binary **doesn't compile them in at all**: EventKit and the calendar are not
present, so a flaw in either cannot be reached. The security bound is the
dependency list, not a runtime check.

`ikigai-llm` **is** linked — deliberately, and only mounted when browse is
configured. The explain seam derives through `urn:llm:*`, and ikigai-llm is an
outbound HTTP client to *local* inference servers — not the EventKit/TCC class
of platform authority the gating posture exists to exclude. The rejected
alternative (mounting `urn:llm:` from the main host) would couple every fresh
explanation to the main host's uptime — the dev seam would degrade with the
very process it exists to stand apart from.

On top of that, the expensive seams are wrapped in a
[`RateLimit`](https://github.com/ikigai-rs/ikigai-throttle): 30 subprocess
spawns/min on exec, 120 reads/min on `urn:repo:`, and 30 asks/min on
`urn:llm:` (derivations are expensive; the limit governs explain's internal
asks too, since subrequests resolve back through the overlay). Pure local
graph ops pass unlimited.

## Conformance, and what a mounting client actually gets

`tests/conformance.rs` runs [`ikigai-conformance`](https://github.com/ikigai-rs/ikigai-conformance)
over the kernel this binary serves — the same `ikigai_dev_server::compose` that
`main` calls, not a re-creation of it. This crate binds **zero endpoints of its
own**, so the test is not "does my module conform" but three questions a
composing host is the only place to ask:

- **Is the served catalog exactly what the manifest composes?** Every pattern is
  pinned with the crate that owns it, so linkage-gating is a checked fact rather
  than a paragraph, and a dependency that grows an endpoint puts it on this
  socket only after someone classifies it.
- **Does every entry answer `Verb::Meta` with a real contract?** This process is
  the peer on the other end of everyone else's `mount` line, and a peer with no
  Meta renderer does not fail — it degrades. `ForwardingEndpoint::describe` is
  best-effort and falls back to `Description::new("remote")`, so a renderer-less
  peer shows up in a client's catalog as one anonymous, action-less row: a whole
  federated kernel reading as *small* rather than broken. The test pins the JSON
  Meta face specifically, because that is the one a mount parses.
- **What does a mount cost?** Golden threads are `#[serde(skip)]` and do not
  cross a wire, so a cacheable representation arrives at a mounting client with
  nothing that can ever cut it. Exactly one resource this server serves has a
  thread to lose — `urn:repo:style`, which hangs on the layered `a11y.toml`
  files. Everything the browse family reads out of a working tree is
  `Expiry::Always`, live by design: this process runs no filesystem watcher, and
  a server must not mint a thread no host keeps.

Findings from the composed modules (`ikigai-repo`, `ikigai-rdf`,
`ikigai-sparql`, `ikigai-browse`, `ikigai-llm`) are **recorded and attributed,
not fixed here** — they belong to those repos. Today: 103, of which 99 are
untyped inputs. Zero are this crate's, and the test fails if a finding ever
names an endpoint the catalog table does not.

## Security posture (honest)

Three real bounds today — **linkage** (only these modules exist),
**rate-limiting** (exec and llm are capped), and the transport's peercred
check (owner-only socket). A per-scope capability *ceiling* enforced
server-side is the "capability-on-the-wire" work; until then the local owner
is trusted and the reachable surface is bounded by what is linked and limited.
Browse's own capability story (`urn:cap:browse:read:*`, `urn:cap:annotate`)
comes with the crate. An MCP face (so an agent connects under a grant) is a
fast-follow.

## License
MIT OR Apache-2.0.
