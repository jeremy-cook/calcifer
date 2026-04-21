import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { StructureId } from '~/model/structures'
import { STRUCTURES } from '~/model/structures'

export interface Entity {
  id: string
  structureId: StructureId
  title: string
  createdAt: string
  updatedAt: string
}

type CreatableStructureId = Exclude<StructureId, 'DailyNote'>

interface EntityState {
  entities: Record<string, Entity>
  createEntity: (structureId: CreatableStructureId, title?: string) => Entity
}

function defaultTitleFor(structureId: CreatableStructureId): string {
  return `Untitled ${STRUCTURES[structureId].name}`
}

export const useEntityStore = create<EntityState>()(
  persist(
    (set, get) => ({
      entities: {},
      createEntity: (structureId, title) => {
        const now = new Date().toISOString()
        const entity: Entity = {
          id: crypto.randomUUID(),
          structureId,
          title: title ?? defaultTitleFor(structureId),
          createdAt: now,
          updatedAt: now,
        }
        set({ entities: { ...get().entities, [entity.id]: entity } })
        return entity
      },
    }),
    { name: 'calcifer.entities.v1' },
  ),
)

export function listByStructure(entities: Record<string, Entity>, structureId: StructureId): Entity[] {
  const matches = Object.values(entities).filter((e) => e.structureId === structureId)
  if (structureId === 'DailyNote') {
    return matches.sort((a, b) => b.title.localeCompare(a.title))
  }
  return matches.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
}

export function entityById(entities: Record<string, Entity>, id: string): Entity | undefined {
  return entities[id]
}
