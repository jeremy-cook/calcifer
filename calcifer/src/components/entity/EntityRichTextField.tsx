import { useCallback, useRef } from 'react'
import type { JSONContent } from '@tiptap/core'
import type { RichTextRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'
import { richTextKey, useRichTextStore } from '~/model/richtext'
import { syncLinksFromDoc } from '~/model/linkSync'

export const RICHTEXT_DEBOUNCE_MS = 300

export interface EntityRichTextFieldProps {
  propertyId: string
  propertyRef: RichTextRef
  autoFocus?: boolean
  hideToolbar?: boolean
}

export function EntityRichTextField({ propertyId, propertyRef, autoFocus, hideToolbar }: EntityRichTextFieldProps) {
  const doc = useRichTextStore((s) => s.docs[richTextKey(propertyRef)]?.doc ?? '')
  const putRichText = useRichTextStore((s) => s.putRichText)

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const handleUpdate = useCallback(
    (json: JSONContent) => {
      if (timerRef.current) clearTimeout(timerRef.current)
      timerRef.current = setTimeout(() => {
        putRichText(propertyRef, JSON.stringify(json))
        syncLinksFromDoc(propertyRef.entityId, propertyId, json)
      }, RICHTEXT_DEBOUNCE_MS)
    },
    [propertyId, propertyRef, putRichText],
  )

  return (
    <TiptapEditor
      key={`${propertyRef.entityId}:${propertyId}`}
      doc={doc}
      onUpdate={handleUpdate}
      autoFocus={autoFocus}
      hideToolbar={hideToolbar}
    />
  )
}
