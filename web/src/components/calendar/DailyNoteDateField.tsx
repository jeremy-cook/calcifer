import { EntityDateField } from '~/components/entity/EntityDateField'
import { useDailyNoteDate } from '~/model/daily-notes'
import type { Entity } from '~/model/store'

export interface DailyNoteDateFieldProps {
  entity: Entity
}

export function DailyNoteDateField({ entity }: DailyNoteDateFieldProps) {
  const { iso, validate, error, move } = useDailyNoteDate(entity)
  if (!iso) return null
  return <EntityDateField label="Date" showLabel={false} iso={iso} validate={validate} error={error} onChange={move} />
}
