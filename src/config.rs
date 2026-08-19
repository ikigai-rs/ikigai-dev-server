//! Configuration: a config-home file plus command-line flags — flags override
//! the file, and there is deliberately **no environment-variable channel**
//! (two sources, not three).
//!
//! The file is `~/.config/ikigai/dev.toml` (honouring `XDG_CONFIG_HOME`) — the
//! dev server's OWN file, not the main host's `config.toml`: the host's
//! instance-scoped browse grammar (`serve.browse.root = …`) and this server's
//! flat one must never claim each other's lines, and an unscoped `browse.root`
//! in the shared file would put every cli process on the store lock this
//! server is supposed to own. A minimal `key = "value"` scanner (the cli
//! config's convention): `#` comments, dotted keys are ordinary keys,
//! repeatable keys repeat.
//!
//! ```toml
//! socket = "~/.ikigai/dev.sock"                # where to serve (this IS the default)
//!
//! # The browse family: one line per root; the URN repo name is the
//! # directory's basename (urn:repo:ikigai-core:tree, …). Repeatable.
//! browse.root = "~/git-personal/ikigai-core"
//! browse.root = "~/git-personal/ikigai-cli"
//!
//! # Everything below is optional once a root exists.
//! browse.store = "~/.ikigai/browse-store"      # persistent archive (this IS the default)
//! browse.file_model = "coder"                  # explain file grain: urn:llm:{id}:ask, or a full IRI
//! browse.dir_model = "ask"                     # explain dir rollup ("ask" = urn:llm:ask)
//! browse.file_max_tokens = 400                 # explain ceilings (the crate's defaults shown)
//! browse.dir_max_tokens = 600
//! browse.review_model = "coder"                # review pass (urn:repo:{repo}:review:{path})
//! browse.review_max_tokens = 800               # review ceiling (findings carry quotes)
//! browse.pr_model = "coder"                    # pull-request explain (the PR family)
//! browse.pr_max_tokens = 600
//! browse.review_model_label = "qwen3:30b"      # operator overrides folded into version tags;
//! browse.pr_model_label = "qwen3:30b"          # unset, the true model id resolves at derive time
//! browse.allow_model = "big"                   # ALSO selectable by an explain `provider=`
//! browse.allow_model = "ollama"                # (repeatable; same spellings; default: none)
//! ```
//!
//! `browse.allow_model` is the one key here that is not a tuning but an
//! **authority** decision, so it is worth saying why it exists. Browse's
//! explain takes `provider={iri}` — derive THIS explanation against a named
//! backend — and the option menu beside the explain button offers exactly the
//! set this host will accept: the two configured tiers (`file_model`,
//! `dir_model`) plus whatever the host allow-lists. Anything else is `Denied`,
//! never a silent fall back. It must be the OPERATOR's list rather than the
//! caller's argument because `explain` declares one capability
//! (`urn:cap:net:*`) and a capability cannot vary by argument value: it says
//! "may derive", never "may derive against the metered vendor". So every
//! provider named here is one that any caller who may explain at all may point
//! this host at — add a backend when you would be content to pay for it.
//! Empty by default: an unconfigured server offers the two tiers and nothing
//! more, exactly as before the key existed.
//!
//! No `browse.root` line and no `--root` flag ⇒ **no browse family at all** —
//! absence, not error: the module is opt-in and an unconfigured server must
//! not even hint at it in the catalog. But a `browse.*` tuning with no root is
//! a HALF-configuration and fails LOUD: someone plainly intended browse, and a
//! server that came up silently without it would look healthy while answering
//! nothing.

use std::path::PathBuf;

/// How to invoke the binary; printed on `--help` and on a flag error.
pub const USAGE: &str = "\
ikigai-dev [socket] [flags]

  socket                    where to serve (default ~/.ikigai/dev.sock; also `--socket`)
  --config <file>           read this file instead of ~/.config/ikigai/dev.toml
  --socket <path>           where to serve (overrides the file and the positional form)
  --root <dir>              browse root (repeatable; overrides ALL browse.root lines)
  --store <path>            explanation/annotation archive (default ~/.ikigai/browse-store)
  --file-model <id>         explain file grain: an id (urn:llm:{id}:ask), `ask`, or a full IRI
  --dir-model <id>          explain dir rollup, same spellings
  --file-max-tokens <n>     explain ceilings (defaults: 400 file, 600 dir)
  --dir-max-tokens <n>
  --review-model <id>       review pass (urn:repo:{repo}:review:{path}), same spellings
  --pr-model <id>           pull-request explain, same spellings
  --review-max-tokens <n>   review/pr ceilings (defaults: 800 review, 600 pr)
  --pr-max-tokens <n>
  --review-model-label <s>  operator overrides folded into version tags (unset, the
  --pr-model-label <s>      true model id resolves at derive time)
  --allow-model <id>        ALSO selectable by an explain `provider=` request; same
                            spellings (repeatable; overrides ALL browse.allow_model lines)
  --help

Config file grammar (flags override it):
  socket = \"~/.ikigai/dev.sock\"
  browse.root = \"~/git-personal/ikigai-core\"     # repeatable; enables the browse family
  browse.store = \"~/.ikigai/browse-store\"
  browse.file_model = \"coder\"
  browse.dir_model = \"ask\"
  browse.file_max_tokens = 400
  browse.dir_max_tokens = 600
  browse.review_model = \"coder\"
  browse.review_max_tokens = 800
  browse.pr_model = \"coder\"
  browse.pr_max_tokens = 600
  browse.review_model_label = \"qwen3:30b\"
  browse.pr_model_label = \"qwen3:30b\"
  browse.allow_model = \"big\"                     # repeatable; default: none
";

/// Root names the browse grammar must not claim: `ikigai-repo` (also linked
/// here) binds `urn:repo:status` / `:log` / `:branch` / `:list` / `:pr:*` as
/// Exacts, and a root of the same name would interleave two families under one
/// URN segment, with which-one-wins decided by space order. Refused at startup
/// instead. (Host-side by design — the crate cannot know what else this binary
/// binds under `urn:repo:`.)
const RESERVED_ROOTS: [&str; 5] = ["status", "log", "branch", "list", "pr"];

/// Everything `main` needs, merged from file and flags and validated loud.
pub struct Settings {
    /// Where to serve. Stable by default (`~/.ikigai/dev.sock`) — mounts in
    /// other processes' configs name this path, so it must not churn the way
    /// `$TMPDIR` does across reboots.
    pub socket: PathBuf,
    /// The browse family's knobs, or `None` when no root is configured.
    pub browse: Option<BrowseSettings>,
}

/// The browse family, fully resolved: existing directories, non-reserved
/// names, numeric ceilings.
pub struct BrowseSettings {
    /// `(name, directory)` pairs; the name is the directory's basename.
    pub roots: Vec<(String, PathBuf)>,
    /// The persistent archive path (RocksDB; exclusive lock).
    pub store: PathBuf,
    /// Provider IRI for the explain file grain.
    pub file_model: String,
    /// Provider IRI for the explain directory rollup.
    pub dir_model: String,
    /// Provider IRI for the review pass.
    pub review_model: String,
    /// Provider IRI for the pull-request explain.
    pub pr_model: String,
    /// Explain ceilings; `None` keeps the crate's defaults (400 / 600).
    pub file_max_tokens: Option<u32>,
    pub dir_max_tokens: Option<u32>,
    /// Review / pr-explain ceilings; `None` keeps the crate's defaults
    /// (800 / 600).
    pub review_max_tokens: Option<u32>,
    pub pr_max_tokens: Option<u32>,
    /// Operator overrides folded into review / pr version tags; `None` (the
    /// default) resolves the true model identity through the kernel at
    /// derivation time, so a model swap re-keys the archive by itself.
    pub review_model_label: Option<String>,
    pub pr_model_label: Option<String>,
    /// Provider IRIs an explain REQUEST may name with `provider=`, on top of
    /// the two tiers browse always makes selectable. Empty by default, so the
    /// menu offers `file_model` and `dir_model` alone — what this server
    /// offered before the knob existed.
    ///
    /// ★ An authority list, not a tuning. `explain` declares `urn:cap:net:*`
    /// and an `ActionSpec` cannot express a capability that varies by argument
    /// value, so the cap means "may derive", not "may derive against the
    /// metered vendor": the operator names which backends a caller may spend,
    /// and browse republishes the set as the `provider` ArgSpec's `one_of` so
    /// the manifold states it. The review and PR providers are deliberately
    /// NOT folded in — those are the models this host derives two OTHER
    /// actions with, and making `browse.review_model` widen explain's
    /// selectable set would grant reach as a side effect of an unrelated line.
    /// An operator who wants one offered names it here.
    pub allow_models: Vec<String>,
}

/// Read the real command line and the real config file. Exits on `--help`.
pub fn settings() -> Settings {
    let flags = Flags::parse(std::env::args().skip(1));
    if flags.help {
        print!("{USAGE}");
        std::process::exit(0);
    }
    let text = match &flags.config {
        // An EXPLICIT --config that cannot be read fails loud: expected-but-
        // unset must stop, never silently degrade to defaults.
        Some(path) => std::fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("ikigai-dev: --config {path}: {e} — fix the path (or drop the flag)")
        }),
        // The default file is genuinely optional: absent means unconfigured.
        None => std::fs::read_to_string(config_dir().join("dev.toml")).unwrap_or_default(),
    };
    merge(&flags, &text)
}

/// `$XDG_CONFIG_HOME/ikigai`, or `~/.config/ikigai` — the shared ikigai config
/// home (the main host's `config.toml` and `llm.json` live here too).
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
            home.join(".config")
        });
    base.join("ikigai")
}

/// `~/`-expansion for config paths, matching the cli config convention.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// The raw command line. Every browse-flavoured flag also counts as "browse
/// was intended" for the half-configuration check.
#[derive(Default)]
struct Flags {
    help: bool,
    config: Option<String>,
    socket: Option<String>,
    roots: Vec<String>,
    store: Option<String>,
    file_model: Option<String>,
    dir_model: Option<String>,
    review_model: Option<String>,
    pr_model: Option<String>,
    file_max_tokens: Option<String>,
    dir_max_tokens: Option<String>,
    review_max_tokens: Option<String>,
    pr_max_tokens: Option<String>,
    review_model_label: Option<String>,
    pr_model_label: Option<String>,
    allow_models: Vec<String>,
}

impl Flags {
    /// Parse the argument vector. Unknown flags fail loud with the usage; one
    /// bare positional argument is the socket (the original invocation shape).
    fn parse(args: impl Iterator<Item = String>) -> Flags {
        let mut flags = Flags::default();
        let mut args = args;
        let next_value = |args: &mut dyn Iterator<Item = String>, flag: &str| {
            args.next()
                .unwrap_or_else(|| panic!("ikigai-dev: {flag} needs a value\n\n{USAGE}"))
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => flags.help = true,
                "--config" => flags.config = Some(next_value(&mut args, "--config")),
                "--socket" => flags.socket = Some(next_value(&mut args, "--socket")),
                "--root" => flags.roots.push(next_value(&mut args, "--root")),
                "--store" => flags.store = Some(next_value(&mut args, "--store")),
                "--file-model" => flags.file_model = Some(next_value(&mut args, "--file-model")),
                "--dir-model" => flags.dir_model = Some(next_value(&mut args, "--dir-model")),
                "--file-max-tokens" => {
                    flags.file_max_tokens = Some(next_value(&mut args, "--file-max-tokens"));
                }
                "--dir-max-tokens" => {
                    flags.dir_max_tokens = Some(next_value(&mut args, "--dir-max-tokens"));
                }
                "--review-model" => {
                    flags.review_model = Some(next_value(&mut args, "--review-model"));
                }
                "--pr-model" => flags.pr_model = Some(next_value(&mut args, "--pr-model")),
                "--review-max-tokens" => {
                    flags.review_max_tokens = Some(next_value(&mut args, "--review-max-tokens"));
                }
                "--pr-max-tokens" => {
                    flags.pr_max_tokens = Some(next_value(&mut args, "--pr-max-tokens"));
                }
                "--review-model-label" => {
                    flags.review_model_label = Some(next_value(&mut args, "--review-model-label"));
                }
                "--pr-model-label" => {
                    flags.pr_model_label = Some(next_value(&mut args, "--pr-model-label"));
                }
                "--allow-model" => flags
                    .allow_models
                    .push(next_value(&mut args, "--allow-model")),
                other if other.starts_with('-') => {
                    panic!("ikigai-dev: unknown flag {other}\n\n{USAGE}")
                }
                // The original positional socket, kept working. Loud on a
                // second one — a typo'd flag value would land here.
                other => {
                    assert!(
                        flags.socket.is_none(),
                        "ikigai-dev: unexpected argument {other} (socket already given)\n\n{USAGE}"
                    );
                    flags.socket = Some(other.to_string());
                }
            }
        }
        flags
    }

    /// Did any browse-flavoured flag appear? (For the half-configuration check.)
    fn wants_browse(&self) -> bool {
        !self.roots.is_empty()
            || self.store.is_some()
            || self.file_model.is_some()
            || self.dir_model.is_some()
            || self.review_model.is_some()
            || self.pr_model.is_some()
            || self.file_max_tokens.is_some()
            || self.dir_max_tokens.is_some()
            || self.review_max_tokens.is_some()
            || self.pr_max_tokens.is_some()
            || self.review_model_label.is_some()
            || self.pr_model_label.is_some()
            || !self.allow_models.is_empty()
    }
}

/// Merge flags over file text into validated [`Settings`]. Fails loud on a
/// half-configuration, a missing root directory, a reserved root name, or a
/// non-numeric ceiling.
fn merge(flags: &Flags, text: &str) -> Settings {
    let socket = flags
        .socket
        .clone()
        .or_else(|| value_for(text, "socket"))
        .unwrap_or_else(|| "~/.ikigai/dev.sock".to_string());

    // Roots: flags replace the file's lines wholesale (a flag override that
    // MERGED with the file could not turn a root off).
    let root_lines = if flags.roots.is_empty() {
        values_for(text, "browse.root")
    } else {
        flags.roots.clone()
    };

    let browse = if root_lines.is_empty() {
        // No roots anywhere. A browse TUNING without a root is a half-
        // configuration: fail loud rather than come up silently browse-less.
        let dangling: Vec<&str> = browse_keys(text);
        assert!(
            dangling.is_empty() && !flags.wants_browse(),
            "ikigai-dev: browse settings ({}) but no browse.root / --root — \
             a server that started anyway would look configured while serving no \
             browse family at all. Add at least one root, or remove the settings.",
            if dangling.is_empty() {
                "flags".to_string()
            } else {
                dangling.join(", ")
            }
        );
        None
    } else {
        Some(BrowseSettings {
            roots: named_roots(&root_lines),
            store: expand_home(
                &flags
                    .store
                    .clone()
                    .or_else(|| value_for(text, "browse.store"))
                    .unwrap_or_else(|| "~/.ikigai/browse-store".to_string()),
            ),
            file_model: provider_iri(
                &flags
                    .file_model
                    .clone()
                    .or_else(|| value_for(text, "browse.file_model"))
                    .unwrap_or_else(|| "coder".to_string()),
            ),
            dir_model: provider_iri(
                &flags
                    .dir_model
                    .clone()
                    .or_else(|| value_for(text, "browse.dir_model"))
                    .unwrap_or_else(|| "ask".to_string()),
            ),
            review_model: provider_iri(
                &flags
                    .review_model
                    .clone()
                    .or_else(|| value_for(text, "browse.review_model"))
                    .unwrap_or_else(|| "coder".to_string()),
            ),
            pr_model: provider_iri(
                &flags
                    .pr_model
                    .clone()
                    .or_else(|| value_for(text, "browse.pr_model"))
                    .unwrap_or_else(|| "coder".to_string()),
            ),
            file_max_tokens: ceiling(
                "browse.file_max_tokens",
                flags
                    .file_max_tokens
                    .clone()
                    .or_else(|| value_for(text, "browse.file_max_tokens")),
            ),
            dir_max_tokens: ceiling(
                "browse.dir_max_tokens",
                flags
                    .dir_max_tokens
                    .clone()
                    .or_else(|| value_for(text, "browse.dir_max_tokens")),
            ),
            review_max_tokens: ceiling(
                "browse.review_max_tokens",
                flags
                    .review_max_tokens
                    .clone()
                    .or_else(|| value_for(text, "browse.review_max_tokens")),
            ),
            pr_max_tokens: ceiling(
                "browse.pr_max_tokens",
                flags
                    .pr_max_tokens
                    .clone()
                    .or_else(|| value_for(text, "browse.pr_max_tokens")),
            ),
            review_model_label: flags
                .review_model_label
                .clone()
                .or_else(|| value_for(text, "browse.review_model_label")),
            pr_model_label: flags
                .pr_model_label
                .clone()
                .or_else(|| value_for(text, "browse.pr_model_label")),
            // Repeatable, so it takes `browse.root`'s override rule and not
            // the single-valued knobs': the flags REPLACE the file's lines
            // wholesale. Merging would leave an allowlist that can only grow —
            // `--allow-model` could not take one of the file's backends back
            // off — which is the wrong direction for the one key here that
            // hands out reach.
            allow_models: if flags.allow_models.is_empty() {
                values_for(text, "browse.allow_model")
            } else {
                flags.allow_models.clone()
            }
            .iter()
            .map(String::as_str)
            .map(provider_iri)
            .collect(),
        })
    };

    Settings {
        socket: expand_home(&socket),
        browse,
    }
}

/// Which `browse.*` keys the file sets (for the half-configuration message).
fn browse_keys(text: &str) -> Vec<&str> {
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim();
            if name.starts_with("browse.") && !keys.contains(&name) {
                keys.push(name);
            }
        }
    }
    keys
}

/// Root lines → `(name, directory)` pairs, the name from the basename. Missing
/// dirs and reserved names fail loud here with the config line in hand; the
/// emptiness/`:`/`/`/duplicate checks live in `ikigai_browse`'s own mount-time
/// validation.
fn named_roots(lines: &[String]) -> Vec<(String, PathBuf)> {
    lines
        .iter()
        .map(|line| {
            let dir = expand_home(line);
            assert!(
                dir.is_dir(),
                "ikigai-dev: browse root `{line}` is not a directory — fix the config \
                 (a root that resolves against nothing would answer every request with \
                 an error)"
            );
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            assert!(
                !RESERVED_ROOTS.contains(&name.as_str()),
                "ikigai-dev: browse root `{line}`: the name `{name}` is reserved — \
                 ikigai-repo binds urn:repo:{name} — rename the directory or browse it \
                 under a symlinked name"
            );
            (name, dir)
        })
        .collect()
}

/// A provider id as an IRI: a full `urn:` IRI passes through, `ask` names the
/// facade (`urn:llm:ask`), and any other id is an `urn:llm:{id}:ask` backend
/// (`coder`, `mlx`, `big`, …). The cli's convention, verbatim.
fn provider_iri(value: &str) -> String {
    if value.starts_with("urn:") {
        value.to_string()
    } else if value == "ask" {
        "urn:llm:ask".to_string()
    } else {
        format!("urn:llm:{value}:ask")
    }
}

/// A configured token ceiling, or `None` when unset. Set-but-garbage fails
/// loud: a typo that silently fell back to the default would look configured.
fn ceiling(key: &str, value: Option<String>) -> Option<u32> {
    value.map(|v| {
        v.parse()
            .unwrap_or_else(|_| panic!("ikigai-dev: {key} `{v}` is not a number — fix the config"))
    })
}

/// The first `key = value` line for `key`, trimmed and unquoted. Blank lines
/// and `#` comments are skipped. Not a TOML parser — the flat `key = "value"`
/// shape the ikigai config convention uses.
fn value_for(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim() == key {
                return Some(unquote(value));
            }
        }
    }
    None
}

/// Every `key = value` line for `key`, in file order — for settings that
/// legitimately repeat (`browse.root`).
fn values_for(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| name.trim() == key)
        .map(|(_, value)| unquote(value))
        .collect()
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches(['"', '\'']).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{browse_keys, merge, provider_iri, values_for, Flags};

    /// The three spellings a model line can take: a backend id, the facade,
    /// and a full IRI.
    #[test]
    fn provider_ids_become_iris() {
        assert_eq!(provider_iri("coder"), "urn:llm:coder:ask");
        assert_eq!(provider_iri("ask"), "urn:llm:ask");
        assert_eq!(provider_iri("urn:llm:mlx:ask"), "urn:llm:mlx:ask");
    }

    /// Repeatable keys all come back in order; quoted and unquoted values read
    /// the same; comments are skipped.
    #[test]
    fn repeated_keys_all_come_back() {
        let text = "# roots\nbrowse.root = \"~/a\"\nother = 1\nbrowse.root = ~/b\n";
        assert_eq!(
            values_for(text, "browse.root"),
            vec!["~/a".to_string(), "~/b".to_string()]
        );
        assert!(values_for(text, "absent").is_empty());
    }

    /// No config at all ⇒ the defaults: the stable socket, no browse family.
    #[test]
    fn empty_config_is_the_bare_server() {
        let settings = merge(&Flags::default(), "");
        assert!(settings.socket.ends_with(".ikigai/dev.sock"));
        assert!(settings.browse.is_none());
    }

    /// A browse tuning with no root is a half-configuration — loud, not a
    /// silently browse-less server.
    #[test]
    #[should_panic(expected = "no browse.root")]
    fn tuning_without_a_root_is_refused() {
        merge(&Flags::default(), "browse.file_model = \"coder\"\n");
    }

    /// Same check for the flag channel.
    #[test]
    #[should_panic(expected = "no browse.root")]
    fn flag_tuning_without_a_root_is_refused() {
        let flags = Flags {
            store: Some("/tmp/x".to_string()),
            ..Flags::default()
        };
        merge(&flags, "");
    }

    /// A ceiling that is set but not numeric fails loud — a typo must never
    /// silently become the default.
    #[test]
    #[should_panic(expected = "not a number")]
    fn garbage_ceiling_is_refused() {
        let dir = std::env::temp_dir();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            file_max_tokens: Some("many".to_string()),
            ..Flags::default()
        };
        merge(&flags, "");
    }

    /// Flags override the file: socket, and --root replaces ALL browse.root
    /// lines (an override that merged could not turn a root off).
    #[test]
    fn flags_override_the_file() {
        let dir = std::env::temp_dir().join("ikigai-dev-test-root");
        std::fs::create_dir_all(&dir).unwrap();
        let flags = Flags {
            socket: Some("/tmp/other.sock".to_string()),
            roots: vec![dir.to_string_lossy().into_owned()],
            file_model: Some("mlx".to_string()),
            ..Flags::default()
        };
        let text = "socket = \"/tmp/file.sock\"\nbrowse.root = \"/nonexistent/ignored\"\n";
        let settings = merge(&flags, text);
        assert_eq!(settings.socket.to_string_lossy(), "/tmp/other.sock");
        let browse = settings.browse.expect("browse configured");
        assert_eq!(browse.roots.len(), 1);
        assert_eq!(browse.roots[0].0, "ikigai-dev-test-root");
        assert_eq!(browse.file_model, "urn:llm:mlx:ask");
        // Untouched knobs fall to the defaults.
        assert_eq!(browse.dir_model, "urn:llm:ask");
        assert!(browse.store.ends_with(".ikigai/browse-store"));
        assert!(browse.file_max_tokens.is_none());
        assert_eq!(browse.review_model, "urn:llm:coder:ask");
        assert_eq!(browse.pr_model, "urn:llm:coder:ask");
        assert!(browse.review_max_tokens.is_none());
        assert!(browse.pr_max_tokens.is_none());
        assert!(browse.review_model_label.is_none());
        assert!(browse.pr_model_label.is_none());
        assert!(browse.allow_models.is_empty());
    }

    /// The review / pr knobs read from the file with the same spellings as the
    /// file/dir tiers: model ids map through `provider_iri`, labels pass
    /// through verbatim, ceilings parse.
    #[test]
    fn review_and_pr_knobs_flow_through() {
        let dir = std::env::temp_dir().join("ikigai-dev-test-root");
        std::fs::create_dir_all(&dir).unwrap();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            ..Flags::default()
        };
        let text = "browse.review_model = \"mlx\"\n\
                    browse.review_model_label = \"qwen3-coder:30b\"\n\
                    browse.review_max_tokens = 1600\n\
                    browse.pr_model = \"urn:llm:big:ask\"\n\
                    browse.pr_max_tokens = 900\n";
        let browse = merge(&flags, text).browse.expect("browse configured");
        assert_eq!(browse.review_model, "urn:llm:mlx:ask");
        assert_eq!(
            browse.review_model_label.as_deref(),
            Some("qwen3-coder:30b")
        );
        assert_eq!(browse.review_max_tokens, Some(1600));
        assert_eq!(browse.pr_model, "urn:llm:big:ask");
        assert!(browse.pr_model_label.is_none());
        assert_eq!(browse.pr_max_tokens, Some(900));
    }

    /// The review/pr flags override their file lines like every other knob.
    #[test]
    fn review_and_pr_flags_override_the_file() {
        let dir = std::env::temp_dir().join("ikigai-dev-test-root");
        std::fs::create_dir_all(&dir).unwrap();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            review_max_tokens: Some("2000".to_string()),
            pr_model_label: Some("pinned".to_string()),
            ..Flags::default()
        };
        let text = "browse.review_max_tokens = 800\nbrowse.pr_model_label = \"file\"\n";
        let browse = merge(&flags, text).browse.expect("browse configured");
        assert_eq!(browse.review_max_tokens, Some(2000));
        assert_eq!(browse.pr_model_label.as_deref(), Some("pinned"));
    }

    /// The new ceilings get the same set-but-garbage loudness as the old ones.
    #[test]
    #[should_panic(expected = "not a number")]
    fn garbage_review_ceiling_is_refused() {
        let dir = std::env::temp_dir();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            review_max_tokens: Some("plenty".to_string()),
            ..Flags::default()
        };
        merge(&flags, "");
    }

    /// A review/pr tuning is browse intent too: without a root it is the same
    /// half-configuration as the older knobs.
    #[test]
    #[should_panic(expected = "no browse.root")]
    fn review_tuning_without_a_root_is_refused() {
        let flags = Flags {
            pr_max_tokens: Some("900".to_string()),
            ..Flags::default()
        };
        merge(&flags, "");
    }

    /// A reserved root name (ikigai-repo's URN segment) is refused loud.
    #[test]
    #[should_panic(expected = "reserved")]
    fn reserved_root_names_are_refused() {
        let dir = std::env::temp_dir().join("status");
        std::fs::create_dir_all(&dir).unwrap();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            ..Flags::default()
        };
        merge(&flags, "");
    }

    /// `browse.allow_model` repeats like `browse.root` and spells providers
    /// like the tier keys: an id becomes `urn:llm:{id}:ask`, `ask` is the
    /// facade, a full IRI passes through. Unset it is EMPTY — the tiers alone,
    /// which is what this server offered before the key existed.
    #[test]
    fn allow_models_read_from_the_file() {
        let dir = std::env::temp_dir().join("ikigai-dev-test-root");
        std::fs::create_dir_all(&dir).unwrap();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            ..Flags::default()
        };
        assert!(merge(&flags, "")
            .browse
            .expect("browse configured")
            .allow_models
            .is_empty());

        let text = "browse.allow_model = \"big\"\n\
                    browse.allow_model = \"ask\"\n\
                    browse.allow_model = \"urn:llm:mlx:ask\"\n";
        let browse = merge(&flags, text).browse.expect("browse configured");
        assert_eq!(
            browse.allow_models,
            ["urn:llm:big:ask", "urn:llm:ask", "urn:llm:mlx:ask"]
        );
    }

    /// The flag REPLACES the file's lines rather than merging with them — the
    /// `browse.root` rule, for the same reason: an allowlist that could only
    /// grow would give the command line no way to take a backend back off.
    #[test]
    fn allow_model_flags_replace_the_file_lines() {
        let dir = std::env::temp_dir().join("ikigai-dev-test-root");
        std::fs::create_dir_all(&dir).unwrap();
        let flags = Flags {
            roots: vec![dir.to_string_lossy().into_owned()],
            allow_models: vec!["mlx".to_string()],
            ..Flags::default()
        };
        let text = "browse.allow_model = \"big\"\nbrowse.allow_model = \"ollama\"\n";
        let browse = merge(&flags, text).browse.expect("browse configured");
        assert_eq!(browse.allow_models, ["urn:llm:mlx:ask"]);
    }

    /// An allowlist with no root is browse intent like every other knob: the
    /// same loud half-configuration rather than a silently browse-less server.
    #[test]
    #[should_panic(expected = "no browse.root")]
    fn allow_model_without_a_root_is_refused() {
        let flags = Flags {
            allow_models: vec!["big".to_string()],
            ..Flags::default()
        };
        merge(&flags, "");
    }

    /// The half-configuration message names the file's dangling keys.
    #[test]
    fn browse_keys_surface_for_the_error() {
        let text = "browse.store = \"/x\"\n# browse.root = commented\nbrowse.dir_model = a\n";
        assert_eq!(browse_keys(text), vec!["browse.store", "browse.dir_model"]);
    }
}
