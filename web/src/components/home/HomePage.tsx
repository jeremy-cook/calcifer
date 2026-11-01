import { useEffect, useMemo } from 'react'
import { Link } from '@tanstack/react-router'
import { formatDistanceToNow } from 'date-fns'
import { ArrowUpRightIcon, ClockCounterClockwiseIcon, FeatherIcon, type Icon } from '@phosphor-icons/react'
import { EmptyLine } from '~/components/empty-state/EmptyLine'
import { EntityRichTextField } from '~/components/entity/EntityRichTextField'
import { EntityRows } from '~/components/structure-list/EntityRows'
import { HomeTodos } from '~/components/home/HomeTodos'
import { itemsOn, useCalendarIndex } from '~/model/calendar'
import { formatDayTitle, timestampMsOrZero, useToday, weekNumber } from '~/model/dates'
import { dailyNoteContentRef, useDailyNotePrune } from '~/model/daily-notes'
import { entityUpdatedAtDate, useAllEntities, useReplicaReady, useResolveDailyNote } from '~/model/store'
import { facetPresentation, structurePresentation } from '~/model/structures'
import { TODO_FACET } from '~/model/todos'

// Today's daily note, ready to write in, with ToDos due this week and recently edited
// notes beside it.
export function HomePage() {
  const todo = facetPresentation(TODO_FACET)

  return (
    <div className="grid h-full grid-cols-[minmax(0,1fr)_340px]">
      <TodayNote />
      <aside className="flex flex-col gap-6 overflow-y-auto border-l border-border bg-card/60 px-5 py-7">
        <SideSection icon={todo.icon} color={todo.color} title="ToDos" link={{ to: '/v/todos', label: 'All ToDos' }}>
          <HomeTodos />
        </SideSection>
        <SideSection icon={ClockCounterClockwiseIcon} color="var(--chart-1)" title="Recently edited">
          <RecentlyEdited />
        </SideSection>
      </aside>
    </div>
  )
}

// Home is for writing, so today's note is made on arrival; leaving with it still
// empty prunes it, as on the calendar.
function TodayNote() {
  const today = useToday()
  const dailyNote = itemsOn(useCalendarIndex(), today, 'dailyNote')[0]?.entity
  const ready = useReplicaReady()
  const resolveDailyNote = useResolveDailyNote()
  const { icon: DailyNoteIcon, color } = structurePresentation('DailyNote')

  useDailyNotePrune(today)

  useEffect(() => {
    if (ready && !dailyNote) void resolveDailyNote(today)
  }, [ready, dailyNote, today, resolveDailyNote])

  const contentRef = dailyNote ? dailyNoteContentRef(dailyNote) : undefined
  const details = [
    `Week ${weekNumber(today)}`,
    dailyNote ? `edited ${formatDistanceToNow(entityUpdatedAtDate(dailyNote), { addSuffix: true })}` : null,
  ].filter((detail) => detail !== null)

  return (
    <div className="flex min-h-0 flex-col">
      <header className="flex flex-col gap-1.5 px-16 pt-7 pb-5">
        <div className="flex h-7 items-center gap-2 text-xs font-bold tracking-wider text-muted-foreground uppercase">
          <DailyNoteIcon weight="duotone" className="size-4.5 shrink-0" style={{ color }} aria-hidden />
          Daily note
          <span className="font-medium tracking-normal normal-case">· {details.join(' · ')}</span>
          {dailyNote && (
            <Link
              to="/e/$id"
              params={{ id: dailyNote.id }}
              className="ml-auto flex items-center gap-1 font-medium tracking-normal normal-case hover:text-foreground"
            >
              Open
              <ArrowUpRightIcon className="size-3.5" />
            </Link>
          )}
        </div>
        <h1 className="font-heading text-4xl leading-tight font-semibold text-heading">{formatDayTitle(today)}</h1>
      </header>
      <div className="min-h-0 flex-1">
        {contentRef && (
          <EntityRichTextField
            key={contentRef.entityId}
            entityId={contentRef.entityId}
            propertyId={contentRef.propertyId}
            hideToolbar
            className="border-x-0 border-b-0"
          />
        )}
      </div>
    </div>
  )
}

interface SideSectionProps {
  icon: Icon
  color: string
  title: string
  link?: { to: '/v/todos'; label: string }
  children: React.ReactNode
}

function SideSection({ icon: SectionIcon, color, title, link, children }: SideSectionProps) {
  return (
    <section className="flex flex-col gap-1.5">
      <h2 className="flex h-6 items-center gap-2 text-xs font-bold tracking-wider text-muted-foreground uppercase">
        <SectionIcon weight="duotone" className="size-4" style={{ color }} aria-hidden />
        {title}
        {link && (
          <Link to={link.to} className="ml-auto font-medium tracking-normal normal-case hover:text-foreground">
            {link.label} →
          </Link>
        )}
      </h2>
      {children}
    </section>
  )
}

function RecentlyEdited() {
  const entities = useAllEntities()

  const RECENT_COUNT = 6
  // Daily notes are left out: today's is on this page, and older ones are on the calendar.
  const recent = useMemo(
    () =>
      entities
        .filter((e) => e.structureType !== 'DailyNote')
        .sort((a, b) => timestampMsOrZero(b.updatedAt) - timestampMsOrZero(a.updatedAt))
        .slice(0, RECENT_COUNT),
    [entities],
  )

  if (recent.length === 0) {
    return (
      <EmptyLine icon={FeatherIcon} color="var(--chart-1)">
        Nothing yet. Notes you edit show up here.
      </EmptyLine>
    )
  }
  return <EntityRows entities={recent} />
}
