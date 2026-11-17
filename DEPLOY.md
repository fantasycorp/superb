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

Work from a clean checkout of `main`. The repository is
<https://github.com/fantasycorp/superb>; `git remote -v` should show it as
`origin`.

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

Pick the version by [SemVer](https://semver.org/); `X.Y.Z` stands for it
below. The last release is the newest `## [x.y.z]` heading in
[CHANGELOG.md](CHANGELOG.md), and `git tag --list 'v*'` lists them all.
Before 1.0, a change that breaks the API raises the middle number. Then:

- Set `version` in [Cargo.toml](Cargo.toml): `version = "X.Y.Z"`.
- Set `HOST_VERSION` in
  [src/primitives/plugins/clap/mod.rs](src/primitives/plugins/clap/mod.rs)
  to the same number. It is what a CLAP plugin is told the host's version is.
- In [CHANGELOG.md](CHANGELOG.md), rename `## [Unreleased]` to
  `## [X.Y.Z] - YYYY-MM-DD` and add a fresh empty `## [Unreleased]` above it.
- Run `cargo check` so `Cargo.lock` takes the new version.

### 4. Commit and push

```bash
git commit -am "release: superb X.Y.Z"
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

Tag the release commit of step 4, the one on `main` that was published.
The tag is annotated, named `vX.Y.Z`, with the message `superb X.Y.Z`.

```bash
git tag -a vX.Y.Z -m "superb X.Y.Z"
git push origin vX.Y.Z
```

A pushed tag is not moved afterwards: people and tools that fetched it
keep the old one. If `main` is ever rewritten, the tag stays on the commit
that was published.

### 8. Check that it installs

From any directory outside this repo:

```bash
cargo new superb-check && cd superb-check
cargo add superb
cargo check
```

### 9. Update the website

The install line in [www/components/Hero.tsx](www/components/Hero.tsx) is
`cargo add superb` and names no version, so it needs no change. Redeploy
`www/` only if the page itself changed.

## Mistakes

- **Broken release:** `cargo yank --version X.Y.Z superb` stops new projects
  from picking it up. Existing lockfiles still build. Then publish the next
  patch version. Undo a yank with `cargo yank --undo --version X.Y.Z superb`.
- **Wrong files in the tarball:** fix `exclude` in `Cargo.toml`, bump the
  version, and publish again. A published version cannot be replaced.
- **Leaked token:** revoke it at <https://crates.io/me> and run `cargo login`
  again.
