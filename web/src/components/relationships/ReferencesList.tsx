import { Link } from '@tanstack/react-router'
import { formatDistanceToNow } from 'date-fns'
import { CellSignalHighIcon, LinkSimpleIcon } from '@phosphor-icons/react'
import { EmptyLine } from '~/components/empty-state/EmptyLine'
import { LoadingRows } from '~/components/loading/LoadingRows'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import { formatShortDate, timestampMsOrZero, useToday } from '~/model/dates'
import { useIncomingReferences, type Reference } from '~/model/relationships'
import { entityUpdatedAtDate, hasFacet, useReplicaReady, type Entity } from '~/model/store'
import { facetPresentation, getFacet, structurePresentation, useStructures } from '~/model/structures'
import { TODO_FACET, todoFields, useSetTodoStatus } from '~/model/todos'

interface ReferencesListProps {
  entity: Entity
}

// Everything that refers to an entity with no text of its own (a tag): ToDos first,
// then each structure in registry order. A row there only because its text mentions the
// entity says "in text"; the rest name it in a relation property (Tags).
export function ReferencesList({ entity }: ReferencesListProps) {
  const references = useIncomingReferences(entity.id)
  const structures = useStructures()
  const ready = useReplicaReady()

  if (!ready) return <LoadingRows className="max-w-3xl" />
  if (references.length === 0) {
    return (
      <EmptyLine icon={LinkSimpleIcon} color="var(--muted-foreground)" className="px-0">
        Nothing refers to this yet.
      </EmptyLine>
    )
  }

  const todos = references
    .filter((r) => hasFacet(r.entity, TODO_FACET))
    .sort((a, b) => compareTodos(a.entity, b.entity))
  const others = references.filter((r) => !hasFacet(r.entity, TODO_FACET))
  const byRecent = (a: Reference, b: Reference) =>
    timestampMsOrZero(b.entity.updatedAt) - timestampMsOrZero(a.entity.updatedAt)

  const todo = facetPresentation(TODO_FACET)
  const groups = [
    { key: TODO_FACET, label: getFacet(TODO_FACET)?.plural ?? 'ToDos', ...todo, items: todos },
    ...structures.map((s) => ({
      key: s.type,
      label: s.plural,
      ...structurePresentation(s.type),
      items: others.filter((r) => r.entity.structureType === s.type).sort(byRecent),
    })),
  ].filter((group) => group.items.length > 0)

  return (
    <div className="flex flex-col gap-4">
      {groups.map(({ key, label, icon: Icon, color, items }) => (
        <section key={key} className="flex max-w-3xl flex-col gap-1">
          <h2 className="flex h-6 items-center gap-2 text-xs font-bold tracking-wider text-muted-foreground uppercase">
            <Icon weight="duotone" className="size-4" style={{ color }} aria-hidden />
            {label}
            <span className="font-medium tracking-normal normal-case">· {items.length}</span>
          </h2>
          <ul className="flex flex-col">
            {items.map((reference) => (
              <ReferenceRow key={reference.entity.id} reference={reference} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  )
}

// Open before done, then by due date (none last), then by name.
function compareTodos(a: Entity, b: Entity): number {
  const af = todoFields(a)
  const bf = todoFields(b)
  const done = Number(af.status === 'done') - Number(bf.status === 'done')
  if (done !== 0) return done
  if (af.due !== bf.due) return (af.due ?? '9999') < (bf.due ?? '9999') ? -1 : 1
  return a.name.localeCompare(b.name)
}

const ROW_CLASS = 'flex h-[30px] items-center gap-2.5 rounded-[5px] px-2.5 text-sm even:bg-card hover:bg-accent'

interface ReferenceRowProps {
  reference: Reference
}

function ReferenceRow({ reference }: ReferenceRowProps) {
  const { entity, inTextOnly } = reference
  const pill = inTextOnly && (
    <span className="shrink-0 rounded-full border border-chart-2/30 px-2 text-xs text-chart-2">in text</span>
  )

  if (hasFacet(entity, TODO_FACET)) return <TodoReferenceRow entity={entity} pill={pill} />

  const { icon: Icon, color } = structurePresentation(entity.structureType)
  return (
    <li className={ROW_CLASS}>
      <Icon weight="duotone" className="size-4 shrink-0" style={{ color }} aria-hidden />
      <Link to="/e/$id" params={{ id: entity.id }} className="min-w-0 flex-1 truncate">
        {entity.name}
      </Link>
      {pill}
      <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
        {formatDistanceToNow(entityUpdatedAtDate(entity), { addSuffix: true })}
      </span>
    </li>
  )
}

interface TodoReferenceRowProps {
  entity: Entity
  pill: React.ReactNode
}

function TodoReferenceRow({ entity, pill }: TodoReferenceRowProps) {
  const today = useToday()
  const setTodoStatus = useSetTodoStatus()
  const { status, priority, due } = todoFields(entity)
  const done = status === 'done'
  const overdue = !done && due !== undefined && due < today

  return (
    <li className={ROW_CLASS}>
      <Checkbox
        checked={done}
        onCheckedChange={(checked) => setTodoStatus(entity, checked === true ? 'done' : 'open')}
        aria-label={`Mark "${entity.name}" done`}
      />
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('min-w-0 flex-1 truncate', done && 'text-muted-foreground line-through')}
      >
        {entity.name}
      </Link>
      {priority === 'high' && !done && (
        <CellSignalHighIcon weight="fill" className="size-3.5 shrink-0 text-destructive" aria-label="High priority" />
      )}
      {pill}
      {due && (
        <span
          className={cn(
            'shrink-0 text-xs text-muted-foreground tabular-nums',
            overdue && 'font-semibold text-destructive',
          )}
        >
          {due === today ? 'Today' : formatShortDate(due)}
        </span>
      )}
    </li>
  )
}
