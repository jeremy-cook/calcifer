import { useState } from 'react'
import { Code, ConnectError } from '@connectrpc/connect'
import { EntityDateField } from '~/components/entity/EntityDateField'
import { dailyNoteByDate, dailyNoteDate, useAllEntities, useSetProperty, type Entity } from '~/model/store'

export interface DailyNoteDateFieldProps {
  entity: Entity
}

// A day can hold only one daily note. Moving one is a plain `date` write; the
// server renames it for the new day, and the saved entity carries the new name.
export function DailyNoteDateField({ entity }: DailyNoteDateFieldProps) {
  const entities = useAllEntities()
  // Keyed by entity id so an error doesn't follow the field to another note.
  const [serverError, setServerError] = useState<{ id: string; message: string } | null>(null)
  // validate uses the cached list, which can be stale; the server's unique
  // date index is the real check and answers AlreadyExists on a collision.
  const setProperty = useSetProperty({
    onError: (err, failed) => {
      const collided = err instanceof ConnectError && err.code === Code.AlreadyExists
      const message = collided
        ? 'A daily note already exists for that day.'
        : "Couldn't move the daily note. Try again."
      setServerError({ id: failed.id, message })
    },
  })
  const iso = dailyNoteDate(entity)
  if (!iso) return null

  const validate = (next: string) => {
    const occupant = dailyNoteByDate(entities, next)
    return occupant && occupant.id !== entity.id ? 'A daily note already exists for that day.' : null
  }

  const handleChange = (next: string) => {
    setServerError(null)
    setProperty(entity, 'date', { case: 'date', value: next })
  }

  const error = serverError?.id === entity.id ? serverError.message : null

  return <EntityDateField label="Date" iso={iso} validate={validate} error={error} onChange={handleChange} />
}
