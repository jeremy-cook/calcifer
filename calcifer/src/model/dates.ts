import { format, getISOWeek, parse } from 'date-fns'

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
