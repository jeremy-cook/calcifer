import { Badge } from '~/components/ui/badge'
import { optionLabel, propertyDef, useFacet } from '~/model/structures'
import { TODO_FACET, TODO_PRIORITY, type TodoPriority } from '~/model/todos'

export interface PriorityBadgeProps {
  priority: TodoPriority
}

export function PriorityBadge({ priority }: PriorityBadgeProps) {
  const priorityDef = propertyDef(useFacet(TODO_FACET), TODO_PRIORITY)
  if (priority === 'none') return null

  // Presentation only: options without an entry (e.g. newly added on the
  // server) get the neutral outline badge.
  const variants: Record<string, 'secondary' | 'outline' | 'destructive'> = {
    low: 'outline',
    medium: 'secondary',
    high: 'destructive',
  }

  return <Badge variant={variants[priority] ?? 'outline'}>{optionLabel(priorityDef, priority)}</Badge>
}
