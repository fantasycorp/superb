# docs

Everything documentation lives here: the Markdown, and the site that
serves it.

The Markdown is the source. `quick-start.md` is the task index.
`flows.md` and `flows/<flow>.md` are the short flow pages;
`flows/<flow>.ref.md` is the generated reference (`scripts/reference.py`).
`primitives.md` and `primitives/<module>.md` are the engine contracts,
`ARCHITECTURE.md` the rules and `TERMINOLOGY.md` the words; the
repository's `README.md`, `examples/README.md` and `CONTRIBUTING.md`
complete the set. GitHub renders all of it as it is.

Every Rust example in these pages is a whole program, and
`cargo test --doc --features hardware,files` compiles them (`no_run` on a
fence means it needs a device or a file of yours).

`/llms.txt` is the index for an agent and `/llms-full.txt` is everything
in one file, in reading order: the task index, each flow's page with its
generated reference, the architecture, the terminology, the primitive
contracts, the remaining work, and one page per program under `examples/`
with its whole source (`lib/agent-pages.ts` is the order).

The site is [Fumadocs](https://fumadocs.dev) on Next.js. `scripts/sync-docs.mjs`
turns that Markdown into MDX under `content/docs/` with a sidebar, and
makes a page of every example's source;
`bun dev` and `bun run build` run it first, and `content/docs/` is not
committed. Edit the Markdown, not the generated pages.

```sh
cd docs
bun install
bun dev           # live at http://localhost:3000, rebuilds pages as you edit
bun run build     # a production build
bun start         # serve the production build
```

- `lib/shared.ts`: the site name and, once the repository has a public
  home, the GitHub link.
- `lib/source.ts`: the content source (Fumadocs MDX).
- `app/docs`: the documentation layout and pages; `app/(home)`: the
  landing page; `app/api/search`: search; `app/llms*.txt`: the docs as
  plain text for language models.
