# calcifer (frontend)

The React + Vite UI for Calcifer. The repository [README](../README.md) covers
the whole system, how to run it, and the conventions; component style rules are in
[`CLAUDE.md`](../CLAUDE.md).

```bash
pnpm install --frozen-lockfile   # Node per ../.nvmrc; a pnpm workspace rooted one level up
pnpm dev                         # Vite on :5173, proxying /api to the backend
pnpm test                        # vitest
pnpm lint                        # eslint
pnpm build                       # tsc -b && vite build, into dist/
```

The API types come from the workspace package `calcifer` (`../proto/ts`), imported as
`calcifer/v1/entities_pb`.

`src/model/` holds the server-fed replica and the hooks components read it through;
`src/editors/tiptap/` is the rich-text editor; `src/components/ui/` is shadcn.
