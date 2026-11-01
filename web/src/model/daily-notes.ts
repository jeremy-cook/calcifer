import { useEffect, useState } from 'react'
import type { RichTextRef } from 'calcifer/v1/entities_pb'
import { isAlreadyExists } from '~/model/api'
import { formatDayTitle } from '~/model/dates'
import { richTextRef } from '~/model/richtext'
import {
  dailyNoteByDate,
  dailyNoteDate,
  deleteEntity,
  deleteEntityIfRichTextEmpty,
  getEntitiesSnapshot,
  usePropertyWriters,
  type Entity,
} from '~/model/store'
import { richTextPropertyIds } from '~/model/structures'

// The daily note's content doc: its structure's declared rich-text property
// (DailyNote declares one, `content`), addressed by the entity's id.
export function dailyNoteContentRef(entity: Entity): RichTextRef | undefined {
  const [propertyId] = richTextPropertyIds(entity.structureType)
  return propertyId ? richTextRef(entity.id, propertyId) : undefined
}

// What to call an entity on screen. A daily note's stored name is the server's long
// date ("November 1, 2026"); it shows as its day, the way Home and the calendar do.
export function entityTitle(entity: Entity): string {
  const iso = dailyNoteDate(entity)
  return iso ? formatDayTitle(iso) : entity.name
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

const DAY_TAKEN = 'A daily note already exists for that day.'

export interface DailyNoteDate {
  // The note's day, or undefined when the entity has none.
  iso: string | undefined
  // The reason `next` can't be this note's day, or null. It reads the cached list,
  // which can be stale; the server's unique date index is the real check.
  validate: (next: string) => string | null
  // The server's last refusal for this note, cleared by the next move.
  error: string | null
  // Moves the note to `next`: a plain `date` write, which the server answers by
  // renaming the note for the new day.
  move: (next: string) => void
}

// A day can hold only one daily note. The cached-list check and the server's
// AlreadyExists answer give the same message.
export function useDailyNoteDate(entity: Entity): DailyNoteDate {
  // Keyed by entity id so an error doesn't follow the field to another note.
  const [serverError, setServerError] = useState<{ id: string; message: string } | null>(null)
  const { setDate } = usePropertyWriters({
    onError: (err, failed) => {
      const message = isAlreadyExists(err) ? DAY_TAKEN : "Couldn't move the daily note. Try again."
      setServerError({ id: failed.id, message })
    },
  })

  const validate = (next: string) => {
    const occupant = dailyNoteByDate(getEntitiesSnapshot(), next)
    return occupant && occupant.id !== entity.id ? DAY_TAKEN : null
  }

  const move = (next: string) => {
    setServerError(null)
    setDate(entity, 'date', next)
  }

  return {
    iso: dailyNoteDate(entity),
    validate,
    error: serverError?.id === entity.id ? serverError.message : null,
    move,
  }
}
