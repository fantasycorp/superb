# Deploying the `superb` crate

How to release a version of `superb` to crates.io. Publishing is permanent:
a version number can be yanked but never deleted or reused, so run the
dry run and read it before the real publish.

## One-time setup

1. Create an account at <https://crates.io> and sign in with GitHub.
2. Create an API token at <https://crates.io/me> with the `publish-new` and
   `publish-update` scopes.
3. Log in locally. The token is stored in `~/.cargo/credentials.toml`; never
   commit it.

```bash
cargo login
```

## Each release

Work from a clean checkout of `main`.

### 1. Check the tree

```bash
git switch main
git pull --ff-only origin main
git status --short        # must print nothing
```

### 2. Run every check

```bash
cargo test
cargo test --doc --features hardware,files
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features hardware -- -D warnings
cargo fmt --all -- --check
cargo doc --no-deps
```

### 3. Set the version

Pick the version by [SemVer](https://semver.org/). Then:

- Set `version` in [Cargo.toml](Cargo.toml), for example `version = "0.1.0"`.
- In [CHANGELOG.md](CHANGELOG.md), rename `## [Unreleased]` to
  `## [0.1.0] - YYYY-MM-DD`, delete the "Nothing has been tagged yet" line,
  and add a fresh empty `## [Unreleased]` above it.

### 4. Commit and push

```bash
git commit -am "release: superb 0.1.0"
git push origin main
```

### 5. Check the package

`cargo publish --dry-run` builds the exact tarball crates.io will receive.
Read the file list before publishing.

```bash
cargo package --list
cargo publish --dry-run
```

The list should contain `src/`, `examples/`, `tests/`, `README.md`,
`CHANGELOG.md`, the licence files and `Cargo.toml`. It should not contain
`www/`, `docs/`, `scripts/` or `.github/`.

### 6. Publish

```bash
cargo publish
```

The crate appears at <https://crates.io/crates/superb> within a minute or so.

### 7. Tag the release

```bash
git tag -a v0.1.0 -m "superb 0.1.0"
git push origin v0.1.0
```

### 8. Check that it installs

From any directory outside this repo:

```bash
cargo new superb-check && cd superb-check
cargo add superb
cargo check
```

### 9. Update the website

In [www/components/Hero.tsx](www/components/Hero.tsx), change `INSTALL` to
`cargo add superb`, then redeploy `www/`.

## Mistakes

- **Broken release:** `cargo yank --version 0.1.0 superb` stops new projects
  from picking it up. Existing lockfiles still build. Then publish `0.1.1`.
  Undo a yank with `cargo yank --undo --version 0.1.0 superb`.
- **Wrong files in the tarball:** fix `exclude` in `Cargo.toml`, bump the
  version, and publish again. A published version cannot be replaced.
- **Leaked token:** revoke it at <https://crates.io/me> and run `cargo login`
  again.
