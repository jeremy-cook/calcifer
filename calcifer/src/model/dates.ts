import { useEffect, useState } from 'react'
import { format, getISOWeek, parse } from 'date-fns'
import { timestampMs, type Timestamp } from '@bufbuild/protobuf/wkt'

// A proto Timestamp in epoch milliseconds; an unset one reads as 0 (the epoch).
export function timestampMsOrZero(ts: Timestamp | undefined): number {
  return ts ? timestampMs(ts) : 0
}

export function todayIso(): string {
  return isoFromDate(new Date())
}

export function isoFromDate(d: Date): string {
  return format(d, 'yyyy-MM-dd')
}

export function dateFromIso(iso: string): Date {
  return parse(iso, 'yyyy-MM-dd', new Date())
}

export function formatLongDate(iso: string): string {
  return format(dateFromIso(iso), 'MMMM d, yyyy')
}

export function formatWeekday(iso: string): string {
  return format(dateFromIso(iso), 'EEEE')
}

export function weekNumber(iso: string): number {
  return getISOWeek(dateFromIso(iso))
}

export function shiftIso(iso: string, days: number): string {
  const d = dateFromIso(iso)
  d.setDate(d.getDate() + days)
  return isoFromDate(d)
}

// Milliseconds from `now` until the local midnight that ends `iso`, clamped at
// 0 (already past it). Built from the calendar date, so a 23h or 25h DST day
// still lands on local midnight.
export function msUntilEndOfDay(iso: string, now: Date = new Date()): number {
  return Math.max(0, dateFromIso(shiftIso(iso, 1)).getTime() - now.getTime())
}

// todayIso() for render-time use: re-renders at the next local midnight. Timers
// stall across sleep and don't fire early when the clock jumps forward, so the
// date is also re-checked whenever the page becomes visible or regains focus.
export function useToday(): string {
  const [today, setToday] = useState(todayIso)

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined

    function schedule() {
      clearTimeout(timer)
      timer = setTimeout(check, msUntilEndOfDay(today))
    }

    // A changed date re-runs this effect, which schedules the next midnight;
    // otherwise (woke early, or date unchanged) reschedule from the current clock.
    function check() {
      const next = todayIso()
      if (next !== today) setToday(next)
      else schedule()
    }

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') check()
    }

    schedule()
    document.addEventListener('visibilitychange', handleVisibilityChange)
    window.addEventListener('focus', check)
    return () => {
      clearTimeout(timer)
      document.removeEventListener('visibilitychange', handleVisibilityChange)
      window.removeEventListener('focus', check)
    }
  }, [today])

  return today
}
