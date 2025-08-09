# vecgrep

Find meaning, not just strings. `vecgrep` is a semantic grep powered by [MinishLab/model2vec-rs](https://github.com/MinishLab/model2vec-rs). It matches lines that mean the same thing, even if the exact words differ.

```bash
$ echo -e "server crashed with error 123\nuser logged in" | vecgrep "server shutdown error"
server crashed with error 123
```

## Install

One-line install (tries prebuilt, falls back to from-source):

```bash
curl -fsSL https://raw.githubusercontent.com/toppev/vecgrep/main/install.sh | bash
```

Ensure `~/.cargo/bin` or `~/.local/bin` is in your `PATH`.
Note: If no prebuilt binary is available for your platform, the script will ask for confirmation before installing Rust and building from source.

## Usage

Real-life example: noisy logs, same issue phrased differently

```bash
cat examples/logs.txt | vecgrep -A1 -B1 "database connection error"
```

Even though the logs use different phrasings like "database failed to connect" or "Database: connection refused", `vecgrep` finds the same failure pattern and shows helpful context.

Examples (reads from stdin):

- Basic search with defaults (`potion-base-8M`, threshold 0.6):

```bash
cat logs.txt | vecgrep "database connection error"
```

- With context like grep:

```bash
cat logs.txt | vecgrep -A5 -B5 "database connection error"
```

- Lower threshold (scores are shown by default):

```bash
cat logs.txt | vecgrep -t 0.5 "db failed to connect"
```

At the end, `vecgrep` prints a similarity distribution to stderr to help you pick a good `-t` value.
This distribution is computed over all lines, so you can pick thresholds by target match rate. For example, if you want roughly 1% of lines to match, start around the reported `p99`.

- Use a different model:

```bash
cat logs.txt | vecgrep -m minishlab/potion-multilingual-128M "query"
```

Or use VECGREP_MODEL env var to set the model.

- Streaming endless input (e.g., `tail -f` or `docker logs -f`):

```bash
tail -f myapp.log | vecgrep --stream -A2 -B2 "timeout"
```
  - Use `--stream` when the input never ends; it processes and prints incrementally. Without `--stream`, `vecgrep` waits for EOF before printing.

- Top-N matches by cosine similarity (disables threshold; not allowed with `--stream`):

```bash
cat logs.txt | vecgrep --top 3 "database connection error"
```

## Help

```bash
vecgrep -h
```

Shows all parameters, including:

- `-t, --threshold <FLOAT>`: similarity threshold (default 0.6)
- `-A <N>, -B <N>`: after/before context
- `-m, --model <MODEL>`: model ID (default `minishlab/potion-base-8M`, or use env var: `VECGREP_MODEL=minishlab/potion-retrieval-32M`)
- `--hide-scores`: hide per-line similarity scores (shown by default)
- `--batch-size <N>`: set encoding batch size (default 1024)
 - `--stream`: process endless streams (e.g., `tail -f`, `docker logs -f`) line-by-line with `-A/-B` context (no batching)
 - `--top <N>`: select top-N most similar lines (disables threshold, not available with `--stream`)

The tool prints percentiles on all lines (similarity distribution). For example, if you want to match only about 1 in 1000, use threshold shown for `p99.9`. This works more accurately with larger files.

## Development

- Build: `cargo build` (or `cargo build --release`)
- Lint/format: `cargo clippy` and `cargo fmt --all`
- Test smoke run: `cat examples/logs.txt | cargo run -- "database connection error"`
- Update expected output in CI: `cat examples/logs.txt | cargo run -- -A1 -B1 -t 0.6 "database connection error" > examples/expected_basic.txt`
- CI: see `.github/workflows/ci.yml` (build, clippy, fmt, E2E smoke)
- Releases: tag `vX.Y.Z` to build and upload binaries (see `.github/workflows/release.yml`)

## Notes

- Models: see the model list at [MinishLab model2vec base models](https://huggingface.co/collections/minishlab/model2vec-base-models-66fd9dd9b7c3b3c0f25ca90e).
  - Default model: `minishlab/potion-base-8M`. Change with `-m`.
  - These models run efficiently on CPU; thousands of lines per second on a single thread is typical, depending on model and hardware.
- Prints a similarity distribution summary to stderr after matches to help you tune `-t`.
  - Includes `p90/p95/p99/p99.9` on all lines, so you can map desired top-k% to thresholds quickly.
- Batching can be adjusted with `--batch-size`.
  - Example: `--batch-size 4096` for faster CPU throughput if you have memory headroom.
- Limited scope: this is not full GNU grep; it focuses on semantic matching with simple `-A/-B` context and thresholding. Feel free to contribute :)

## License

MIT
