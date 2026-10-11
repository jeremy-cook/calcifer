import { describe, expect, it } from 'vitest'
import { msUntilEndOfDay, shiftIso } from '~/model/dates'

describe('dates', () => {
  it('shifts an ISO date across a month end', () => {
    expect(shiftIso('2026-01-31', 1)).toBe('2026-02-01')
  })

  it('counts down to the local midnight that ends the day', () => {
    expect(msUntilEndOfDay('2026-03-10', new Date(2026, 2, 10, 23, 0))).toBe(60 * 60 * 1000)
    expect(msUntilEndOfDay('2026-03-10', new Date(2026, 2, 11, 1, 0))).toBe(0)
  })
})
