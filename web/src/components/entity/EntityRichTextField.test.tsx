// @vitest-environment jsdom
import { useEffect, useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import { Code, ConnectError } from '@connectrpc/connect'
import { QueryClientProvider } from '@tanstack/react-query'
import { queryClient, richTextClient } from '~/model/api'
import { writeRichTextIfNewer } from '~/model/richtext'
import { richText } from '~/model/testing'
import { EntityRichTextField } from '~/components/entity/EntityRichTextField'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {},
  richTextClient: { getRichText: vi.fn(), putRichText: vi.fn() },
  structureClient: {},
}))

// A stand-in for the real editor that records what the field hands it and lets
// a test emit an edit, or throw the instance away and make a fresh one.
interface StubProps {
  doc?: string
  externalDoc?: string
  externalVersion?: number
  onUpdate?: (json: object, appliedVersion: number | undefined) => void
  notice?: React.ReactNode
}

const probe = vi.hoisted(() => ({
  props: null as StubProps | null,
  recreate: () => {},
  // The `doc` and `externalVersion` each instance was created with.
  mounted: [] as Array<{ doc?: string; externalVersion?: number }>,
}))

vi.mock('~/editors/tiptap/TiptapEditor', () => ({
  TiptapEditor: function TiptapEditorStub(props: StubProps) {
    const [instance, setInstance] = useState(0)
    probe.props = props
    useEffect(() => {
      probe.recreate = () => setInstance((n) => n + 1)
    }, [])
    return <EditorInstance key={instance} {...props} />
  },
}))

function EditorInstance({ doc, externalVersion, notice }: StubProps) {
  useEffect(() => {
    probe.mounted.push({ doc, externalVersion })
    // Only what it was created with counts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  return <div>{notice}</div>
}

const OLD = '{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"old"}]}]}'
const APPENDED = '{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"old + appended"}]}]}'
const EDITED = '{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"edited"}]}]}'

const get = vi.mocked(richTextClient.getRichText)
const put = vi.mocked(richTextClient.putRichText)
const served = (doc: string, updatedAtMs: number) => ({ richText: richText('a', 'content', doc, updatedAtMs) }) as never

// What the editor does when the user types.
const type = (doc: string, appliedVersion: number) => act(() => probe.props!.onUpdate!(JSON.parse(doc), appliedVersion))

const expectedMs = (call: number) => {
  const { seconds = 0n, nanos = 0 } = put.mock.calls[call][0].expectedUpdatedAt!
  return Number(seconds) * 1_000 + nanos / 1e6
}

beforeEach(() => {
  queryClient.clear()
  probe.props = null
  probe.mounted = []
  get.mockResolvedValue(served(OLD, 1_000))
})

afterEach(() => {
  vi.clearAllMocks()
})

async function mountField() {
  render(
    <QueryClientProvider client={queryClient}>
      <EntityRichTextField entityId="a" propertyId="content" />
    </QueryClientProvider>,
  )
  await waitFor(() => expect(probe.props).not.toBeNull())
}

// An agent's append arrives over Watch while the editor is open.
async function deliverAppend() {
  act(() => writeRichTextIfNewer(richText('a', 'content', APPENDED, 2_000)))
  await waitFor(() => expect(probe.props!.externalVersion).toBe(1))
}

describe('EntityRichTextField', () => {
  it('saves a doc built on the version the editor set its content from (I-80)', async () => {
    await mountField()
    await deliverAppend()
    put.mockRejectedValueOnce(new ConnectError('stale', Code.FailedPrecondition))
    get.mockResolvedValue(served(APPENDED, 2_000))

    // The editor never took the append: it still holds, and emits, version 0.
    type(OLD, 0)

    await waitFor(() => expect(put).toHaveBeenCalledTimes(1))
    expect(expectedMs(0)).toBe(1_000)
    expect((await screen.findByRole('status')).textContent).toContain('Updated elsewhere')
  })

  it('saves a doc the editor built on the adopted version against that version', async () => {
    await mountField()
    await deliverAppend()
    put.mockResolvedValueOnce(served(EDITED, 3_000))

    type(EDITED, 1)

    await waitFor(() => expect(put).toHaveBeenCalledTimes(1))
    expect(expectedMs(0)).toBe(2_000)
  })

  it("expects its own save's version for an edit made while that save was in flight", async () => {
    await mountField()
    let land!: (value: never) => void
    put.mockReturnValueOnce(new Promise((resolve) => (land = resolve)) as never)
    put.mockResolvedValueOnce(served(EDITED, 4_000))

    type(OLD, 0)
    await waitFor(() => expect(put).toHaveBeenCalledTimes(1))
    type(EDITED, 0)
    await act(async () => land(served(OLD, 3_000)))

    await waitFor(() => expect(put).toHaveBeenCalledTimes(2))
    expect(expectedMs(0)).toBe(1_000)
    expect(expectedMs(1)).toBe(3_000)
  })

  it('starts a re-created editor from the adopted doc (I-80)', async () => {
    await mountField()
    expect(probe.mounted).toEqual([{ doc: OLD, externalVersion: 0 }])
    await deliverAppend()

    act(() => probe.recreate())

    expect(probe.mounted).toHaveLength(2)
    expect(probe.mounted[1]).toEqual({ doc: APPENDED, externalVersion: 1 })
  })

  it('starts a re-created editor from the doc its own save wrote', async () => {
    await mountField()
    put.mockResolvedValueOnce(served(EDITED, 3_000))
    type(EDITED, 0)
    await waitFor(() => expect(probe.props!.doc).toBe(EDITED))

    act(() => probe.recreate())

    expect(probe.mounted[1]).toEqual({ doc: EDITED, externalVersion: 0 })
  })
})
