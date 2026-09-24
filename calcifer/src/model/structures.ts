import type { Icon } from '@phosphor-icons/react'
import { CheckSquareIcon, HashIcon, NoteIcon, NotebookIcon } from '@phosphor-icons/react'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

export interface SelectOption {
  key: string
  label: string
}

export interface PropertyDef {
  id: string
  type: 'richtext' | 'text' | 'number' | 'date' | 'select' | 'relation' | 'relations'
  label?: string
  editable?: boolean
  options?: readonly SelectOption[]
  default?: string
  targetStructure?: string
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

const TODO_STATUS_OPTIONS = [
  { key: 'open', label: 'Open' },
  { key: 'done', label: 'Done' },
] as const satisfies readonly SelectOption[]

const TODO_PRIORITY_OPTIONS = [
  { key: 'none', label: 'None' },
  { key: 'low', label: 'Low' },
  { key: 'medium', label: 'Medium' },
  { key: 'high', label: 'High' },
] as const satisfies readonly SelectOption[]

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
  Todo: {
    type: 'Todo',
    name: 'To-do',
    plural: 'To-dos',
    icon: CheckSquareIcon,
    color: 'var(--chart-4)',
    properties: [
      { id: 'status', type: 'select', label: 'Status', options: TODO_STATUS_OPTIONS, default: 'open' },
      { id: 'priority', type: 'select', label: 'Priority', options: TODO_PRIORITY_OPTIONS, default: 'none' },
      { id: 'due', type: 'date', label: 'Due' },
      { id: 'tags', type: 'relations', label: 'Tags', targetStructure: 'Tag' },
      { id: 'content', type: 'richtext' },
    ],
  },
} as const satisfies Record<string, StructureMeta>

export type StructureType = keyof typeof STRUCTURES

type PropertyDefOf<S extends StructureType> = (typeof STRUCTURES)[S]['properties'][number]

// Typed lookup of a declared property, so callers derive option keys/labels and
// defaults from STRUCTURES instead of restating them.
export function propertyDef<S extends StructureType, Id extends PropertyDefOf<S>['id']>(
  structureType: S,
  id: Id,
): Extract<PropertyDefOf<S>, { id: Id }> {
  const def = (STRUCTURES[structureType].properties as readonly PropertyDef[]).find((p) => p.id === id)
  return def as Extract<PropertyDefOf<S>, { id: Id }>
}

export function optionLabel<K extends string>(options: readonly { key: K; label: string }[], key: K): string {
  return options.find((o) => o.key === key)?.label ?? key
}

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
