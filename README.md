# cm

`cm` generates Git commit messages from staged changes using a local MLX model. It produces a Conventional Commits header and one bullet for every changed file.

The Rust binary communicates directly with an OpenAI-compatible MLX server. It does not require `git`, `curl`, or `jq` as external commands.

## Features

- Analyzes only changes added to the Git index.
- Supports repositories with no commits yet.
- Generates Conventional Commits 1.0.0 messages.
- Covers every changed file, including configuration, build, and documentation files.
- Splits large diffs into sequential requests and merges the partial analyses.
- Validates file coverage after chunked processing and attempts one correction when necessary.
- Bounds stored patch and summary text with a RAM budget and temporary disk spill.
- Supports custom models, server addresses, chunk sizes, memory limits, and system prompts through environment variables.

## Requirements

- macOS capable of running MLX.
- A Rust toolchain with Cargo.
- Homebrew and `uv` for the MLX setup below.
- A Git repository with staged changes.

## Install MLX

```zsh
brew install uv

uv venv ~/.venvs/mlx
source ~/.venvs/mlx/bin/activate

uv pip install mlx-lm
```

## Start the model

```zsh
source ~/.venvs/mlx/bin/activate

mlx_lm.server \
  --model mlx-community/Qwen2.5-Coder-7B-Instruct-4bit \
  --host 127.0.0.1 \
  --port 8080
```

The first launch may download the model. Keep the server running while using `cm`.

## Build

For a standard release build:

```bash
cargo build --release --locked
```

The binary is written to `target/release/cm`.

For additional optimizations on the current machine:

```bash
make release
```

This enables full link-time optimization, one code-generation unit, symbol stripping, `panic=abort`, and `target-cpu=native`. The resulting binary is optimized for the current processor and may not run on older or different processors.

## Usage

Stage the changes you want to include:

```bash
git add src/main.rs Cargo.toml
```

Run the release binary from the repository:

```bash
./target/release/cm
```

During development, you can use Cargo instead:

```bash
cargo run --quiet
```

The command prints only the generated commit message. For example:

```text
feat: add user authentication

- src/auth.rs: add credential validation
- src/main.rs: initialize the authentication flow
```

`cm` does not stage files or create commits. Errors are written to stderr and result in a nonzero exit status.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `MLX_MODEL` | `mlx-community/Qwen2.5-Coder-7B-Instruct-4bit` | Model name sent to the server |
| `MLX_HOST` | `http://127.0.0.1:8080` | Base URL of the MLX server |
| `CM_MAX_DIFF_BYTES` | `8000` | Maximum diff or partial-analysis data per chunk/merge request |
| `CM_MAX_MEMORY_BYTES` | `67108864` | RAM budget for stored patch and partial-analysis text |
| `CM_CACHE_DIR` | `~/Library/Caches/cm` on macOS | Root for the current run's temporary spill directory |
| `CM_SYSTEM_PROMPT` | Built-in Conventional Commits prompt | Custom system prompt for the final response |

Override settings for one invocation:

```bash
MLX_MODEL="mlx-community/another-model" \
MLX_HOST="http://127.0.0.1:8080" \
CM_MAX_DIFF_BYTES=16000 \
./target/release/cm
```

To replace the system prompt, including with multiline text:

```bash
export CM_SYSTEM_PROMPT='You generate concise Git commit messages.
Return one Conventional Commits message with one bullet per changed file.'

./target/release/cm
```

When `CM_SYSTEM_PROMPT` is set, it replaces the default prompt for the final response. Internal prompts used to analyze and consolidate chunks remain unchanged.

## Large diffs and memory

A diff that fits within `CM_MAX_DIFF_BYTES` is analyzed in one request. A larger diff is processed as follows:

1. Split the patch at file boundaries. Split an individual file further when its patch exceeds the limit, preserving UTF-8 boundaries.
2. Send each chunk to MLX sequentially instead of retaining a vector of all chunks.
3. Store partial analyses as RAM/disk text references and consolidate them in bounded merge groups, using additional reduction levels if necessary.
4. Generate one final message using the complete changed-file manifest.
5. Validate that chunked output contains exactly one bullet per file and attempt one repair if needed.

The limit applies to each chunk or merge group, not to the total staged diff. To use a single request for a larger diff, increase the limit so that the complete patch fits:

```bash
CM_MAX_DIFF_BYTES=100000 ./target/release/cm
```

`CM_MAX_MEMORY_BYTES` is specifically a budget for program-managed text accumulators: stored patches and partial analyses. It is not a hard RSS limit. File metadata, one active HTTP/merge/final prompt buffer, `git2`, `reqwest`, and other process memory are outside that accounting. A required active prompt that cannot fit the configured budget causes an explicit error asking for a larger budget.

When adding text would exceed the budget, existing RAM blocks are written as individual UTF-8 `.txt` files in a unique private `spill-*` directory under `CM_CACHE_DIR`. A block larger than the entire budget goes directly to disk and is later read in bounded ranges. These files are temporary spill for one invocation, not a persistent response cache. The directory is removed automatically on normal completion and ordinary errors; an abrupt kill or crash can leave it behind.

For example, to force aggressive spilling during a local experiment:

```bash
CM_CACHE_DIR="$HOME/Library/Caches/cm" \
CM_MAX_MEMORY_BYTES=1048576 \
CM_MAX_DIFF_BYTES=2000 \
./target/release/cm
```

The MLX model cache managed by `mlx_lm.server` is separate from this client-side spill cache.

A model can still omit details or produce an imperfect message, so review the result before committing.
