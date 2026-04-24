import type { Icon } from '@phosphor-icons/react'
import { HashIcon, NoteIcon, NotebookIcon } from '@phosphor-icons/react'

export interface PropertyDef {
  id: string
  type: 'richtext' | 'text' | 'number' | 'date' | 'select' | 'relation'
}

export interface StructureMeta {
  type: string
  name: string
  plural: string
  icon: Icon
  color: string
  properties: readonly PropertyDef[]
  creatable?: boolean
}

export const STRUCTURES = {
  Note: {
    type: 'Note',
    name: 'Note',
    plural: 'Notes',
    icon: NoteIcon,
    color: 'var(--chart-1)',
    properties: [{ id: 'content', type: 'richtext' }],
  },
  Tag: {
    type: 'Tag',
    name: 'Tag',
    plural: 'Tags',
    icon: HashIcon,
    color: 'var(--chart-2)',
    properties: [],
  },
  DailyNote: {
    type: 'DailyNote',
    name: 'Daily Note',
    plural: 'Daily Notes',
    icon: NotebookIcon,
    color: 'var(--chart-3)',
    properties: [{ id: 'content', type: 'richtext' }],
    creatable: false,
  },
} as const satisfies Record<string, StructureMeta>

export type StructureType = keyof typeof STRUCTURES

export const STRUCTURE_LIST: readonly StructureMeta[] = Object.values(STRUCTURES)

export const CREATABLE_STRUCTURES: readonly StructureMeta[] = STRUCTURE_LIST.filter((s) => s.creatable !== false)
