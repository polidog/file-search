# file-search

[English](README.md) | [日本語](README.ja.md)

Search source code **by meaning**, not keywords: "where do we swallow errors?", "code that retries". Files are cut into function-sized blocks, and for each block the probability that it is what you are looking for is asked of [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), the System One model from [TypeSafe AI](https://typesafe.ai/), via [polidog/jev](https://github.com/polidog/jev).

## Installation

Download a binary for Linux (x86_64 / aarch64, static), macOS (Intel / Apple Silicon) or Windows from [Releases](https://github.com/polidog/file-search/releases/latest), extract it, and put `file-search` on your `PATH`.

Or build it with Cargo:

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
0.97  /home/you/src/jev/src/http.rs:9  pub fn post<T: DeserializeOwned>(
0.92  /home/you/src/jev/src/provider/vercel.rs:8  pub struct Vercel;
0.91  /home/you/src/jev/src/provider/typesafe.rs:6  pub struct TypeSafe;
```

Each line is the probability, `path:line`, and the block's first line. Like grep, it exits 1 when nothing reaches the threshold.

Pipe a file list to narrow the candidates first (and respect `.gitignore`):

```bash
rg -l "fn " | file-search "code that sends HTTP requests"
```

Price a run before sending anything:

```bash
file-search "where errors are swallowed" src --dry-run
# 39 files / 1007 blocks / 40 requests / ~502585 input tokens / ~$0.0211
```

| Option | Default | |
| --- | --- | --- |
| `PATH...` | stdin, else `.` | Files or directories to search (several allowed). Omit them and pipe paths on stdin instead |
| `-n`, `--num` | `10` | Number of results to show |
| `-t`, `--threshold` | `0.5` | Hide results below this probability |
| `-m`, `--max` | `1000` | Maximum number of blocks to score (caps API usage) |
| `--dry-run` | | Print the number of requests, input tokens and an estimated price, and send nothing |
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
- Each block is asked as a yes/no question (`noul`): is the code at these lines what the user is looking for? The answer is a probability.
- The whole file, with line numbers, goes into the request's state, and the questions point at blocks by line range. The model sees the surrounding code, and a file is sent once however many blocks it has. Blocks from different files never share a request. A file over 64 KB is split across requests.
- Up to 16 requests run in parallel. 429 and 5xx responses are retried with backoff (1, 2, 4, 8, 16 s).
- The request shape follows what [mizchi/jev-lint](https://github.com/mizchi/jev-lint) measured about Jev. The price in `--dry-run` uses its measured rate (about $0.042 per million input tokens) and is an estimate, not a quote.

## License

[MIT](LICENSE)
