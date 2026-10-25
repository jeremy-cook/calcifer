import { Checkbox } from '~/components/ui/checkbox'
import { todoFields, useSetTodoStatus } from '~/model/todos'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

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
    <label className="flex items-center gap-3 px-12 py-3">
      <span className="w-24 text-sm text-muted-foreground">Status</span>
      <span className="flex items-center gap-2">
        <Checkbox checked={status === 'done'} onCheckedChange={handleCheckedChange} />
        <span className="text-sm">Done</span>
      </span>
    </label>
  )
}
