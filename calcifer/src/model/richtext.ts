import { useCallback } from 'react'
import { create as createMessage } from '@bufbuild/protobuf'
import { useMutation, useQuery } from '@tanstack/react-query'
import { RichTextSchema, type RichTextRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { isNotFound, qk, queryClient, richTextClient } from '~/model/api'

export function richTextKey(ref: RichTextRef): string {
  return `${ref.entityId}:${ref.propertyId}`
}

interface RichTextDocNode {
  type?: string
  content?: RichTextDocNode[]
}

export function isRichTextEmpty(doc: string | undefined): boolean {
  if (!doc) return true
  let parsed: RichTextDocNode
  try {
    parsed = JSON.parse(doc) as RichTextDocNode
  } catch {
    return false
  }
  const content = parsed.content
  if (!content || content.length === 0) return true
  if (content.length > 1) return false
  const only = content[0]
  if (only.type !== 'paragraph') return false
  return !only.content || only.content.length === 0
}

// Returns the doc JSON string; a not-yet-saved doc reads as '' (NOT_FOUND).
export function useRichText(ref: RichTextRef) {
  return useQuery({
    queryKey: qk.richtext(ref.entityId, ref.propertyId),
    queryFn: async () => {
      try {
        const rt = await richTextClient.get({ entityId: ref.entityId, propertyId: ref.propertyId })
        return rt.doc
      } catch (err) {
        if (isNotFound(err)) return ''
        throw err
      }
    },
    enabled: !!ref.entityId,
  })
}

export function usePutRichText() {
  const m = useMutation({
    mutationFn: ({ ref, doc }: { ref: RichTextRef; doc: string }) =>
      richTextClient.put(createMessage(RichTextSchema, { ref, doc })),
    onSuccess: (saved) => {
      const ref = saved.ref
      if (ref) {
        queryClient.setQueryData(qk.richtext(ref.entityId, ref.propertyId), saved.doc)
        // Put derived this entity's links + referenced_dates server-side, which
        // also changes targets' backlinks — refresh the source entity and the
        // list that backlinks/calendar derive from.
        void queryClient.invalidateQueries({ queryKey: qk.entity(ref.entityId) })
      }
      void queryClient.invalidateQueries({ queryKey: ['entities'] })
    },
  })
  return useCallback((ref: RichTextRef, doc: string) => m.mutate({ ref, doc }), [m])
}

export function getRichTextSnapshot(ref: RichTextRef): string | undefined {
  return queryClient.getQueryData<string>(qk.richtext(ref.entityId, ref.propertyId))
}
