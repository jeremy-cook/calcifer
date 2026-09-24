import { dailyNoteDate, useAllEntities, type Entity } from '~/model/store'
import { todoDue } from '~/model/todos'

// The calendar's own view of the entity graph. Domains expose plain accessors
// (dailyNoteDate, todoDue, referencedDates); this module is the single place
// that maps them into calendar terms. Rendering decisions stay in components/calendar.

export type CalendarItemKind = 'dailyNote' | 'due' | 'reference'

export interface CalendarItem {
  iso: string
  kind: CalendarItemKind
  entity: Entity
}

export type CalendarIndex = ReadonlyMap<string, readonly CalendarItem[]>

const EMPTY: readonly CalendarItem[] = []

function calendarItemsOf(entity: Entity): CalendarItem[] {
  const items: CalendarItem[] = []
  const noteDate = dailyNoteDate(entity)
  if (noteDate) items.push({ iso: noteDate, kind: 'dailyNote', entity })
  const due = todoDue(entity)
  if (due) items.push({ iso: due, kind: 'due', entity })
  for (const iso of entity.referencedDates) items.push({ iso, kind: 'reference', entity })
  return items
}

export function buildCalendarIndex(entities: Entity[]): CalendarIndex {
  const index = new Map<string, CalendarItem[]>()
  for (const entity of entities) {
    for (const item of calendarItemsOf(entity)) {
      const day = index.get(item.iso)
      if (day) day.push(item)
      else index.set(item.iso, [item])
    }
  }
  return index
}

// Keyed on the query's array identity so every calendar consumer shares one
// build per entities snapshot instead of each memoizing its own.
const indexCache = new WeakMap<Entity[], CalendarIndex>()

export function calendarIndex(entities: Entity[]): CalendarIndex {
  let index = indexCache.get(entities)
  if (!index) {
    index = buildCalendarIndex(entities)
    indexCache.set(entities, index)
  }
  return index
}

export function useCalendarIndex(): CalendarIndex {
  return calendarIndex(useAllEntities())
}

export function itemsOn(index: CalendarIndex, iso: string, kind?: CalendarItemKind): readonly CalendarItem[] {
  const day = index.get(iso) ?? EMPTY
  return kind ? day.filter((item) => item.kind === kind) : day
}
