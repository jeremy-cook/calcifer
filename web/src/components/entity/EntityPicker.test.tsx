// @vitest-environment jsdom
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { create } from '@bufbuild/protobuf'
import { ListStructuresResponseSchema } from 'calcifer/v1/services_pb'
import { NameRule } from 'calcifer/v1/structures_pb'
import { queryClient } from '~/model/api'
import { replica } from '~/model/replica'
import { structuresQuery } from '~/model/structures'
import { entity, ofType } from '~/model/testing'
import { EntityPicker } from '~/components/entity/EntityPicker'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {},
  richTextClient: {},
  structureClient: {},
}))

beforeAll(() => {
  // cmdk measures and scrolls its list, which jsdom doesn't do.
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  Element.prototype.scrollIntoView ??= () => {}
})

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

afterEach(() => {
  cleanup()
})

// Opens a tags-field picker and types `query` into it. `excludeIds` are the
// Tags the note already has.
function typeIntoTagPicker(query: string, excludeIds: string[] = []) {
  render(
    <QueryClientProvider client={queryClient}>
      <EntityPicker targetStructure="Tag" excludeIds={excludeIds} onPick={() => {}}>
        <button type="button">Add tag</button>
      </EntityPicker>
    </QueryClientProvider>,
  )
  fireEvent.click(screen.getByRole('button', { name: 'Add tag' }))
  fireEvent.change(screen.getByPlaceholderText('Search…'), { target: { value: query } })
}

describe('EntityPicker', () => {
  it("offers no Create for an existing Tag's name, whatever the case (I-91)", () => {
    replica.applySnapshot([ofType(entity('t', 'concept'), 'Tag')])
    typeIntoTagPicker('Concept')
    expect(screen.getByText('concept')).toBeTruthy()
    expect(screen.queryByText(/^Create/)).toBeNull()
  })

  it('offers no Create for a Tag the note already has', () => {
    replica.applySnapshot([ofType(entity('t', 'concept'), 'Tag')])
    typeIntoTagPicker('concept', ['t'])
    expect(screen.queryByText(/^Create/)).toBeNull()
  })

  it("leaves a sentence's closing period out of the name (I-91)", () => {
    typeIntoTagPicker('naming.')
    expect(screen.getByText('Create "naming"')).toBeTruthy()
  })

  it('says the rule instead of offering a name that breaks it (I-90)', () => {
    typeIntoTagPicker('Apollo 11')
    expect(screen.queryByText(/^Create/)).toBeNull()
    expect(screen.getByText('Tag names are letters and digits with single dots between them, like v1.2')).toBeTruthy()
  })
})
