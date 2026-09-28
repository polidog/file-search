---
name: jev-sift
description: Sift source code or CSV / Excel rows by meaning with the jev-sift CLI. Best at listing every row of a large table that matches a natural-language description ("complaints about delivery", "expenses that look personal") for cents in seconds, when reading the rows yourself would be too slow or too expensive. Also finds code by what it does when grep would need guessing identifiers.
---

# jev-sift

`jev-sift` cuts code into function-sized blocks and tables (CSV / TSV / Excel) into one block per row, asks Jev for the probability that each block is what a natural-language query describes, and prints the matches as `path:line`.

## When to use

- A CSV / Excel file has more rows than you can read, and the task is to find every row matching a description. Pass the file directly; set `-n` above the row count to get all matches, then read the `path:line` rows to confirm.
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
