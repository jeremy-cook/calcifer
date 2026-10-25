// A full-width strip for when the browser can't reach the server. Stateless: the
// caller decides when it shows. `destructive` is one colour in both themes, so
// white text on it reads in light and dark alike.
export function ServerUnreachableNotice() {
  return (
    <div role="status" aria-live="polite" className="border-b bg-destructive px-4 py-1.5 text-sm text-white">
      Can't reach the server. Retrying…
    </div>
  )
}
