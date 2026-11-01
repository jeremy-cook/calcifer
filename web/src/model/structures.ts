import type { Icon } from '@phosphor-icons/react'
import { CheckSquareIcon, FileIcon, HashIcon, NoteIcon, NotebookIcon } from '@phosphor-icons/react'
import { queryOptions, useQuery } from '@tanstack/react-query'
import type { Entity } from 'calcifer/v1/entities_pb'
import type { ListStructuresResponse } from 'calcifer/v1/services_pb'
import { PropertyKind, type FacetDef, type PropertyDef, type StructureDef } from 'calcifer/v1/structures_pb'
import { queryClient, qk, structureClient } from '~/model/api'

export { PropertyKind } from 'calcifer/v1/structures_pb'
export type { FacetDef, PropertyDef, SelectOption, StructureDef } from 'calcifer/v1/structures_pb'

// The registry itself (structures and facets: names, properties, options, flags) is authored in
// server/src/structures.rs and fetched once over StructureService.ListStructures (ADR 7).
// It never changes while the server runs. App doesn't render the router until
// it has loaded, so everything below the router may read it synchronously.
export const structuresQuery = queryOptions({
  queryKey: qk.structures(),
  queryFn: (): Promise<ListStructuresResponse> => structureClient.listStructures({}),
  staleTime: Infinity,
  gcTime: Infinity,
  // The app can't render without the registry, so it retries until the server answers.
  retry: true,
  retryDelay: 2_000,
})

// Sync read for non-React code (editor suggestions, route validation, pure
// selectors). Empty only before the registry has loaded.
export function getStructures(): readonly StructureDef[] {
  return queryClient.getQueryData(structuresQuery.queryKey)?.structures ?? []
}

export function useStructures(): readonly StructureDef[] {
  return useQuery(structuresQuery).data?.structures ?? []
}

export function getFacets(): readonly FacetDef[] {
  return queryClient.getQueryData(structuresQuery.queryKey)?.facets ?? []
}

export function useFacets(): readonly FacetDef[] {
  return useQuery(structuresQuery).data?.facets ?? []
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

const NO_STRUCTURE_TYPES: readonly string[] = []

// The structure types an entity of `structureType` may hold relationships to (ADR 12).
// Empty: it can't hold any, or the registry doesn't know the type.
export function useRelatesTo(structureType: string): readonly string[] {
  return useStructure(structureType)?.relatesTo ?? NO_STRUCTURE_TYPES
}

export function getFacet(id: string): FacetDef | undefined {
  return getFacets().find((f) => f.id === id)
}

export function useFacet(id: string): FacetDef | undefined {
  return useFacets().find((f) => f.id === id)
}

// Works on a structure or a facet: both declare `properties`.
export function propertyDef(def: StructureDef | FacetDef | undefined, id: string): PropertyDef | undefined {
  return def?.properties.find((p) => p.id === id)
}

// The definition of `id` on this entity: its structure's, else one of its
// attached facets'. Facet ids are namespaced (`todo.status`), so they can't collide.
export function propertyDefFor(entity: Entity, id: string): PropertyDef | undefined {
  const own = propertyDef(getStructure(entity.structureType), id)
  if (own) return own
  for (const facetId of entity.facets) {
    const def = propertyDef(getFacet(facetId), id)
    if (def) return def
  }
  return undefined
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
  // How the structure's list page lays out its entities; rows when unset.
  listLayout?: 'alphabetical'
}

// Keyed by structure type. A type the server adds without an entry here still
// renders, with the fallback.
const PRESENTATION: Record<string, StructurePresentation> = {
  Note: { icon: NoteIcon, color: 'var(--chart-1)' },
  Tag: { icon: HashIcon, color: 'var(--chart-2)', listLayout: 'alphabetical' },
  DailyNote: { icon: NotebookIcon, color: 'var(--chart-3)' },
}

const FALLBACK_PRESENTATION: StructurePresentation = { icon: FileIcon, color: 'var(--muted-foreground)' }

export function structurePresentation(structureType: string): StructurePresentation {
  return PRESENTATION[structureType] ?? FALLBACK_PRESENTATION
}

export interface FacetPresentation {
  icon: Icon
  color: string
  // Sidebar label for the facet's list page.
  navLabel: string
  // The facet's list page; undefined when it has none.
  route?: string
}

// Keyed by facet id. A facet the server adds without an entry here still
// renders, with the fallback icon, its plural as the label and no list page.
const FACET_PRESENTATION: Record<string, FacetPresentation> = {
  todo: { icon: CheckSquareIcon, color: 'var(--chart-4)', navLabel: 'ToDo', route: '/v/todos' },
}

export function facetPresentation(id: string): FacetPresentation {
  return FACET_PRESENTATION[id] ?? { ...FALLBACK_PRESENTATION, navLabel: getFacet(id)?.plural ?? id }
}
