// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { create } from '@bufbuild/protobuf'
import type { Entity } from 'calcifer/v1/entities_pb'
import { ListStructuresResponseSchema } from 'calcifer/v1/services_pb'
import { PropertyKind } from 'calcifer/v1/structures_pb'
import { entityClient, queryClient } from '~/model/api'
import { replica } from '~/model/replica'
import { structuresQuery } from '~/model/structures'
import { entity, link, ofType, withLinks } from '~/model/testing'
import { RelationshipsPanel } from '~/components/relationships/RelationshipsPanel'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: { removeRelationship: vi.fn(), addRelationship: vi.fn() },
  richTextClient: {},
  structureClient: {},
}))

// The router isn't under test; a row's link only needs to render.
vi.mock('@tanstack/react-router', async () => {
  const { createElement } = await import('react')
  return {
    Link: ({ to, params, children }: { to: string; params: { id: string }; children: React.ReactNode }) =>
      createElement('a', { href: to.replace('$id', params.id) }, children),
  }
})

beforeEach(() => {
  replica.reset()
  queryClient.clear()
  queryClient.setQueryData(
    structuresQuery.queryKey,
    create(ListStructuresResponseSchema, {
      structures: [
        {
          type: 'Note',
          properties: [
            { id: 'tags', kind: PropertyKind.RELATIONS, label: 'Tags', targetStructure: 'Tag' },
            { id: 'content', kind: PropertyKind.RICHTEXT },
          ],
          relatesTo: ['Note'],
        },
        { type: 'Tag' },
      ],
    }),
  )
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.clearAllMocks()
})

// Seeds the replica with `entities` and renders the panel for the first, open.
function renderOpenPanel(...entities: Entity[]) {
  replica.applySnapshot(entities)
  const result = render(
    <QueryClientProvider client={queryClient}>
      <RelationshipsPanel entity={entities[0]} />
    </QueryClientProvider>,
  )
  fireEvent.click(screen.getByRole('button', { name: /^Relationships/ }))
  return result
}

// The names of the listed rows, in order.
function rowNames(): string[] {
  return screen.getAllByRole('link').map((a) => a.textContent ?? '')
}

// What a row says besides its name.
function labelsOf(name: string): string {
  return screen.getByText(name).closest('li')!.textContent!.slice(name.length)
}

describe('RelationshipsPanel', () => {
  it('lists rows with their reasons', () => {
    const a = withLinks(
      entity('a', 'Alpha'),
      link('b', '@related', 5_000),
      link('c', 'content', 3_000),
      link('t', 'tags', 6_000, 'Tag'),
    )
    const d = withLinks(entity('d', 'Delta'), link('a', '@related', 4_000))
    const e = withLinks(entity('e', 'Echo'), link('a', 'content', 2_000))
    renderOpenPanel(a, entity('b', 'Bravo'), entity('c', 'Charlie'), ofType(entity('t', 'Urgent'), 'Tag'), d, e)

    expect(screen.getByRole('button', { name: 'Relationships (4)' })).toBeTruthy()
    // Direct relationships first, then the rest; the Tag shows in its property field.
    expect(rowNames()).toEqual(['Bravo', 'Delta', 'Charlie', 'Echo'])
    expect(labelsOf('Bravo')).toBe('related')
    expect(labelsOf('Delta')).toBe('related to this')
    expect(labelsOf('Charlie')).toBe('mentioned')
    expect(labelsOf('Echo')).toBe('mentions this')
  })

  it('joins several reasons for one row', () => {
    const a = withLinks(entity('a', 'Alpha'), link('b', '@related', 5_000), link('b', 'content', 3_000))
    renderOpenPanel(a, entity('b', 'Bravo'))
    expect(rowNames()).toEqual(['Bravo'])
    expect(labelsOf('Bravo')).toBe('related · mentioned')
  })

  it("labels a back-reference from a relation property with the property's label", () => {
    const urgent = ofType(entity('t', 'Urgent'), 'Tag')
    const n = withLinks(entity('n', 'Note one'), link('t', 'tags', 2_000, 'Tag'))
    renderOpenPanel(urgent, n)
    expect(labelsOf('Note one')).toBe('via Tags')
  })

  it('says so when there is nothing to list', () => {
    renderOpenPanel(entity('a', 'Alpha'))
    expect(screen.getByText(/No relationships yet/)).toBeTruthy()
  })

  it('shows a remove control only on related rows', () => {
    const a = withLinks(
      entity('a', 'Alpha'),
      link('b', '@related', 5_000),
      link('c', 'content', 3_000),
      link('f', '@related', 2_000),
      link('f', 'content', 1_000),
    )
    const d = withLinks(entity('d', 'Delta'), link('a', '@related', 4_000))
    renderOpenPanel(a, entity('b', 'Bravo'), entity('c', 'Charlie'), d, entity('f', 'Foxtrot'))

    const removable = screen.getAllByRole('button', { name: /^Remove relationship to / }).map((b) => b.ariaLabel)
    // Not on the mention-only row, nor on the one that relates to this entity.
    expect(removable).toEqual(['Remove relationship to Bravo', 'Remove relationship to Foxtrot'])
  })

  it('removes a relationship from its row', async () => {
    const a = withLinks(entity('a', 'Alpha'), link('b', '@related', 5_000))
    vi.mocked(entityClient.removeRelationship).mockResolvedValue({ entity: entity('a', 'Alpha') } as never)
    renderOpenPanel(a, entity('b', 'Bravo'))

    fireEvent.click(screen.getByRole('button', { name: 'Remove relationship to Bravo' }))

    await waitFor(() => expect(entityClient.removeRelationship).toHaveBeenCalledWith({ entityId: 'a', targetId: 'b' }))
  })

  it('shows a jump control on mention-only rows', () => {
    const a = withLinks(
      entity('a', 'Alpha'),
      link('b', '@related', 5_000),
      link('c', 'content', 3_000),
      link('f', '@related', 2_000),
      link('f', 'content', 1_000),
    )
    const d = withLinks(entity('d', 'Delta'), link('a', 'content', 4_000))
    renderOpenPanel(a, entity('b', 'Bravo'), entity('c', 'Charlie'), d, entity('f', 'Foxtrot'))

    const jumps = screen.getAllByRole('button', { name: /^Go to mention of / }).map((b) => b.ariaLabel)
    // Only where the entity itself mentions the other and holds no relationship to it.
    expect(jumps).toEqual(['Go to mention of Charlie'])
  })

  it('scrolls to the first chip for the row and highlights it for a second', () => {
    const a = withLinks(entity('a', 'Alpha'), link('c', 'content', 3_000))
    const scrollIntoView = vi.fn()
    Element.prototype.scrollIntoView = scrollIntoView
    vi.useFakeTimers()
    render(
      <main>
        <span data-type="mention" data-id="x" />
        <span data-type="hashtag" data-id="c" data-testid="first" />
        <span data-type="mention" data-id="c" data-testid="second" />
      </main>,
    )
    renderOpenPanel(a, entity('c', 'Charlie'))

    fireEvent.click(screen.getByRole('button', { name: 'Go to mention of Charlie' }))

    const first = screen.getByTestId('first')
    expect(scrollIntoView.mock.contexts).toEqual([first])
    expect(first.classList.contains('ring-2')).toBe(true)
    expect(screen.getByTestId('second').classList.contains('ring-2')).toBe(false)
    vi.advanceTimersByTime(1_000)
    expect(first.classList.contains('ring-2')).toBe(false)
  })

  it('collapses after 20 rows behind Show all', () => {
    const others = Array.from({ length: 25 }, (_, i) => entity(`n${i}`, `Note ${i}`))
    const a = withLinks(entity('a', 'Alpha'), ...others.map((o, i) => link(o.id, '@related', 1_000 + i)))
    renderOpenPanel(a, ...others)

    expect(screen.getAllByRole('link')).toHaveLength(20)
    fireEvent.click(screen.getByRole('button', { name: 'Show all (25)' }))

    expect(screen.getAllByRole('link')).toHaveLength(25)
    expect(screen.queryByRole('button', { name: /^Show all/ })).toBeNull()
  })

  it('hides Add when the structure relates to nothing', () => {
    const urgent = ofType(entity('t', 'Urgent'), 'Tag')
    renderOpenPanel(urgent)
    expect(screen.queryByRole('button', { name: 'Add' })).toBeNull()
  })

  it('offers Add when the structure relates to something', () => {
    renderOpenPanel(entity('a', 'Alpha'))
    expect(screen.getByRole('button', { name: 'Add' })).toBeTruthy()
  })
})
