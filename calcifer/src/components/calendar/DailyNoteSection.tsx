import { useEffect, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { ArrowsOutSimpleIcon, TrashIcon } from '@phosphor-icons/react'
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
import {
  dailyNoteByDate,
  deleteEntityImperative,
  getEntitiesSnapshot,
  useCreateDailyNote,
  useDeleteEntity,
  type Entity,
} from '~/model/store'
import { itemsOn, useCalendarIndex } from '~/model/calendar'
import { getRichTextSnapshot, isRichTextEmpty } from '~/model/richtext'
import type { RichTextRef } from '@calcifer/proto/calcifer/v1/entities_pb'

interface DailyNoteSectionProps {
  iso: string
}

export function DailyNoteSection({ iso }: DailyNoteSectionProps) {
  const dailyNote = itemsOn(useCalendarIndex(), iso, 'dailyNote')[0]?.entity
  const createDailyNote = useCreateDailyNote()
  const [autoFocusKey, setAutoFocusKey] = useState<string | null>(null)
  if (autoFocusKey !== null && autoFocusKey !== iso) setAutoFocusKey(null)

  const handleCreate = () => {
    void createDailyNote(iso)
    setAutoFocusKey(iso)
  }

  useEffect(() => {
    return () => {
      // Prune an empty daily note when leaving its day (reads the live cache).
      const existing = dailyNoteByDate(getEntitiesSnapshot(), iso)
      if (!existing) return
      const ref = contentRichTextRef(existing)
      const doc = ref ? getRichTextSnapshot(ref) : undefined
      if (isRichTextEmpty(doc)) {
        deleteEntityImperative(existing.id)
      }
    }
  }, [iso])

  return (
    <section className="flex flex-col gap-3">
      <header className="flex items-center justify-between">
        <h2 className="text-base font-semibold">Daily note</h2>
        {dailyNote ? <DailyNoteActions dailyNote={dailyNote} /> : <EmptyState onClick={handleCreate} />}
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
      <Button variant={'ghost'} onClick={onClick}>
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
  const contentRef = contentRichTextRef(dailyNote)
  if (!contentRef) return null
  return <EntityRichTextField propertyId="content" propertyRef={contentRef} autoFocus={autoFocus} hideToolbar={true} />
}

interface DailyNoteActionsProps {
  dailyNote: Entity
}

function DailyNoteActions({ dailyNote }: DailyNoteActionsProps) {
  const deleteEntity = useDeleteEntity()
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
        className="rounded p-1 hover:bg-muted hover:text-foreground"
      >
        <ArrowsOutSimpleIcon className="size-4" />
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
            <AlertDialogAction onClick={handleDelete}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

function contentRichTextRef(entity: Entity): RichTextRef | undefined {
  const property = entity.properties.find((p) => p.id === 'content')
  const value = property?.value?.value
  return value?.case === 'richtext' ? value.value : undefined
}
