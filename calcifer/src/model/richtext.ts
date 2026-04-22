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
  RichTextSchema,
  type RichText,
  type RichTextRef,
} from '@calcifer/proto/calcifer/v1/entities_pb'

interface RichTextState {
  docs: Record<string, RichText>
  getRichText: (ref: RichTextRef) => RichText | undefined
  putRichText: (ref: RichTextRef, doc: string) => void
  deleteByEntity: (entityId: string) => void
}

type PersistedRichTextState = Pick<RichTextState, 'docs'>

export function richTextKey(ref: RichTextRef): string {
  return `${ref.entityId}:${ref.propertyId}`
}

const richTextStorage: PersistStorage<PersistedRichTextState> = {
  getItem: (name) => {
    const raw = localStorage.getItem(name)
    if (!raw) return null
    const parsed = JSON.parse(raw) as {
      state: { docs: Record<string, JsonValue> }
      version?: number
    }
    return {
      state: {
        docs: Object.fromEntries(
          Object.entries(parsed.state.docs).map(([key, json]) => [
            key,
            fromJson(RichTextSchema, json),
          ]),
        ),
      },
      version: parsed.version,
    }
  },
  setItem: (name, value) => {
    const serialized = {
      state: {
        docs: Object.fromEntries(
          Object.entries(value.state.docs).map(([key, doc]) => [
            key,
            toJson(RichTextSchema, doc),
          ]),
        ),
      },
      version: value.version,
    }
    localStorage.setItem(name, JSON.stringify(serialized))
  },
  removeItem: (name) => localStorage.removeItem(name),
}

export const useRichTextStore = create<RichTextState>()(
  persist(
    (set, get) => ({
      docs: {},
      getRichText: (ref) => get().docs[richTextKey(ref)],
      putRichText: (ref, doc) => {
        const key = richTextKey(ref)
        const message = createMessage(RichTextSchema, {
          ref,
          doc,
          updatedAt: timestampNow(),
        })
        set({ docs: { ...get().docs, [key]: message } })
      },
      deleteByEntity: (entityId) => {
        const prefix = `${entityId}:`
        const next: Record<string, RichText> = {}
        for (const [key, doc] of Object.entries(get().docs)) {
          if (!key.startsWith(prefix)) next[key] = doc
        }
        set({ docs: next })
      },
    }),
    {
      name: 'calcifer.richtext.v1',
      storage: richTextStorage,
      partialize: (state) => ({ docs: state.docs }),
    },
  ),
)
