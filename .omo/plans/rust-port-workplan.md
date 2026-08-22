# Rust Port Workplan

## Must have

- Rust `jikji` is the shipped default entry point.
- CLI, parser, index, search, media engine, benchmark, and public adapters have Rust paths.
- Python remains reference/parity-only and is not executed by the Rust CLI.

## Must NOT have

- No Python subprocess in the Rust default path.
- No fabricated benchmark success for unsupported input.
- No destructive source-file organization.

## Final verification wave

- Run `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, release build, and Python parity tests.
- Run the reproducible central database benchmark and record raw timings.
- Verify both GitHub remotes match the local main commit.
