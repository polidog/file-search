---
name: jev-sift
description: Search source code by meaning rather than keywords with the jev-sift CLI. Use when looking for code by what it does or how it behaves ("where are errors swallowed", "code that retries", "functions that write to the database") and grep would need guessing the exact identifiers.
---

# jev-sift

`jev-sift` cuts files into function-sized blocks, asks Jev for the probability that each block is what a natural-language query describes, and prints the matches as `path:line`.

## When to use

- The question is about behavior or intent, not a known name: "where do we swallow errors?", "code that sends HTTP requests", "input that reaches SQL unescaped".
- grep / rg would need you to guess identifiers, or returns too many hits to read.

If you already know the identifier, use `rg` instead — it is faster and free.

## How to call

```bash
# a directory, files, or both
jev-sift "where errors are swallowed" src -n 5
jev-sift "code that sends HTTP requests" src/http.rs src/provider

# narrow the candidates with rg first (respects .gitignore, cheaper)
rg -l "fn |def |function " | jev-sift "code that retries on failure"
```

Output, best first: probability, `path:line`, the block's first line.

```
0.97  src/http.rs:9  pub fn post<T: DeserializeOwned>(
0.92  src/provider/vercel.rs:8  pub struct Vercel;
```

Only results at or above `-t/--threshold` (default 0.5) are shown. Like grep, it exits 1 when nothing matches -- a real "not found", not an error. Read the listed `path:line` to confirm; a probability is a judgment, not proof.

## Notes

- Every run calls the Jev API. `--dry-run` prints the request count, input tokens and an estimated price without sending anything; a run over ~1000 blocks is typically a few cents. Scope to a subdirectory or pipe from `rg -l` on large repos. `-m/--max` (default 1000) caps the number of blocks scored.
- Needs `TYPESAFE_API_KEY` (or `-p cloudflare` / `-p vercel` with their credentials; see https://github.com/polidog/jev#providers).
- Hidden entries and `target` / `node_modules` / `vendor` are skipped when walking directories.
