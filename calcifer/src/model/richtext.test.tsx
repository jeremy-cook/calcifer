// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { RichText } from '@calcifer/proto/calcifer/v1/entities_pb'
import { qk, queryClient, richTextClient } from '~/model/api'
import { richTextRef, useRichText, writeRichTextIfNewer, type RichTextState } from '~/model/richtext'
import { richText } from '~/model/testing'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {},
  richTextClient: { getRichText: vi.fn() },
  structureClient: {},
}))

beforeEach(() => {
  queryClient.clear()
})

afterEach(() => {
  vi.clearAllMocks()
})

interface WrapperProps {
  children: React.ReactNode
}

function wrapper({ children }: WrapperProps) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

const ref = richTextRef('a', 'content')

function serve(rt: RichText) {
  vi.mocked(richTextClient.getRichText).mockResolvedValue({ richText: rt } as never)
}

const cached = () => queryClient.getQueryData<RichTextState>(qk.richtext('a', 'content'))

describe('useRichText', () => {
  it('keeps a newer event that lands while the fetch is in flight (I-49)', async () => {
    let resolve!: (value: { richText: RichText }) => void
    vi.mocked(richTextClient.getRichText).mockReturnValue(new Promise((r) => (resolve = r)) as never)
    const { result } = renderHook(() => useRichText(ref), { wrapper })

    act(() => writeRichTextIfNewer(richText('a', 'content', 'event', 2_000)))
    await act(async () => resolve({ richText: richText('a', 'content', 'stale', 1_000) }))

    await waitFor(() => expect(result.current.isFetching).toBe(false))
    expect(result.current.data?.doc).toBe('event')
  })

  it('replaces the cache with a newer fetch', async () => {
    writeRichTextIfNewer(richText('a', 'content', 'old', 1_000))
    serve(richText('a', 'content', 'new', 2_000))
    const { result } = renderHook(() => useRichText(ref), { wrapper })
    expect(result.current.data?.doc).toBe('old')

    await act(async () => void (await result.current.refetch()))
    await waitFor(() => expect(result.current.data?.doc).toBe('new'))
  })

  it('keeps the cached object when the fetch is equally new', async () => {
    writeRichTextIfNewer(richText('a', 'content', 'cached', 1_000))
    const before = cached()
    serve(richText('a', 'content', 'fetched', 1_000))
    const { result } = renderHook(() => useRichText(ref), { wrapper })

    await act(async () => void (await result.current.refetch()))
    expect(vi.mocked(richTextClient.getRichText)).toHaveBeenCalled()
    expect(cached()).toBe(before)
    expect(result.current.data).toBe(before)
  })
})
