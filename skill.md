---
name: file-search
description: Search source code by meaning rather than keywords with the file-search CLI. Use when looking for code by what it does or how it behaves ("where are errors swallowed", "code that retries", "functions that write to the database") and grep would need guessing the exact identifiers.
---

# file-search

`file-search` cuts files into function-sized blocks, scores each block against a natural-language query with Jev, and prints the best matches as `path:line`.

## When to use

- The question is about behavior or intent, not a known name: "where do we swallow errors?", "code that sends HTTP requests", "input that reaches SQL unescaped".
- grep / rg would need you to guess identifiers, or returns too many hits to read.

If you already know the identifier, use `rg` instead — it is faster and free.

## How to call

```bash
# a directory, files, or both
file-search "where errors are swallowed" src -n 5
file-search "code that sends HTTP requests" src/http.rs src/provider

# narrow the candidates with rg first (respects .gitignore, cheaper)
rg -l "fn |def |function " | file-search "code that retries on failure"
```

Output, best first (score 0–4, stars 1–5):

```
★★★★★ 3.7  src/http.rs:9  pub fn post<T: DeserializeOwned>(
★★★★☆ 3.3  src/provider/cloudflare.rs:17  impl Provider for Cloudflare {
```

Then read the listed `path:line` to confirm. Scores are judgments, not proof.

## Notes

- Every run calls the Jev API; each block costs. Scope to a subdirectory or pipe from `rg -l` on large repos. `-m/--max` (default 1000) caps the number of blocks scored.
- Needs `TYPESAFE_API_KEY` (or `-p cloudflare` / `-p vercel` with their credentials; see https://github.com/polidog/jev#providers).
- Hidden entries and `target` / `node_modules` / `vendor` are skipped when walking directories.
