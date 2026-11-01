import { useConnectionState } from '~/model/store'

// A pill in the top bar while the Watch stream is down after a failed attempt, so
// the page doesn't shift. `connecting` shows nothing, so a normal load doesn't flash it.
export function ConnectionPill() {
  const connection = useConnectionState()
  if (connection !== 'retrying') return null

  return (
    <div
      role="status"
      aria-live="polite"
      title="Can't reach the server. Retrying…"
      className="flex h-6.5 shrink-0 items-center gap-1.5 rounded-full border border-destructive/45 bg-destructive/15 px-2.5 text-xs font-semibold text-destructive"
    >
      <span className="size-2 animate-pulse rounded-full bg-destructive" aria-hidden />
      Offline · retrying
    </div>
  )
}
