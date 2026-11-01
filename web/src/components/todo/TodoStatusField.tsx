import { Checkbox } from '~/components/ui/checkbox'
import { PILL_CLASS } from '~/components/entity/property-pill'
import { todoFields, useSetTodoStatus } from '~/model/todos'
import type { Entity } from 'calcifer/v1/entities_pb'

export interface TodoStatusFieldProps {
  entity: Entity
}

export function TodoStatusField({ entity }: TodoStatusFieldProps) {
  const setTodoStatus = useSetTodoStatus()
  const { status } = todoFields(entity)

  const handleCheckedChange = (checked: boolean | 'indeterminate') => {
    setTodoStatus(entity, checked === true ? 'done' : 'open')
  }

  return (
    <label className={PILL_CLASS}>
      <Checkbox checked={status === 'done'} onCheckedChange={handleCheckedChange} aria-label="Done" />
      {status === 'done' ? 'Done' : 'Open'}
    </label>
  )
}
