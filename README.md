# jev-sift

[English](README.md) | [日本語](README.ja.md)

Search source code **by meaning**, not keywords: "where do we swallow errors?", "code that retries". Files are cut into function-sized blocks, and for each block the probability that it is what you are looking for is asked of [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), the System One model from [TypeSafe AI](https://typesafe.ai/), via [polidog/jev](https://github.com/polidog/jev).

## What it is good at

Measured against Claude Code (Sonnet):

- **To find one place, asking Claude Code is more reliable.** On [cli/cli](https://github.com/cli/cli) (1,000 Go files), Claude Code answered all 7 "where does it do X?" questions without jev-sift. Making it use jev-sift did not change the answers and cost 16% more. jev-sift helps when a person wants a lead in 1–3 seconds for a few cents.
- **Sifting out everything that matches from a large set is where it shines.** On [banking77](https://github.com/PolyAI-LDN/task-specific-datasets) (10,003 customer queries to a bank), each tool was asked to list every query matching three questions.

  | | F1 | Price per question | Time per question |
  | --- | --- | --- | --- |
  | jev-sift (threshold 0.5) | 0.58–0.67 | ~$0.09 | 2–4 s |
  | jev-sift (threshold tuned per question) | 0.61–0.80 | ~$0.09 | 2–4 s |
  | Claude Code | 0.71–0.83 | $1.1–3.9 | 3–8 min |

  Claude Code's best question had five subagents read every row, for $3.9. The gap widens with the number of rows. Raise `-n` when you want everything.

## Installation

Download a binary for Linux (x86_64 / aarch64, static), macOS (Intel / Apple Silicon) or Windows from [Releases](https://github.com/polidog/jev-sift/releases/latest), extract it, and put `jev-sift` on your `PATH`.

Or build it with Cargo:

```bash
cargo install --git https://github.com/polidog/jev-sift
```

## Usage

```bash
export TYPESAFE_API_KEY=...
jev-sift "code that sends HTTP requests" ~/src/jev/src -n 3
# files and directories can be mixed
jev-sift "code that sends HTTP requests" src/http.rs src/provider
```

```
0.97  /home/you/src/jev/src/http.rs:9  pub fn post<T: DeserializeOwned>(
0.92  /home/you/src/jev/src/provider/vercel.rs:8  pub struct Vercel;
0.91  /home/you/src/jev/src/provider/typesafe.rs:6  pub struct TypeSafe;
```

Each line is the probability, `path:line`, and the block's first line. Like grep, it exits 1 when nothing reaches the threshold.

Pipe a file list to narrow the candidates first (and respect `.gitignore`):

```bash
rg -l "fn " | jev-sift "code that sends HTTP requests"
```

Price a run before sending anything:

```bash
jev-sift "where errors are swallowed" src --dry-run
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

Install a skill that teaches Claude Code or Codex when and how to call `jev-sift`:

```bash
jev-sift --install-skill claude   # ~/.claude/skills/jev-sift/SKILL.md
jev-sift --install-skill codex    # ~/.codex/skills/jev-sift/SKILL.md
```

`CLAUDE_CONFIG_DIR` / `CODEX_HOME` are respected. Re-running overwrites the file with the version bundled in the binary.

In the cli/cli measurement above, Claude Code never called jev-sift on its own with the skill installed (0 of 7), because rg was enough for those questions.

## How it works

- Walks each directory in `PATH`, skipping hidden entries and `target` / `node_modules` / `vendor`. Binary and non-UTF-8 files are ignored. `.gitignore` is not read.
- Files over 1 MiB are skipped.
- Each file is cut at non-indented lines that follow a blank line (roughly: top-level items). Blocks shorter than 3 lines are merged into the next; blocks over 40 lines are cut at the next blank line. Methods inside `impl` / `class` are not split individually.
- Each block is asked as a yes/no question (`noul`): is the code at these lines what the user is looking for? The answer is a probability.
- The whole file, with line numbers, goes into the request's state, and the questions point at blocks by line range. The model sees the surrounding code, and a file is sent once however many blocks it has. Blocks from different files never share a request. A file over 64 KB is split across requests, and a request the server calls too big (`max_tokens_exceeded`) is split in half and resent.
- Up to 16 requests run in parallel. 429 and 5xx responses are retried with backoff (1, 2, 4, 8, 16 s).
- The request shape follows what [mizchi/jev-lint](https://github.com/mizchi/jev-lint) measured about Jev. The price in `--dry-run` uses its measured rate (about $0.042 per million input tokens) and is an estimate, not a quote.

Repositories this project drew on are listed in [references.json](references.json).

## License

[MIT](LICENSE)
