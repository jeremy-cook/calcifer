import { useEffect, useState } from 'react'

// The server is local, so most loads finish in a blink: a placeholder waits this
// long before it shows, so a quick load shows nothing at all.
const LOADING_DELAY_MS = 300

export function useShownAfterDelay(): boolean {
  const [shown, setShown] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => setShown(true), LOADING_DELAY_MS)
    return () => clearTimeout(timer)
  }, [])
  return shown
}
