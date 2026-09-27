# file-search

Find files whose content matches what you're looking for **in meaning**, not just keywords. Each file is scored by [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), the System One model from [TypeSafe AI](https://typesafe.ai/), via [polidog/jev](https://github.com/polidog/jev).

## Installation

```bash
cargo install --git https://github.com/polidog/file-search
```

## Usage

```bash
export TYPESAFE_API_KEY=...
file-search "code that sends requests to the Jev API" ~/src/jev -n 5
```

```
★★★★★ 3.7  /home/you/src/jev/src/provider/typesafe.rs
★★★★★ 3.6  /home/you/src/jev/src/http.rs
★★★★☆ 3.3  /home/you/src/jev/src/provider/vercel.rs
★★★★☆ 3.3  /home/you/src/jev/src/provider/cloudflare.rs
★★★☆☆ 2.1  /home/you/src/jev/src/provider/mod.rs
```

| Option | Default | |
| --- | --- | --- |
| `DIR` | `.` | Directory to search |
| `-n`, `--num` | `10` | Number of results to show |
| `-m`, `--max-files` | `200` | Maximum number of files to score (caps API usage) |
| `-p`, `--provider` | `typesafe` | `typesafe` / `cloudflare` / `vercel` (or `JEV_PROVIDER`) |

See [polidog/jev](https://github.com/polidog/jev#providers) for each provider's environment variables.

## How it works

- Walks `DIR`, skipping hidden entries and `target` / `node_modules` / `vendor`. Binary and non-UTF-8 files are ignored. `.gitignore` is not read.
- Only the first 600 characters of each file are scored.
- Files are sent 20 per request, with up to 8 requests in flight.

## License

[MIT](LICENSE)
