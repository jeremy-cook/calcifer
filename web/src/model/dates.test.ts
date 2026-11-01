import { describe, expect, it } from 'vitest'
import { timestampFromMs } from '@bufbuild/protobuf/wkt'
import { compareTimestamps, formatTimeAgo, msUntilEndOfDay, shiftIso } from '~/model/dates'

describe('dates', () => {
  it('says how long ago across a clock change', () => {
    // 24 minutes either side of US Pacific's fall-back (09:00 UTC); right in any time zone.
    const edited = new Date(Date.UTC(2026, 10, 1, 8, 50))
    expect(formatTimeAgo(edited, Date.UTC(2026, 10, 1, 9, 14))).toBe('24 minutes ago')
  })

  it('shifts an ISO date across a month end', () => {
    expect(shiftIso('2026-01-31', 1)).toBe('2026-02-01')
  })

  it('counts down to the local midnight that ends the day', () => {
    expect(msUntilEndOfDay('2026-03-10', new Date(2026, 2, 10, 23, 0))).toBe(60 * 60 * 1000)
    expect(msUntilEndOfDay('2026-03-10', new Date(2026, 2, 11, 1, 0))).toBe(0)
  })

  it('orders timestamps, reading unset as the epoch', () => {
    expect(compareTimestamps(timestampFromMs(1_500), timestampFromMs(1_000))).toBeGreaterThan(0)
    expect(compareTimestamps(timestampFromMs(1_000), timestampFromMs(1_001))).toBeLessThan(0)
    expect(compareTimestamps(undefined, timestampFromMs(0))).toBe(0)
  })
})
