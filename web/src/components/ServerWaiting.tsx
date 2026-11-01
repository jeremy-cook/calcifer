// The whole window while the app can't load because the server hasn't answered:
// the logo pulsing in the hearth rings. Stateless: the caller decides when it shows.
export function ServerWaiting() {
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex h-screen flex-col items-center justify-center gap-1 text-center"
    >
      <div className="hearth-rings mb-2 flex size-24 items-center justify-center">
        <div className="hearth-logo size-7.5 animate-pulse" aria-hidden />
      </div>
      <h1 className="font-heading text-2xl font-semibold text-heading">Waiting for the server</h1>
      <p className="text-sm text-muted-foreground">Calcifer can't reach it yet. Retrying…</p>
    </div>
  )
}
