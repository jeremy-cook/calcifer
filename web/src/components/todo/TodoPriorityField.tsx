import { EntitySelectField } from '~/components/entity/EntitySelectField'
import { usePropertyWriters, type Entity } from '~/model/store'
import { propertyDef, useFacet } from '~/model/structures'
import { TODO_FACET, TODO_PRIORITY, todoFields } from '~/model/todos'
import { PRIORITY_CLASSES, PRIORITY_ICONS } from './priority'

export interface TodoPriorityFieldProps {
  entity: Entity
}

// The priority pill, with a signal-bars icon for its level; high reads in the alert colour.
// It shows in the ToDo band, under its label, so empty it reads "+ Set".
export function TodoPriorityField({ entity }: TodoPriorityFieldProps) {
  const { setSelect } = usePropertyWriters()
  const def = propertyDef(useFacet(TODO_FACET), TODO_PRIORITY)
  const { priority } = todoFields(entity)
  if (!def || def.options.length === 0) return null

  return (
    <EntitySelectField
      label={def.label || 'Priority'}
      value={priority}
      options={def.options}
      emptyValue={def.defaultOption}
      emptyLabel="Set"
      icon={PRIORITY_ICONS[priority]}
      optionIcons={PRIORITY_ICONS}
      optionClasses={PRIORITY_CLASSES}
      className={priority === 'high' ? 'text-destructive' : undefined}
      onChange={(next) => setSelect(entity, TODO_PRIORITY, next)}
    />
  )
}
