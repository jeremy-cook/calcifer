import type { Icon } from '@phosphor-icons/react'
import { CheckSquareIcon, FileIcon, HashIcon, NoteIcon, NotebookIcon } from '@phosphor-icons/react'
import { queryOptions, useQuery } from '@tanstack/react-query'
import { PropertyKind, type PropertyDef, type StructureDef } from '@calcifer/proto/calcifer/v1/structures_pb'
import { queryClient, qk, structureClient } from '~/model/api'

export { PropertyKind } from '@calcifer/proto/calcifer/v1/structures_pb'
export type { PropertyDef, SelectOption, StructureDef } from '@calcifer/proto/calcifer/v1/structures_pb'

// The registry itself (types, names, properties, options, flags) is authored in
// server/src/structures.rs and fetched once over StructureService.ListStructures (ADR 7).
// It never changes while the server runs. App doesn't render the router until
// it has loaded, so everything below the router may read it synchronously.
export const structuresQuery = queryOptions({
  queryKey: qk.structures(),
  queryFn: async () => (await structureClient.listStructures({})).structures,
  staleTime: Infinity,
  gcTime: Infinity,
})

// Sync read for non-React code (editor suggestions, route validation, pure
// selectors). Empty only before the registry has loaded.
export function getStructures(): readonly StructureDef[] {
  return queryClient.getQueryData(structuresQuery.queryKey) ?? []
}

export function useStructures(): readonly StructureDef[] {
  return useQuery(structuresQuery).data ?? []
}

export function findStructure(structures: readonly StructureDef[], structureType: string): StructureDef | undefined {
  return structures.find((s) => s.type === structureType)
}

export function getStructure(structureType: string): StructureDef | undefined {
  return findStructure(getStructures(), structureType)
}

export function useStructure(structureType: string): StructureDef | undefined {
  return findStructure(useStructures(), structureType)
}

export function propertyDef(structure: StructureDef | undefined, id: string): PropertyDef | undefined {
  return structure?.properties.find((p) => p.id === id)
}

// A structure's declared rich-text property ids, in registry order. Each one
// names a doc per entity, addressed with `richTextRef(entity.id, id)`.
export function richTextPropertyIds(structureType: string): string[] {
  const properties = getStructure(structureType)?.properties ?? []
  return properties.filter((p) => p.kind === PropertyKind.RICHTEXT).map((p) => p.id)
}

export function optionLabel(def: PropertyDef | undefined, key: string): string {
  return def?.options.find((o) => o.key === key)?.label ?? key
}

// Unknown types default to the permissive side, as when these were optional flags.
export function isMentionable(structureType: string): boolean {
  return getStructure(structureType)?.mentionable ?? true
}

export function isNameEditable(structureType: string): boolean {
  return getStructure(structureType)?.nameEditable ?? true
}

export function hasUniqueNames(structureType: string): boolean {
  return getStructure(structureType)?.uniqueNames ?? false
}

// --- Presentation (client-only) ---

export interface StructurePresentation {
  icon: Icon
  color: string
}

// Keyed by structure type. A type the server adds without an entry here still
// renders, with the fallback.
const PRESENTATION: Record<string, StructurePresentation> = {
  Note: { icon: NoteIcon, color: 'var(--chart-1)' },
  Tag: { icon: HashIcon, color: 'var(--chart-2)' },
  DailyNote: { icon: NotebookIcon, color: 'var(--chart-3)' },
  Todo: { icon: CheckSquareIcon, color: 'var(--chart-4)' },
}

const FALLBACK_PRESENTATION: StructurePresentation = { icon: FileIcon, color: 'var(--muted-foreground)' }

export function structurePresentation(structureType: string): StructurePresentation {
  return PRESENTATION[structureType] ?? FALLBACK_PRESENTATION
}
