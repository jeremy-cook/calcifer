import { EntityDateField } from '~/components/entity/EntityDateField'
import { dailyNoteByDate, dailyNoteDate, useAllEntities, useUpdateEntity, withDailyNoteDate, type Entity } from '~/model/store'

export interface DailyNoteDateFieldProps {
  entity: Entity
}

// Moving a daily note renames it, and a day can hold only one daily note.
export function DailyNoteDateField({ entity }: DailyNoteDateFieldProps) {
  const entities = useAllEntities()
  const updateEntity = useUpdateEntity()
  const iso = dailyNoteDate(entity)
  if (!iso) return null

  const validate = (next: string) => {
    const occupant = dailyNoteByDate(entities, next)
    return occupant && occupant.id !== entity.id ? 'A daily note already exists for that day.' : null
  }

  return (
    <EntityDateField
      label="Date"
      iso={iso}
      validate={validate}
      onChange={(next) => updateEntity(withDailyNoteDate(entity, next))}
    />
  )
}
