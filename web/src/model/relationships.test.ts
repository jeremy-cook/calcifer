import { beforeEach, describe, expect, it, vi } from 'vitest'
import { create } from '@bufbuild/protobuf'
import { ListStructuresResponseSchema } from 'calcifer/v1/services_pb'
import { PropertyKind } from 'calcifer/v1/structures_pb'
import { queryClient } from '~/model/api'
import { selectRelationshipRows, type RelationshipRow } from '~/model/relationships'
import { structuresQuery } from '~/model/structures'
import { entity, link, ofType, withLinks } from '~/model/testing'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {},
  richTextClient: {},
  structureClient: {},
}))

beforeEach(() => {
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
        {
          type: 'DailyNote',
          properties: [
            { id: 'date', kind: PropertyKind.DATE },
            { id: 'content', kind: PropertyKind.RICHTEXT },
          ],
        },
      ],
    }),
  )
})

const tag = (id: string) => ofType(entity(id), 'Tag')

// The reason kinds of a row, with a property reason as `property:<label>`.
function reasonsOf(row: RelationshipRow): string[] {
  return row.reasons.map((r) => (r.kind === 'property' ? `property:${r.label}` : r.kind))
}

function summary(rows: RelationshipRow[]): string[] {
  return rows.map((r) => `${r.entityId} ${r.direction} ${reasonsOf(r).join('+')}`)
}

describe('selectRelationshipRows', () => {
  it('labels outgoing related and mentioned links, merged into one row', () => {
    const a = withLinks(
      entity('a'),
      link('b', '@related', 3_000),
      link('b', 'content', 2_000),
      link('c', 'content', 1_000),
    )
    const rows = selectRelationshipRows([a, entity('b'), entity('c')], a)

    expect(summary(rows)).toEqual(['b outgoing related+mentioned', 'c outgoing mentioned'])
    expect(rows[0].mostRecentAt).toEqual(new Date(3_000))
    expect(rows[0]).toMatchObject({ entityId: 'b', structureType: 'Note', name: 'b' })
  })

  it('labels incoming related, mentions and property back-references', () => {
    const n = entity('n')
    const related = withLinks(entity('related'), link('n', '@related', 3_000))
    const mentions = withLinks(entity('mentions'), link('n', 'content', 2_000))
    const both = withLinks(entity('both'), link('n', 'content', 1_000), link('n', '@related', 1_500))
    expect(summary(selectRelationshipRows([n, related, mentions, both], n))).toEqual([
      'related incoming related',
      'both incoming related+mentioned',
      'mentions incoming mentioned',
    ])

    // A Note whose `tags` names the Tag, and which also writes `#tag` in its text.
    const t = tag('t')
    const tagged = withLinks(entity('tagged'), link('t', 'content', 1_000, 'Tag'), link('t', 'tags', 2_000, 'Tag'))
    const rows = selectRelationshipRows([t, tagged], t)
    expect(summary(rows)).toEqual(['tagged incoming mentioned+property:Tags'])
    expect(rows[0].reasons[1]).toEqual({ kind: 'property', propertyId: 'tags', label: 'Tags' })
  })

  it('falls back to the property id when the registry has no label for it', () => {
    const t = tag('t')
    const other = withLinks(entity('other'), link('t', 'mystery', 1_000, 'Tag'))
    expect(summary(selectRelationshipRows([t, other], t))).toEqual(['other incoming property:mystery'])
  })

  it("leaves out the entity's own relation-property links", () => {
    const a = withLinks(entity('a'), link('t', 'tags', 2_000, 'Tag'), link('b', 'content', 1_000))
    const rows = selectRelationshipRows([a, tag('t'), entity('b')], a)

    expect(summary(rows)).toEqual(['b outgoing mentioned'])
  })

  it('excludes self-links', () => {
    const a = withLinks(entity('a'), link('a', '@related', 2_000), link('a', 'content', 1_000))
    const b = withLinks(entity('b'), link('b', 'content', 1_000))

    expect(selectRelationshipRows([a, b], a)).toEqual([])
  })

  it('orders related rows first, newest first within each group', () => {
    const n = withLinks(
      entity('n'),
      link('old-rel', '@related', 1_000),
      link('new-rel', '@related', 2_000),
      link('new-men', 'content', 9_000),
      link('old-men', 'content', 5_000),
      // Mentioned after the relationship was made, but still in the related group.
      link('both', '@related', 500),
      link('both', 'content', 20_000),
    )
    const incomingRelated = withLinks(entity('in-rel'), link('n', '@related', 8_000))
    const incomingMention = withLinks(entity('in-men'), link('n', 'content', 10_000))
    const entities = [
      n,
      incomingMention,
      entity('old-men'),
      incomingRelated,
      entity('new-men'),
      entity('old-rel'),
      entity('both'),
      entity('new-rel'),
    ]

    expect(summary(selectRelationshipRows(entities, n))).toEqual([
      'both outgoing related+mentioned',
      'new-rel outgoing related',
      'old-rel outgoing related',
      'in-rel incoming related',
      'in-men incoming mentioned',
      'new-men outgoing mentioned',
      'old-men outgoing mentioned',
    ])
  })

  it('keeps an outgoing and an incoming row for the same entity apart', () => {
    const a = withLinks(entity('a'), link('b', 'content', 1_000))
    const b = withLinks(entity('b'), link('a', '@related', 2_000))

    expect(summary(selectRelationshipRows([a, b], a))).toEqual(['b incoming related', 'b outgoing mentioned'])
  })

  it('leaves out links to entities the replica does not hold, and reserved scopes it does not know', () => {
    const a = withLinks(entity('a'), link('gone', '@related', 2_000), link('b', '@blocks', 1_000))
    const other = withLinks(entity('other'), link('a', '@blocks', 1_000))

    expect(selectRelationshipRows([a, entity('b'), other], a)).toEqual([])
  })
})
