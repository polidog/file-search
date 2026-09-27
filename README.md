# file-search

[English](README.md) | [日本語](README.ja.md)

Search source code **by meaning**, not keywords: "where do we swallow errors?", "code that retries". Files are cut into function-sized blocks and each block is scored by [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), the System One model from [TypeSafe AI](https://typesafe.ai/), via [polidog/jev](https://github.com/polidog/jev).

## Installation

```bash
cargo install --git https://github.com/polidog/file-search
```

## Usage

```bash
export TYPESAFE_API_KEY=...
file-search "code that sends HTTP requests" ~/src/jev/src -n 3
# files and directories can be mixed
file-search "code that sends HTTP requests" src/http.rs src/provider
```

```
★★★★★ 3.7  /home/you/src/jev/src/http.rs:9  pub fn post<T: DeserializeOwned>(
★★★★★ 3.7  /home/you/src/jev/src/provider/vercel.rs:8  pub struct Vercel;
★★★★☆ 3.3  /home/you/src/jev/src/provider/cloudflare.rs:17  impl Provider for Cloudflare {
```

Pipe a file list to narrow the candidates first (and respect `.gitignore`):

```bash
rg -l "fn " | file-search "code that sends HTTP requests"
```

| Option | Default | |
| --- | --- | --- |
| `PATH...` | stdin, else `.` | Files or directories to search (several allowed). Omit them and pipe paths on stdin instead |
| `-n`, `--num` | `10` | Number of results to show |
| `-m`, `--max` | `200` | Maximum number of blocks to score (caps API usage) |
| `-p`, `--provider` | `typesafe` | `typesafe` / `cloudflare` / `vercel` (or `JEV_PROVIDER`) |

See [polidog/jev](https://github.com/polidog/jev#providers) for each provider's environment variables.

## Agent skill

Install a skill that teaches Claude Code or Codex when and how to call `file-search`:

```bash
file-search --install-skill claude   # ~/.claude/skills/file-search/SKILL.md
file-search --install-skill codex    # ~/.codex/skills/file-search/SKILL.md
```

`CLAUDE_CONFIG_DIR` / `CODEX_HOME` are respected. Re-running overwrites the file with the version bundled in the binary.

## How it works

- Walks each directory in `PATH`, skipping hidden entries and `target` / `node_modules` / `vendor`. Binary and non-UTF-8 files are ignored. `.gitignore` is not read.
- Files over 1 MiB are skipped.
- Each file is cut at non-indented lines that follow a blank line (roughly: top-level items). Blocks shorter than 3 lines are merged into the next; blocks over 40 lines are cut at the next blank line. Methods inside `impl` / `class` are not split individually.
- Each block is truncated to 1000 characters.
- Blocks are sent 20 per request, with up to 8 requests in flight.

## License

[MIT](LICENSE)
