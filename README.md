# ikigai-dev-server

A standalone, **linkage-gated** [ikigai](https://github.com/ikigai-rs) server for
development tooling. Its `Cargo.toml` *is* the module manifest: the binary
composes a curated space and links **only** what it serves.

```
ikigai-dev [socket]              # default: $TMPDIR/ikigai-dev.sock
ikigai --connect <socket>        # drive it from the REPL
```

## What it serves

| resource | from |
|----------|------|
| `urn:system:exec`, `urn:repo:*` | `ikigai-repo` — git/gh/cargo as capability-gated resources |
| `urn:rdf:*` | `ikigai-rdf` — graph union/diff/transrept |
| `urn:sparql:*` | `ikigai-sparql` — query |

Run it *in* a project directory and it serves that project's git state
(`source urn:repo:status`, `:log`, `:branch`), or pass `dir=` for another repo.

## Why linkage-gating

`ikigai mcp --grant dev` on the omnibus binary *config-gates* — it hides the
calendar and LLM tools, but their code is still linked and reachable in-process.
This binary **doesn't compile them in at all**: EventKit, the LLM backend, and
the calendar are not present, so a flaw in any of them cannot be reached. The
security bound is the dependency list, not a runtime check.

On top of that, the exec seam is wrapped in a
[`RateLimit`](https://github.com/ikigai-rs/ikigai-throttle) (30 subprocess
spawns / minute by default), so a runaway or buggy agent can't hammer git or
GitHub through it. Graph ops are pure and local, so they pass unlimited.

## Security posture (honest)

Two real bounds today — **linkage** (only these modules exist) and
**rate-limiting** (exec is capped) — plus the transport's peercred check
(owner-only socket). A per-scope capability *ceiling* enforced server-side is
the "capability-on-the-wire" work; until then the local owner is trusted and the
reachable surface is bounded by what is linked and limited. An MCP face (so an
agent connects under a grant) is a fast-follow.

## License
MIT OR Apache-2.0.
