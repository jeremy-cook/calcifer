// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { Code, ConnectError } from '@connectrpc/connect'
import { QueryClientProvider } from '@tanstack/react-query'
import type { RichText } from 'calcifer/v1/entities_pb'
import { qk, queryClient, richTextClient } from '~/model/api'
import { replica } from '~/model/replica'
import {
  richTextRef,
  usePutRichText,
  useRichText,
  whenRichTextSaved,
  writeRichTextIfNewer,
  type RichTextState,
} from '~/model/richtext'
import { richText } from '~/model/testing'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {},
  richTextClient: { getRichText: vi.fn(), putRichText: vi.fn() },
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

describe('usePutRichText', () => {
  const put = vi.mocked(richTextClient.putRichText)
  const unavailable = () => new ConnectError('server down', Code.Unavailable)
  const landed = (doc: string, updatedAtMs: number) =>
    ({ richText: richText('a', 'content', doc, updatedAtMs) }) as never

  // Lets queued promise callbacks run, and fake timers advance by `ms`.
  const advance = (ms: number) =>
    act(async () => {
      await vi.advanceTimersByTimeAsync(ms)
    })

  const renderSaver = (options: Partial<Parameters<typeof usePutRichText>[1]> = {}) =>
    renderHook(() => usePutRichText(ref, { initialBase: undefined, ...options }), { wrapper })

  beforeEach(() => {
    vi.useFakeTimers()
    put.mockReset()
    replica.reset()
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('retries a failed save and saves it once the server answers', async () => {
    put.mockRejectedValueOnce(unavailable()).mockResolvedValueOnce(landed('a', 1_000))
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)
    expect(put).toHaveBeenCalledTimes(1)
    expect(result.current.unsaved).toBe(true)

    await advance(1_000)
    expect(put).toHaveBeenCalledTimes(2)
    expect(put.mock.calls[1][0]).toMatchObject({ doc: 'a' })
    expect(put.mock.calls[0][0]).toMatchObject({ doc: 'a' })
    expect(result.current.unsaved).toBe(false)
    expect(result.current.busy).toBe(false)
  })

  it('sends the newest doc on retry', async () => {
    put.mockRejectedValueOnce(unavailable()).mockResolvedValueOnce(landed('b', 1_000))
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)
    act(() => result.current.save('b'))
    await advance(1_000)

    expect(put).toHaveBeenCalledTimes(2)
    expect(put.mock.calls[1][0]).toMatchObject({ doc: 'b' })
    expect(result.current.unsaved).toBe(false)
  })

  it('backs off up to 30 seconds', async () => {
    put.mockRejectedValue(unavailable())
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)

    let calls = 1
    for (const seconds of [1, 2, 4, 8, 16, 30, 30]) {
      await advance(seconds * 1_000 - 1)
      expect(put).toHaveBeenCalledTimes(calls)
      await advance(1)
      calls += 1
      expect(put).toHaveBeenCalledTimes(calls)
    }

    // The server answers at last, so the loop ends.
    put.mockResolvedValueOnce(landed('a', 1_000))
    await advance(30_000)
    expect(result.current.unsaved).toBe(false)
    expect(result.current.busy).toBe(false)
  })

  it('retries at once when the connection goes live', async () => {
    put.mockRejectedValueOnce(unavailable()).mockResolvedValueOnce(landed('a', 1_000))
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)
    expect(put).toHaveBeenCalledTimes(1)

    await act(async () => replica.setConnection('retrying'))
    expect(put).toHaveBeenCalledTimes(1)
    await act(async () => replica.setConnection('live'))
    expect(put).toHaveBeenCalledTimes(2)
    expect(result.current.unsaved).toBe(false)
  })

  it('keeps backing off while the replica changes under an already live connection', async () => {
    replica.setConnection('live')
    put.mockRejectedValueOnce(unavailable()).mockResolvedValueOnce(landed('a', 1_000))
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)
    await act(async () => replica.applySnapshot([]))
    expect(put).toHaveBeenCalledTimes(1)

    await advance(1_000)
    expect(put).toHaveBeenCalledTimes(2)
  })

  it('drops a save that fails with InvalidArgument', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {})
    put.mockRejectedValue(new ConnectError('bad doc', Code.InvalidArgument))
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)
    await advance(60_000)

    expect(put).toHaveBeenCalledTimes(1)
    expect(error).toHaveBeenCalledWith('rich text save failed', expect.any(ConnectError))
    expect(result.current.unsaved).toBe(false)
    expect(result.current.busy).toBe(false)
  })

  it('keeps whenRichTextSaved waiting while it retries', async () => {
    put.mockRejectedValueOnce(unavailable()).mockResolvedValueOnce(landed('a', 1_000))
    const { result } = renderSaver()

    act(() => result.current.save('a'))
    await advance(0)
    let saved = false
    void whenRichTextSaved(ref).then(() => (saved = true))

    await advance(999)
    expect(saved).toBe(false)
    await advance(1)
    expect(saved).toBe(true)
  })

  it('a conflict during retry clears unsaved', async () => {
    put.mockRejectedValueOnce(unavailable()).mockRejectedValueOnce(new ConnectError('stale', Code.FailedPrecondition))
    serve(richText('a', 'content', 'latest', 5_000))
    const onConflict = vi.fn()
    const { result } = renderSaver({ onConflict })

    act(() => result.current.save('a'))
    await advance(0)
    expect(result.current.unsaved).toBe(true)

    await advance(1_000)
    expect(put).toHaveBeenCalledTimes(2)
    expect(onConflict).toHaveBeenCalledTimes(1)
    expect(result.current.unsaved).toBe(false)
    expect(result.current.busy).toBe(false)

    await advance(60_000)
    expect(put).toHaveBeenCalledTimes(2)
  })
})
