// @vitest-environment jsdom
import type { ReactNode } from 'react'
import { describe, expect, it } from 'vitest'
import { renderHook } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { create } from '@bufbuild/protobuf'
import { EntitySchema, PropertyValueSchema } from 'calcifer/v1/entities_pb'
import { queryClient } from '~/model/api'
import { useDailyNoteDate } from '~/model/daily-notes'
import { applyWatchEvent } from '~/model/sync'
import { watch } from '~/model/testing'

function dailyNote(id: string, iso: string) {
  const date = create(PropertyValueSchema, { value: { case: 'date', value: iso } })
  return create(EntitySchema, { id, name: iso, structureType: 'DailyNote', properties: { date } })
}

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
)

describe('useDailyNoteDate', () => {
  const mine = dailyNote('a', '2026-10-18')
  const other = dailyNote('b', '2026-10-19')

  it('reads the note’s day', () => {
    applyWatchEvent(watch.snapshot(mine, other))
    const { result } = renderHook(() => useDailyNoteDate(mine), { wrapper })
    expect(result.current.iso).toBe('2026-10-18')
    expect(result.current.error).toBeNull()
  })

  it('refuses a day another daily note holds, not its own or a free one', () => {
    applyWatchEvent(watch.snapshot(mine, other))
    const { result } = renderHook(() => useDailyNoteDate(mine), { wrapper })
    expect(result.current.validate('2026-10-19')).toBe('A daily note already exists for that day.')
    expect(result.current.validate('2026-10-18')).toBeNull()
    expect(result.current.validate('2026-10-20')).toBeNull()
  })
})
