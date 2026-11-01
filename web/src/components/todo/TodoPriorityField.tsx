import { CellSignalHighIcon, CellSignalLowIcon, CellSignalMediumIcon, type Icon } from '@phosphor-icons/react'
import { EntitySelectField } from '~/components/entity/EntitySelectField'
import { usePropertyWriters, type Entity } from '~/model/store'
import { propertyDef, useFacet } from '~/model/structures'
import { TODO_FACET, TODO_PRIORITY, todoFields } from '~/model/todos'

export interface TodoPriorityFieldProps {
  entity: Entity
}

// The priority pill, with a signal-bars icon for its level; high reads in the alert colour.
export function TodoPriorityField({ entity }: TodoPriorityFieldProps) {
  const { setSelect } = usePropertyWriters()
  const def = propertyDef(useFacet(TODO_FACET), TODO_PRIORITY)
  const { priority } = todoFields(entity)
  if (!def || def.options.length === 0) return null

  // Presentation only: an option without an entry (e.g. newly added on the server) gets no icon.
  const icons: Record<string, Icon> = {
    low: CellSignalLowIcon,
    medium: CellSignalMediumIcon,
    high: CellSignalHighIcon,
  }

  return (
    <EntitySelectField
      label={def.label || 'Priority'}
      value={priority}
      options={def.options}
      emptyValue={def.defaultOption}
      icon={icons[priority]}
      className={priority === 'high' ? 'text-destructive' : undefined}
      onChange={(next) => setSelect(entity, TODO_PRIORITY, next)}
    />
  )
}
