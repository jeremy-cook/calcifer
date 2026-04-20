# Phase 1 — App Layout Shell

Part of `app-plan.md`. This file is the execution-level detail for Phase 1.

## Goal

Introduce the application chrome: a three-part shell (top bar, collapsible left sidebar, main content area) wired behind a router. The editor continues to render inside the main area unchanged.

## What exists today

- `src/main.tsx` → `<App />`
- `src/App.tsx` → `<Home />`
- `src/pages/Home.tsx` → `<TiptapEditor />`
- Path alias `~/` maps to `src/`
- shadcn already installed with `button`, `popover`, `tabs`, `toggle`, `toggle-group`, `separator`, `select`, `calendar`

No router, no layout chrome, no sidebar.

## Deps to add

- `@tanstack/react-router` — routing
- shadcn `resizable` (uses `react-resizable-panels` under the hood) — draggable sidebar divider

Routing will be **code-based** for now (no vite plugin, no generated `routeTree.gen.ts`).

**Modified:**
- `src/App.tsx` — replace `<Home />` with `<RouterProvider router={router} />`
- `src/pages/Home.tsx` — delete; its body moves into `src/routes/index.tsx`

**Untouched:** `src/editors/**`, `src/components/ui/**`, `src/lib/**`, `src/main.tsx`.

## State model

One custom hook owns both the sidebar open/closed state and the keyboard binding:

```ts
// src/hooks/useSidebarState.ts
interface SidebarState {
  collapsed: boolean
  width: number              // px, clamped to [240, 360]
}

const STORAGE_KEY = 'calcifer.sidebar'
const DEFAULT: SidebarState = { collapsed: false, width: 280 }

export function useSidebarState() {
  const [state, setState] = useState<SidebarState>(() => loadFromStorage())

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  }, [state])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
        e.preventDefault()
        setState(s => ({ ...s, collapsed: !s.collapsed }))
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return { ...state, toggle: () => setState(s => ({ ...s, collapsed: !s.collapsed })), setWidth: (width: number) => setState(s => ({ ...s, width })) }
}
```

localStorage structure: `{ "collapsed": false, "width": 280 }`.

## Implementation order

1. **Install deps**
   ```bash
   pnpm add @tanstack/react-router
   ```
   Add the shadcn resizable via the `shadcn` skill (it handles `components.json` + install).

2. **Create `useSidebarState` hook** — the code sketch above is the whole file. No visual wiring yet; easy to unit-test later.

3. **Create layouts:**
   - `AppShell.tsx` — takes `children` (the route outlet). Renders `<TopBar />`, then `<ResizablePanelGroup direction="horizontal">` containing `<ResizablePanel>Sidebar</ResizablePanel>` + `<ResizableHandle />` + `<ResizablePanel>main</ResizablePanel>`. When `collapsed`, hide the sidebar panel + handle entirely (not just 0-width) so keyboard focus doesn't get stuck there.
   - `Sidebar.tsx` — returns a placeholder `<div>Sidebar (Phase 2)</div>` inside a scrollable container.
   - `TopBar.tsx` — height ~40px, flex row: toggle button (icon), breadcrumb placeholder, spacer. Toggle calls `useSidebarState().toggle()`.

4. **Create routes (code-based):**
   ```ts
   // src/router.tsx
   import { createRouter, createRootRoute, createRoute, RouterProvider } from '@tanstack/react-router'

   const rootRoute = createRootRoute({ component: RootLayout })
   const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: IndexPage })
   const entityRoute = createRoute({ getParentRoute: () => rootRoute, path: '/e/$id', component: EntityPage })

   const routeTree = rootRoute.addChildren([indexRoute, entityRoute])
   export const router = createRouter({ routeTree })

   declare module '@tanstack/react-router' {
     interface Register { router: typeof router }
   }
   ```
   `RootLayout` renders `<AppShell><Outlet /></AppShell>`.
   `IndexPage` mounts `<TiptapEditor />` (unchanged from today).
   `EntityPage` reads `:id` via `useParams` and renders `<div>Entity {id} — Phase 3</div>` for now.

5. **Wire router in `App.tsx`:**
   ```tsx
   import { RouterProvider } from '@tanstack/react-router'
   import { router } from '~/router'

   export function App() {
     return <RouterProvider router={router} />
   }
   ```

6. **Delete `src/pages/Home.tsx`** — its one line moved into `IndexPage`.

## Styling notes

- Editor is currently full-viewport-width. After Phase 1 it's `calc(100vw - sidebarWidth)` when sidebar is open. Verify the existing `TiptapEditor.css` still looks right (drag handle gutter, toolbar positioning).
- Top bar border-bottom: `border-b` (tailwind).
- Sidebar border-right: `border-r`. Background: slightly muted (`bg-muted/40` or similar) to distinguish from main content.

## Component style conventions (from CLAUDE.md)

- Named interfaces for props, never inline types.
- Constants above `return` inside the function.
- Early returns over nested conditionals.
- Named render functions for multi-line conditionals.

## Decisions to make during implementation

- **Collapse animation:** none (instant) for v1; add 150ms transition only if the snap feels jarring.
- **Width persistence on drag:** persist on drag-end (pointerup), not on every pointermove — avoids localStorage thrash.
- **Resize direction constraints:** shadcn `ResizablePanel` has `minSize` / `maxSize` (percentage). Use those plus our hook to keep actual px in [240, 360].

## Verification

1. `pnpm dev`
2. Page loads at `/` — editor renders unchanged inside the main area, with sidebar visible on the left and top bar visible on top.
3. Click sidebar toggle in top bar → sidebar hides; click again → reappears.
4. Press `Cmd+\` (or `Ctrl+\` on non-Mac) → toggles sidebar.
5. Drag the divider → sidebar width changes; width stays within the 240–360px range.
6. Reload the page → sidebar collapsed/expanded state and width both persist.
7. Navigate to `/e/abc123` manually → stub entity page renders inside the shell; back to `/` → editor again. Sidebar state persists across route changes.
8. Confirm the existing editor still works: toolbar, drag handle alignment, mentions, slash commands.

## Done when

All seven verification steps pass and `pnpm build` succeeds without TypeScript errors.
