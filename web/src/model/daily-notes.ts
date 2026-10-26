import { useEffect } from 'react'
import type { RichTextRef } from 'calcifer/v1/entities_pb'
import { richTextRef } from '~/model/richtext'
import {
  dailyNoteByDate,
  deleteEntity,
  deleteEntityIfRichTextEmpty,
  getEntitiesSnapshot,
  type Entity,
} from '~/model/store'
import { richTextPropertyIds } from '~/model/structures'

// The daily note's content doc: its structure's declared rich-text property
// (DailyNote declares one, `content`), addressed by the entity's id.
export function dailyNoteContentRef(entity: Entity): RichTextRef | undefined {
  const [propertyId] = richTextPropertyIds(entity.structureType)
  return propertyId ? richTextRef(entity.id, propertyId) : undefined
}

// Pending prunes by day, scheduled when a view of the day goes away.
const pendingPrunes = new Map<string, ReturnType<typeof setTimeout>>()

// Prunes the day's daily note when the view leaves the day, if its doc is empty.
// An unmount only schedules the prune, and a mount for the same day cancels it,
// so a remount that comes straight back (StrictMode in dev, a key change) isn't
// leaving. The prune waits a macrotask, and by then every cleanup in the commit
// has run, the editor's flush of its debounced save included (I-39).
export function useDailyNotePrune(iso: string): void {
  useEffect(() => {
    clearTimeout(pendingPrunes.get(iso))
    pendingPrunes.delete(iso)
    return () => {
      pendingPrunes.set(
        iso,
        setTimeout(() => {
          pendingPrunes.delete(iso)
          pruneDailyNote(iso)
        }, 0),
      )
    }
  }, [iso])
}

function pruneDailyNote(iso: string): void {
  const existing = dailyNoteByDate(getEntitiesSnapshot(), iso)
  if (!existing) return
  const ref = dailyNoteContentRef(existing)
  if (ref) deleteEntityIfRichTextEmpty(existing.id, ref)
  else deleteEntity(existing.id)
}
