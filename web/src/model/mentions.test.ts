import { beforeEach, describe, expect, it } from 'vitest'
import { create } from '@bufbuild/protobuf'
import { ListStructuresResponseSchema } from 'calcifer/v1/services_pb'
import { queryClient } from '~/model/api'
import { findMentionTargets, type MentionQuery } from '~/model/mentions'
import { replica } from '~/model/replica'
import { structuresQuery } from '~/model/structures'
import { applyWatchEvent } from '~/model/sync'
import { entity, ofType, watch } from '~/model/testing'

function find(overrides: Partial<MentionQuery> = {}) {
  const { entities, exactMatch } = findMentionTargets({
    term: '',
    structureType: null,
    mentionableOnly: false,
    limit: 8,
    ...overrides,
  })
  return { names: entities.map((e) => e.name), exactMatch }
}

beforeEach(() => {
  replica.reset()
  queryClient.clear()
  queryClient.setQueryData(
    structuresQuery.queryKey,
    create(ListStructuresResponseSchema, {
      structures: [
        { type: 'Note', mentionable: true },
        { type: 'DailyNote', mentionable: false },
        { type: 'Tag', mentionable: true },
      ],
    }),
  )
  applyWatchEvent(
    watch.snapshot(
      entity('1', 'Alpha'),
      entity('2', 'alphabet'),
      ofType(entity('3', 'Beta'), 'Tag'),
      ofType(entity('4', 'Alpha day'), 'DailyNote'),
    ),
  )
})

describe('findMentionTargets', () => {
  it('matches names containing the term, ignoring case', () => {
    expect(find({ term: 'ALPH' }).names).toEqual(['Alpha', 'alphabet', 'Alpha day'])
  })

  it('narrows to one structure', () => {
    expect(find({ structureType: 'Tag' }).names).toEqual(['Beta'])
  })

  it('skips structures that opt out when asked to', () => {
    expect(find({ term: 'alpha', mentionableOnly: true }).names).toEqual(['Alpha', 'alphabet'])
  })

  it('reports an exact match, case-insensitively', () => {
    expect(find({ term: 'alpha' }).exactMatch).toBe(true)
    expect(find({ term: 'alph' }).exactMatch).toBe(false)
  })

  it('caps the entities but still finds an exact match past the cap', () => {
    const result = find({ term: 'alpha', limit: 1 })
    expect(result.names).toEqual(['Alpha'])
    applyWatchEvent(watch.snapshot(entity('5', 'Zed'), entity('6', 'zed')))
    expect(find({ term: 'zed', limit: 1 })).toEqual({ names: ['Zed'], exactMatch: true })
  })
})
