import { create } from 'zustand'
import { persist, type PersistStorage } from 'zustand/middleware'
import {
  create as createMessage,
  fromJson,
  toJson,
  type JsonValue,
} from '@bufbuild/protobuf'
import { timestampNow } from '@bufbuild/protobuf/wkt'
import {
  EntitySchema,
  PropertyValueSchema,
  PropertySchema,
  RichTextRefSchema,
  type Entity,
  type Property,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { useRichTextStore } from '~/model/richtext'
import { STRUCTURES, type StructureId } from '~/model/structures'

type CreatableStructureId = Exclude<StructureId, 'DailyNote'>

interface EntityState {
  entities: Record<string, Entity>
  createEntity: (structureId: CreatableStructureId, title?: string) => Entity
  updateEntity: (id: string, patch: { title?: string }) => void
  deleteEntity: (id: string) => void
}

type PersistedEntityState = Pick<EntityState, 'entities'>

function defaultTitleFor(structureId: CreatableStructureId): string {
  return `Untitled ${STRUCTURES[structureId].name}`
}

const entityStorage: PersistStorage<PersistedEntityState> = {
  getItem: (name) => {
    const raw = localStorage.getItem(name)
    if (!raw) return null
    const parsed = JSON.parse(raw) as {
      state: { entities: Record<string, JsonValue> }
      version?: number
    }
    return {
      state: {
        entities: Object.fromEntries(
          Object.entries(parsed.state.entities).map(([id, json]) => [
            id,
            fromJson(EntitySchema, json),
          ]),
        ),
      },
      version: parsed.version,
    }
  },
  setItem: (name, value) => {
    const serialized = {
      state: {
        entities: Object.fromEntries(
          Object.entries(value.state.entities).map(([id, entity]) => [
            id,
            toJson(EntitySchema, entity),
          ]),
        ),
      },
      version: value.version,
    }
    localStorage.setItem(name, JSON.stringify(serialized))
  },
  removeItem: (name) => localStorage.removeItem(name),
}

export const useEntityStore = create<EntityState>()(
  persist(
    (set, get) => ({
      entities: {},
      createEntity: (structureId, title) => {
        const id = crypto.randomUUID()
        const now = timestampNow()
        const structure = STRUCTURES[structureId]
        const properties: Property[] = []
        for (const def of structure.properties) {
          if (def.type === 'richtext') {
            const ref = createMessage(RichTextRefSchema, {
              entityId: id,
              propertyId: def.id,
            })
            const value = createMessage(PropertyValueSchema, {
              value: { case: 'richtext', value: ref },
            })
            properties.push(createMessage(PropertySchema, { id: def.id, value }))
            useRichTextStore.getState().putRichText(ref, '')
          }
        }
        const entity = createMessage(EntitySchema, {
          id,
          structureId,
          title: title ?? defaultTitleFor(structureId),
          properties,
          links: [],
          createdAt: now,
          updatedAt: now,
        })
        set({ entities: { ...get().entities, [id]: entity } })
        return entity
      },
      updateEntity: (id, patch) => {
        const current = get().entities[id]
        if (!current) return
        const next = createMessage(EntitySchema, {
          ...current,
          title: patch.title ?? current.title,
          updatedAt: timestampNow(),
        })
        set({ entities: { ...get().entities, [id]: next } })
      },
      deleteEntity: (id) => {
        const next = { ...get().entities }
        delete next[id]
        set({ entities: next })
        useRichTextStore.getState().deleteByEntity(id)
      },
    }),
    {
      name: 'calcifer.entities.v1',
      storage: entityStorage,
      partialize: (state) => ({ entities: state.entities }),
    },
  ),
)

function updatedAtMillis(entity: Entity): number {
  const ts = entity.updatedAt
  if (!ts) return 0
  return Number(ts.seconds) * 1000 + ts.nanos / 1_000_000
}

export type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

export function listByStructure(
  entities: Record<string, Entity>,
  structureId: StructureId,
): Entity[] {
  const matches = Object.values(entities).filter((e) => e.structureId === structureId)
  if (structureId === 'DailyNote') {
    return matches.sort((a, b) => b.title.localeCompare(a.title))
  }
  return matches.sort((a, b) => updatedAtMillis(b) - updatedAtMillis(a))
}

export function entityById(entities: Record<string, Entity>, id: string): Entity | undefined {
  return entities[id]
}

export function entityUpdatedAtDate(entity: Entity): Date {
  return new Date(updatedAtMillis(entity))
}
