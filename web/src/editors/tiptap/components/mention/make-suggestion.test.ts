import { beforeEach, describe, expect, it } from 'vitest'
import { create } from '@bufbuild/protobuf'
import { ListStructuresResponseSchema } from 'calcifer/v1/services_pb'
import { NameRule } from 'calcifer/v1/structures_pb'
import { queryClient } from '~/model/api'
import { replica } from '~/model/replica'
import { structuresQuery } from '~/model/structures'
import { entity, ofType } from '~/model/testing'
import { makeSuggestion } from './make-suggestion'
import type { EntitySuggestionItem } from './types'

// What the `#` menu offers for `query`, the text typed after the `#`.
function hashMenu(query: string): EntitySuggestionItem[] {
  const { items } = makeSuggestion({ char: '#', structureFilter: 'Tag' })
  return items!({ query, editor: undefined as never }) as EntitySuggestionItem[]
}

beforeEach(() => {
  replica.reset()
  queryClient.clear()
  queryClient.setQueryData(
    structuresQuery.queryKey,
    create(ListStructuresResponseSchema, {
      structures: [
        { type: 'Note' },
        { type: 'Tag', name: 'Tag', creatable: true, uniqueNames: true, nameRule: NameRule.DOTTED },
      ],
    }),
  )
})

describe('the # menu', () => {
  it("offers the name without a sentence's closing period (I-91)", () => {
    const [create] = hashMenu('naming.')
    expect(create).toMatchObject({
      isCreate: true,
      label: 'naming',
      createLabel: 'Create new Tag: naming',
      suffix: '.',
    })
  })

  it('offers an existing Tag, not a create, whatever the case', () => {
    replica.applySnapshot([ofType(entity('t', 'Naming'), 'Tag')])
    const items = hashMenu('naming.')
    expect(items.map((i) => [i.label, i.isCreate ?? false, i.suffix])).toEqual([['Naming', false, '.']])
  })

  it('keeps dots inside the name', () => {
    expect(hashMenu('v1.2')[0]).toMatchObject({ isCreate: true, label: 'v1.2', suffix: '' })
  })

  it('says the rule instead of creating a name that breaks it (I-90)', () => {
    const items = hashMenu('apollo-11')
    expect(items).toHaveLength(1)
    expect(items[0]).toMatchObject({
      isHint: true,
      label: 'Tag names are letters and digits with single dots between them, like v1.2',
    })
  })
})
