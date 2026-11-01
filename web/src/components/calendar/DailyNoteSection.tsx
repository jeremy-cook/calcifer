import { useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { ArrowUpRightIcon, TrashIcon } from '@phosphor-icons/react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '~/components/ui/alert-dialog'
import { Button } from '~/components/ui/button'
import { PlusIcon } from '@phosphor-icons/react'
import { EntityRichTextField } from '~/components/entity/EntityRichTextField'
import { deleteEntity, useReplicaReady, useResolveDailyNote, type Entity } from '~/model/store'
import { itemsOn, useCalendarIndex } from '~/model/calendar'
import { dailyNoteContentRef, useDailyNotePrune } from '~/model/daily-notes'
import { structurePresentation } from '~/model/structures'

interface DailyNoteSectionProps {
  iso: string
}

export function DailyNoteSection({ iso }: DailyNoteSectionProps) {
  const dailyNote = itemsOn(useCalendarIndex(), iso, 'dailyNote')[0]?.entity
  const resolveDailyNote = useResolveDailyNote()
  const ready = useReplicaReady()
  const [autoFocusKey, setAutoFocusKey] = useState<string | null>(null)
  if (autoFocusKey !== null && autoFocusKey !== iso) setAutoFocusKey(null)

  const handleCreate = () => {
    void resolveDailyNote(iso)
    setAutoFocusKey(iso)
  }

  useDailyNotePrune(iso)

  const { icon: DailyNoteIcon, color } = structurePresentation('DailyNote')

  // Before the first snapshot there's no telling whether the day has a note, so
  // offer nothing rather than flash "New".
  const renderActions = () => {
    if (dailyNote) return <DailyNoteActions dailyNote={dailyNote} />
    if (!ready) return null
    return <EmptyState onClick={handleCreate} />
  }

  return (
    <section className="flex flex-col border-t border-border">
      <header className="flex h-12 items-center justify-between px-7 @3xl:px-16 pt-2">
        <h2 className="flex items-center gap-2 text-xs font-bold tracking-wider uppercase" style={{ color }}>
          <DailyNoteIcon weight="duotone" className="size-4" aria-hidden />
          Daily note
        </h2>
        {renderActions()}
      </header>
      {dailyNote && <DailyNoteBody dailyNote={dailyNote} autoFocus={autoFocusKey === iso} />}
    </section>
  )
}

interface EmptyStateProps {
  onClick: () => void
}

function EmptyState({ onClick }: EmptyStateProps) {
  return (
    <div>
      <Button variant="outline" size="sm" onClick={onClick}>
        <PlusIcon />
        New
      </Button>
    </div>
  )
}

interface DailyNoteBodyProps {
  dailyNote: Entity
  autoFocus: boolean
}

function DailyNoteBody({ dailyNote, autoFocus }: DailyNoteBodyProps) {
  const contentRef = dailyNoteContentRef(dailyNote)
  if (!contentRef) return null
  return (
    <EntityRichTextField
      entityId={contentRef.entityId}
      propertyId={contentRef.propertyId}
      autoFocus={autoFocus}
      hideToolbar={true}
      className="border-0 [&_.tiptap]:pt-2"
    />
  )
}

interface DailyNoteActionsProps {
  dailyNote: Entity
}

function DailyNoteActions({ dailyNote }: DailyNoteActionsProps) {
  const navigate = useNavigate()

  const handleExpand = () => {
    void navigate({ to: '/e/$id', params: { id: dailyNote.id } })
  }

  const handleDelete = () => {
    deleteEntity(dailyNote.id)
  }

  return (
    <div className="flex items-center gap-1 text-muted-foreground">
      <button
        type="button"
        aria-label="Open daily note"
        onClick={handleExpand}
        className="flex items-center gap-1 rounded px-1.5 py-1 text-sm hover:bg-muted hover:text-foreground"
      >
        Open
        <ArrowUpRightIcon className="size-3.5" />
      </button>
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <button
            type="button"
            aria-label="Delete daily note"
            className="rounded p-1 hover:bg-muted hover:text-foreground"
          >
            <TrashIcon className="size-4" />
          </button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete daily note?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete the daily note for "{dailyNote.name}" and its content.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={handleDelete}>
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
