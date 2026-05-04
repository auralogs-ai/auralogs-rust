# Contributing to auralog-rust

This repo is the **Rust SDK** only. For issues with the Auralog service itself, head to [auralog.ai](https://auralog.ai) or [docs.auralog.ai](https://docs.auralog.ai).

## Reporting Bugs

Open a bug report and include:

- SDK version
- Rust version
- Runtime/framework, if relevant
- Minimal reproduction
- What you expected vs. what happened

## Development Setup

Requirements: Rust 1.86 or later.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation only
- `test:` — tests only
- `refactor:` — code change that neither fixes a bug nor adds a feature
- `build:` — build system, CI, dependencies
- `chore:` — other housekeeping

## Releases

Maintainers publish via GitHub Releases and crates.io. Trusted publishing should be used once crates.io is configured for this repository.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](./LICENSE).
