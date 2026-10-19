// @vitest-environment jsdom
import { StrictMode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, render } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { create } from '@bufbuild/protobuf'
import { EntitySchema, PropertyValueSchema, RichTextSchema } from '@calcifer/proto/calcifer/v1/entities_pb'
import { PropertyKind, StructureDefSchema } from '@calcifer/proto/calcifer/v1/structures_pb'
import { entityClient, queryClient, richTextClient } from '~/model/api'
import { replica } from '~/model/replica'
import { structuresQuery } from '~/model/structures'
import { applyWatchEvent } from '~/model/sync'
import { watch } from '~/model/testing'
import { DailyNoteSection } from '~/components/calendar/DailyNoteSection'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: { deleteEntity: vi.fn() },
  richTextClient: { getRichText: vi.fn() },
  structureClient: {},
}))

// The editor and the router aren't under test; the prune only needs the section.
vi.mock('~/components/entity/EntityRichTextField', () => ({ EntityRichTextField: () => null }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => vi.fn() }))

const ISO = '2026-10-18'

function dailyNote(id: string, iso: string) {
  const date = create(PropertyValueSchema, { value: { case: 'date', value: iso } })
  return create(EntitySchema, { id, name: iso, structureType: 'DailyNote', properties: { date } })
}

// StrictMode at the root, as in main.tsx: nested under a component, React
// doesn't double-run the effects of a first mount.
function section(iso: string) {
  return (
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <DailyNoteSection iso={iso} />
      </QueryClientProvider>
    </StrictMode>
  )
}

// Lets the prune's timers and promise chain run to the end.
async function settle() {
  for (let i = 0; i < 3; i++) await act(() => new Promise((resolve) => setTimeout(resolve, 0)))
}

beforeEach(() => {
  replica.reset()
  queryClient.clear()
  queryClient.setQueryData(structuresQuery.queryKey, [
    create(StructureDefSchema, {
      type: 'DailyNote',
      properties: [{ id: 'content', kind: PropertyKind.RICHTEXT }],
    }),
  ])
  vi.mocked(entityClient.deleteEntity).mockResolvedValue({} as never)
  vi.mocked(richTextClient.getRichText).mockImplementation(
    async ({ entityId, propertyId }) =>
      ({ richText: create(RichTextSchema, { ref: { entityId, propertyId }, doc: '' }) }) as never,
  )
  applyWatchEvent(watch.snapshot(dailyNote('n1', ISO)))
})

afterEach(() => {
  vi.clearAllMocks()
})

describe('DailyNoteSection prune', () => {
  it("keeps the day's empty note through StrictMode's remount (I-65)", async () => {
    render(section(ISO))
    await settle()
    expect(entityClient.deleteEntity).not.toHaveBeenCalled()
  })

  it('prunes the empty note when the day changes', async () => {
    const { rerender } = render(section(ISO))
    await settle()
    rerender(section('2026-10-19'))
    await settle()
    expect(entityClient.deleteEntity).toHaveBeenCalledWith({ id: 'n1' })
  })

  it('prunes the empty note when the section unmounts', async () => {
    const { unmount } = render(section(ISO))
    await settle()
    unmount()
    await settle()
    expect(entityClient.deleteEntity).toHaveBeenCalledWith({ id: 'n1' })
  })

  it('keeps a note with content when leaving its day', async () => {
    vi.mocked(richTextClient.getRichText).mockImplementation(
      async ({ entityId, propertyId }) =>
        ({
          richText: create(RichTextSchema, {
            ref: { entityId, propertyId },
            doc: JSON.stringify({
              type: 'doc',
              content: [{ type: 'paragraph', content: [{ type: 'text', text: 'hi' }] }],
            }),
          }),
        }) as never,
    )
    const { unmount } = render(section(ISO))
    await settle()
    unmount()
    await settle()
    expect(richTextClient.getRichText).toHaveBeenCalled()
    expect(entityClient.deleteEntity).not.toHaveBeenCalled()
  })
})
