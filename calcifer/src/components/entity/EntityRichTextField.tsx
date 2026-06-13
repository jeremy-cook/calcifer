import { useCallback, useRef } from 'react'
import type { JSONContent } from '@tiptap/core'
import type { RichTextRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'
import { useRichText, usePutRichText } from '~/model/richtext'

export const RICHTEXT_DEBOUNCE_MS = 300

export interface EntityRichTextFieldProps {
  propertyId: string
  propertyRef: RichTextRef
  autoFocus?: boolean
  hideToolbar?: boolean
}

export function EntityRichTextField({ propertyId, propertyRef, autoFocus, hideToolbar }: EntityRichTextFieldProps) {
  const { data: doc, isPending } = useRichText(propertyRef)
  const putRichText = usePutRichText()

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const handleUpdate = useCallback(
    (json: JSONContent) => {
      if (timerRef.current) clearTimeout(timerRef.current)
      timerRef.current = setTimeout(() => {
        // Link/date derivation happens server-side inside Put — the FE just saves the doc.
        putRichText(propertyRef, JSON.stringify(json))
      }, RICHTEXT_DEBOUNCE_MS)
    },
    [propertyRef, putRichText],
  )

  // TiptapEditor takes `doc` as initial content only, so wait for the query to
  // resolve before mounting it (the key remounts cleanly per entity/property).
  if (isPending) return <div className="flex h-full w-full flex-col border border-border" />

  return (
    <TiptapEditor
      key={`${propertyRef.entityId}:${propertyId}`}
      doc={doc ?? ''}
      onUpdate={handleUpdate}
      autoFocus={autoFocus}
      hideToolbar={hideToolbar}
    />
  )
}
