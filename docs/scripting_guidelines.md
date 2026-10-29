# Coding Guidelines

Conventions to keep projects consistent. This is the default; a project may
override any rule in its own `CLAUDE.md` / README, and that override wins.

## General principles

- Prefer clarity over cleverness. Code is read far more than it is written.
- Match the style of the surrounding code when editing an existing project.
- Keep functions small and single-purpose. Extract when a block needs a comment
  to explain what it does.
- No dead code, no commented-out blocks left "just in case" — that's what git is
  for.
- Fail loudly: raise/throw on unexpected state rather than silently continuing.
- Don't fork a script/module for a variant. If a second tool differs from the first
  only in what it preserves or filters, make it a thin wrapper that delegates to
  the first (or extract the shared part) — a copied-and-trimmed fork silently rots.
- Never drop input without a trace: content a parser can't classify must land
  somewhere visible (a flag, a note field, an error), not vanish.

## Naming

| Kind                     | Convention              | Example                     |
|--------------------------|-------------------------|-----------------------------|
| Variables, functions     | `snake_case` (Py) / `camelCase` (JS/TS) | `user_count` / `userCount` |
| Constants                | `UPPER_SNAKE_CASE`      | `MAX_RETRIES`               |
| Classes, types, enums    | `PascalCase`            | `HttpClient`                |
| Files/modules            | `kebab-case` (JS/TS) / `snake_case` (Py) | `date-utils.ts` / `date_utils.py` |
| Booleans                 | `is_` / `has_` / `should_` prefix | `is_active`, `has_access` |
| Collections              | plural nouns            | `users`, `pending_jobs`     |
| Private members          | leading `_`             | `_cache`                    |

- Names describe intent, not type: `users` not `user_list`.
- Avoid abbreviations except well-known ones (`id`, `url`, `http`, `db`).
- Units in the name when ambiguous: `timeout_ms`, `file_size_bytes`.

## Formatting

- Use the language's standard auto-formatter; commit its config, never hand-format.
  - Python: `black` + `ruff`
  - JS/TS: `prettier` + `eslint`
  - Go: `gofmt`
  - Rust: `rustfmt`
  - GDScript: `gdformat` + `gdlint` (gdtoolkit; commit a `gdlintrc`)
- Indentation: 4 spaces (Python), 2 spaces (JS/TS/JSON/YAML), tabs (Go, GDScript).
- Max line length: 100 (soft), enforced by the formatter where possible.
- One statement per line. Trailing newline at end of file.

## Types & data

- Type-annotate public function signatures and module boundaries. Internal
  locals can rely on inference.
- Dates/times: store and pass UTC ISO 8601 (`2026-08-29T14:30:00Z`). Convert to
  local only at display time.
- Money: integer minor units (cents) or a decimal type — never float.
- IDs: strings unless there's a reason for integers.
- Prefer immutable data; copy rather than mutate shared structures.
- Normalising whitespace in text that may contain non-ASCII spaces (e.g.
  Japanese U+3000): use an explicit ASCII class — `re.sub(r"[ \t\r\n\f\v]+", " ", s)` —
  never Python's `str.split()`/`" ".join(s.split())`, which also collapses
  U+3000 and other Unicode spaces and will silently corrupt names.

## Errors & logging

- Catch narrow exceptions, not bare `except` / `catch (e)`.
- Error messages state what failed and the relevant value:
  `f"could not open config file: {path}"`.
- Log levels: `ERROR` (needs attention), `WARN` (recoverable oddity),
  `INFO` (lifecycle events), `DEBUG` (developer detail). No `print` in committed code.
- Never log secrets, tokens, or full PII.

## Comments & docs

- Comment the *why*, not the *what*.
- Every module/package gets a one-line purpose at the top.
- Public functions get a docstring: purpose, params, return, raises.
- Keep a `README.md` per project: what it is, how to set up, how to run, how to test.
- **When a value is centralized into config/a single source of truth specifically so
  it's a one-line change later (a folder-name scheme, a stage list, an API base
  path), don't undo that by spelling out its current literal value/shape in
  comments or examples elsewhere** — that recreates the exact "N places to
  update" problem the centralization was for, one level removed into prose. Name
  the *concept* instead ("the class's staged file", "the module's tagged output")
  and point at the config/helper that defines the real shape, rather than writing
  today's path/value as if it were fixed. A concrete example in a usage
  string is fine for a value that's genuinely stable (a class id, a CLI flag
  name); it's specifically the config-derived part that shouldn't be hardcoded.

## Project structure

- Standard layout per ecosystem (`src/`, `tests/`, `pyproject.toml` /
  `package.json` at root).
- Every project has a `CLAUDE.md` based on
  `~/projects/PROJECT_CLAUDE_TEMPLATE.md`, documenting setup/run/test commands.
- Secrets via environment variables; provide a `.env.example`. Never commit `.env`.
- Never hard-code settings that depend on the local environment into scripts:
  local folder/file paths, websites/URLs to access, user names, GitHub
  repository names, hostnames, and the like. Put them in a per-project config
  file (e.g. `config.toml` / `config.yaml` at the project root) and have scripts
  read from it. Commit a `config.example.*` with placeholder values and
  git-ignore the real config if it holds machine- or user-specific values.
  Secrets (tokens, passwords, keys) never go in the config file — they stay in
  environment variables per the rule above.
- Pin dependency versions (lockfile committed).
- `.gitignore` from the start (the global `~/.gitignore_global` already covers
  OS/editor cruft and common secret files).
- Pin the runtime version per project (`.mise.toml` / `.tool-versions`).
- Web pages: keep CSS and JS in their own files under `css/` and `js/`
  subfolders (or the framework's `static/` equivalent), linked from the HTML.
  No inline `<style>` or `<script>` blocks with real content — this holds even
  for quick local test pages and single-page tools. The HTML stays markup; each
  language lives in files its tooling (formatter, linter, syntax highlighting)
  can see.
- Scratch / throwaway files (debug dumps, one-off scripts, downloaded samples,
  intermediate test artifacts) go in a `tmp/` folder at the project root —
  `mkdir -p tmp` on demand. Never write scratch into real output/data
  directories, and don't leave it loose in the project root. **`tmp/` is
  cleared at the end of every run** — a process that writes there cleans up
  after itself (success or failure); nothing may depend on its contents.
- Runtime logs go in a `log/` folder at the project root (`mkdir -p log`), one
  file per component — not `tmp/`, not the repo root. Logs persist across runs
  (append or rotate); they're for reading after the fact.
- Both `tmp/` and `log/` are git-ignored globally (`~/.gitignore_global`).

## Scripts (any language): `--help`

- **Every** runnable script, in any language (Python, Node `.mjs`/`.js`, shell,
  and so on, including one-off and `tmp/` tools), must accept `--help` and `-h`.
  These two spellings are the standard; don't invent `help`, `--usage` or `/?`
  (Windows `.bat` scripts may add `/?`, PowerShell scripts get `Get-Help` through
  comment-based help, but `-h`/`--help` still work).
- Temporary / one-off scripts only need the short summary (item 1 below) and may
  skip items 2 and 3, but `--help` must still not execute the script.
- `--help` must **not execute the script**: no side effects, no file/network/db
  access, no heavy work. Parse arguments first and exit before any real logic;
  keep expensive imports and setup below the parse step or inside `main()`. Guard
  the entry point (`if __name__ == "__main__":`, or a `main()` that runs only after
  parsing).
- Help goes to **stdout** and the script exits **0**. Errors go to stderr.
- The help output must contain:
  1. a short summary of what the script does (`description=` from the module
     docstring is fine);
  2. every argument and option, with type, default, and meaning;
  3. the expected input data (files, formats, stdin, required env vars) and what
     it outputs or writes.
- Scripts with no arguments still handle `--help` (Python: still an
  `ArgumentParser`).
- How, per language:
  - **Python**: `argparse` (`-h`/`--help` and unknown-option errors come with it);
    use `argparse.RawDescriptionHelpFormatter` plus an `epilog=` for input-format
    notes and usage examples.
  - **Node**: `util.parseArgs` (built in, `strict: true` rejects unknown options)
    or `commander`/`yargs`. Keep the usage text in one constant, printed for
    `--help` and after an option error.
  - **Shell**: a `case` on `-h|--help` that prints a `usage()` function, and a
    final `*)` branch for unknown options.
- The **script's own header comment is not a substitute** for `--help`: it may
  hold the longer notes, but the same summary must be printable.

## Command-line scripts (any language): unknown options fail

- A script **must fail on an option it doesn't know**: print an error that names
  the option, point to `--help`, exit with status **2** (usage error; 1 is for
  failures while running), and do it **before any real work** (no partial run,
  no output written). Never ignore an unrecognised flag.
- Why: a silently ignored flag produces a plausible but wrong result. A
  `--edition 2e` run that ignored the flag tagged the 1e data and looked fine; a
  misspelt `--dry-run` deletes for real.
- `argparse` already does this. Don't defeat it with `parse_known_args()` unless
  the leftover arguments are handed on to something that validates them.
- Hand-rolled parsers (Node `process.argv`, shell `case`, custom loops) need an
  explicit list of known options, checked right after parsing. A generic
  `--key value` loop that stores whatever it sees is not enough.
- The same goes for missing or malformed values (`--only` with nothing after it)
  and for stray positional arguments the script doesn't take.
- Every option a script accepts appears in its `--help` output, and the check for
  unknown options and the help text share one source, so they can't drift apart.

## Git

- Branch names: `type/short-description` (`feat/user-auth`, `fix/null-crash`).
- Commit messages: imperative mood, present tense — `Add retry logic to uploader`.
- Small, focused commits. Don't mix refactor + behavior change.
- Don't commit generated files, secrets, or editor cruft.

## Testing

- New behavior ships with tests. Bug fixes ship with a regression test.
- Test names describe the scenario: `test_returns_empty_list_when_no_matches`.
- Arrange–Act–Assert structure. One logical assertion per test where practical.
- Keep tests deterministic: no real network, no `sleep`, seed randomness.
- Any test that mutates process-global state (a singleton registry, a static
  cache) must reset it in `setUp`/`before_each` so it can't leak into whatever
  runs next — via one shared reset helper, not ad-hoc per file. A test that only
  *reads* that state should not have to know a prior test clobbered it.

<!-- Amend as conventions evolve. Record the reasoning in KNOWLEDGE_BASE.md. -->
