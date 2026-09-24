import { Badge } from '~/components/ui/badge'
import { optionLabel } from '~/model/structures'
import { TODO_PRIORITY_OPTIONS, type TodoPriority } from '~/model/todos'

export interface PriorityBadgeProps {
  priority: TodoPriority
}

const VARIANT: Record<Exclude<TodoPriority, 'none'>, 'secondary' | 'outline' | 'destructive'> = {
  low: 'outline',
  medium: 'secondary',
  high: 'destructive',
}

export function PriorityBadge({ priority }: PriorityBadgeProps) {
  if (priority === 'none') return null
  return <Badge variant={VARIANT[priority]}>{optionLabel(TODO_PRIORITY_OPTIONS, priority)}</Badge>
}
