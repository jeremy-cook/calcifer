import type { Icon } from '@phosphor-icons/react'
import { HashIcon, NoteIcon, NotebookIcon } from '@phosphor-icons/react'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

export interface PropertyDef {
  id: string
  type: 'richtext' | 'text' | 'number' | 'date' | 'select' | 'relation'
  editable?: boolean
}

export interface NameMeta {
  editable: boolean
  derive?: (entity: Entity) => string
}

export interface StructureMeta {
  type: string
  name: string
  plural: string
  icon: Icon
  color: string
  properties: readonly PropertyDef[]
  creatable?: boolean
  mentionable?: boolean
  uniqueNames?: boolean
  nameMeta?: NameMeta
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
    mentionable: false,
    uniqueNames: true,
  },
  DailyNote: {
    type: 'DailyNote',
    name: 'Daily Note',
    plural: 'Daily Notes',
    icon: NotebookIcon,
    color: 'var(--chart-3)',
    properties: [
      { id: 'date', type: 'date' },
      { id: 'content', type: 'richtext' },
    ],
    nameMeta: { editable: false },
    creatable: false,
    mentionable: false,
  },
} as const satisfies Record<string, StructureMeta>

export type StructureType = keyof typeof STRUCTURES

export const STRUCTURE_LIST: readonly StructureMeta[] = Object.values(STRUCTURES)

export const CREATABLE_STRUCTURES: readonly StructureMeta[] = STRUCTURE_LIST.filter((s) => s.creatable !== false)

export const MENTIONABLE_STRUCTURES: readonly StructureMeta[] = STRUCTURE_LIST.filter((s) => s.mentionable !== false)

export function isMentionable(structureType: string): boolean {
  const meta = (STRUCTURES as Record<string, StructureMeta>)[structureType]
  return meta?.mentionable !== false
}

export function isNameEditable(structureType: string): boolean {
  const meta = (STRUCTURES as Record<string, StructureMeta>)[structureType]
  return meta?.nameMeta?.editable !== false
}

export function hasUniqueNames(structureType: string): boolean {
  const meta = (STRUCTURES as Record<string, StructureMeta>)[structureType]
  return meta?.uniqueNames === true
}
