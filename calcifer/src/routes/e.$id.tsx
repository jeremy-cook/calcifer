import { useRef, useCallback } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { TrashIcon } from '@phosphor-icons/react'
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
import { Input } from '~/components/ui/input'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'
import type { Entity } from '~/model/store'
import { useEntityStore } from '~/model/store'
import { richTextKey, useRichTextStore } from '~/model/richtext'
import { STRUCTURES, type StructureId } from '~/model/structures'
import type { Property, RichTextRef } from '@calcifer/proto/calcifer/v1/entities_pb'

export const Route = createFileRoute('/e/$id')({
  component: function EntityRoute() {
    const { id } = Route.useParams()
    const entity = useEntityStore((s) => s.entities[id])
    if (!entity) return <EntityNotFound />

    return (
      <div className="flex h-full flex-col">
        <EntityHeader entity={entity} />
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EntityProperties entity={entity} />
        </div>
      </div>
    )
  },
})

function EntityNotFound() {
  return (
    <div className="flex flex-col gap-2 px-16 py-10">
      <h1 className="text-2xl font-semibold">Entity not found</h1>
      <p className="text-sm text-muted-foreground">This entity doesn't exist or was deleted.</p>
    </div>
  )
}

interface EntityHeaderProps {
  entity: Entity
}

function EntityHeader({ entity }: EntityHeaderProps) {
  const updateEntity = useEntityStore((s) => s.updateEntity)
  const deleteEntity = useEntityStore((s) => s.deleteEntity)
  const navigate = useNavigate()

  const structureName = STRUCTURES[entity.structureId as StructureId]?.name ?? entity.structureId
  const isDefaultTitle = entity.title === `Untitled ${structureName}`

  const handleDelete = () => {
    deleteEntity(entity.id)
    void navigate({ to: '/' })
  }

  return (
    <div className="flex items-center gap-2 border-b border-border px-12 py-6">
      <Input
        value={entity.title}
        autoFocus={isDefaultTitle}
        onChange={(e) => updateEntity(entity.id, { title: e.target.value })}
        className="h-10 border-0 bg-transparent text-2xl font-semibold shadow-none focus-visible:ring-0"
        aria-label="Entity title"
      />
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button variant="ghost" size="icon" aria-label="Delete entity">
            <TrashIcon />
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {structureName.toLowerCase()}?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{entity.title}" and its content.
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

interface EntityPropertiesProps {
  entity: Entity
}

function EntityProperties({ entity }: EntityPropertiesProps) {
  if (entity.properties.length === 0) return null

  const renderProperty = (property: Property) => {
    const value = property.value?.value
    if (!value) return null
    switch (value.case) {
      case 'richtext':
        return <EntityRichTextField key={property.id} propertyId={property.id} propertyRef={value.value} />
      default:
        return null
    }
  }

  return <>{entity.properties.map(renderProperty)}</>
}

interface EntityRichTextFieldProps {
  propertyId: string
  propertyRef: RichTextRef
}

const RICHTEXT_DEBOUNCE_MS = 300

function EntityRichTextField({ propertyId, propertyRef }: EntityRichTextFieldProps) {
  const doc = useRichTextStore((s) => s.docs[richTextKey(propertyRef)]?.doc ?? '')
  const putRichText = useRichTextStore((s) => s.putRichText)

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const handleUpdate = useCallback((next: string) => {
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => putRichText(propertyRef, next), RICHTEXT_DEBOUNCE_MS)
  }, [propertyRef, putRichText])

  return (
    <TiptapEditor
      key={`${propertyRef.entityId}:${propertyId}`}
      doc={doc}
      onUpdate={handleUpdate}
    />
  )
}
