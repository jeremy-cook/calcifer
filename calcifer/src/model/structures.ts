import type { Icon } from '@phosphor-icons/react'
import { HashIcon, NoteIcon, NotebookIcon } from '@phosphor-icons/react'

export interface PropertyDef {
  id: string
  type: 'richtext' | 'text' | 'number' | 'date' | 'select' | 'relation'
}

export interface StructureMeta {
  id: string
  name: string
  plural: string
  icon: Icon
  properties: readonly PropertyDef[]
  creatable?: boolean
}

export const STRUCTURES = {
  Note: {
    id: 'Note',
    name: 'Note',
    plural: 'Notes',
    icon: NoteIcon,
    properties: [{ id: 'content', type: 'richtext' }],
  },
  RootTag: {
    id: 'RootTag',
    name: 'Tag',
    plural: 'Tags',
    icon: HashIcon,
    properties: [],
  },
  DailyNote: {
    id: 'DailyNote',
    name: 'Daily Note',
    plural: 'Daily Notes',
    icon: NotebookIcon,
    properties: [{ id: 'content', type: 'richtext' }],
    creatable: false,
  },
} as const satisfies Record<string, StructureMeta>

export type StructureId = keyof typeof STRUCTURES

export const STRUCTURE_LIST: readonly StructureMeta[] = Object.values(STRUCTURES)

export const CREATABLE_STRUCTURES: readonly StructureMeta[] = STRUCTURE_LIST.filter((s) => s.creatable !== false)
