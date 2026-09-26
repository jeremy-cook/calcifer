import { useCallback, useEffect, useRef, useState } from 'react'
import type { JSONContent } from '@tiptap/core'
import type { RichTextRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'
import { isNewerRichText, usePutRichText, useRichText, type RichTextState } from '~/model/richtext'

export const RICHTEXT_DEBOUNCE_MS = 300

export interface EntityRichTextFieldProps {
  propertyId: string
  propertyRef: RichTextRef
  autoFocus?: boolean
  hideToolbar?: boolean
}

export function EntityRichTextField({ propertyId, propertyRef, autoFocus, hideToolbar }: EntityRichTextFieldProps) {
  const { data, isError } = useRichText(propertyRef)

  // The editor needs the saved doc and its version before it mounts (the key
  // remounts cleanly per entity/property). Once it has data, a failed background
  // refetch keeps that data, so the open editor stays mounted.
  if (!data && isError) return <p className="p-4 text-sm text-muted-foreground">Couldn't load this text.</p>
  if (!data) return <div className="flex h-full w-full flex-col border border-border" />

  return (
    <LiveRichTextEditor
      key={`${propertyRef.entityId}:${propertyId}`}
      propertyRef={propertyRef}
      initial={data}
      autoFocus={autoFocus}
      hideToolbar={hideToolbar}
    />
  )
}

interface LiveRichTextEditorProps {
  propertyRef: RichTextRef
  initial: RichTextState
  autoFocus?: boolean
  hideToolbar?: boolean
}

interface SyncedVersion {
  // The server version the editor's content is based on.
  updatedAt: RichTextState['updatedAt']
  // The doc to push into the editor; bumping `version` pushes it.
  doc: string
  version: number
}

// Keeps an open editor in step with the server: debounced conditional saves,
// outside changes (e.g. an agent's append) loaded while there are no local
// edits in play, and a reload plus notice when a save loses a conflict.
function LiveRichTextEditor({ propertyRef, initial, autoFocus, hideToolbar }: LiveRichTextEditorProps) {
  const { data = initial } = useRichText(propertyRef)
  const [synced, setSynced] = useState<SyncedVersion>({ updatedAt: initial.updatedAt, doc: initial.doc, version: 0 })
  // True while a debounced save is waiting to fire.
  const [dirty, setDirty] = useState(false)
  const [conflicted, setConflicted] = useState(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const handleSaved = useCallback((saved: RichTextState) => {
    // Our own save: the editor already shows it, so only the base moves.
    setSynced((s) => (isNewerRichText(saved.updatedAt, s.updatedAt) ? { ...s, updatedAt: saved.updatedAt } : s))
  }, [])

  const handleConflict = useCallback((latest: RichTextState) => {
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = null
    setDirty(false)
    setConflicted(true)
    setSynced((s) => ({ updatedAt: latest.updatedAt, doc: latest.doc, version: s.version + 1 }))
  }, [])

  const { save, adopt, busy } = usePutRichText(propertyRef, {
    initialBase: initial.updatedAt,
    onSaved: handleSaved,
    onConflict: handleConflict,
  })

  // Load a newer cached version (from Watch) only when no local save is
  // pending or in flight; otherwise the next save's conflict check covers it.
  if (!dirty && !busy && isNewerRichText(data.updatedAt, synced.updatedAt)) {
    setSynced({ updatedAt: data.updatedAt, doc: data.doc, version: synced.version + 1 })
  }

  // The next save expects the version now in the editor.
  useEffect(() => {
    adopt(synced.updatedAt)
  }, [adopt, synced.updatedAt])

  const handleUpdate = useCallback(
    (json: JSONContent) => {
      setConflicted(false)
      setDirty(true)
      if (timerRef.current) clearTimeout(timerRef.current)
      timerRef.current = setTimeout(() => {
        timerRef.current = null
        setDirty(false)
        // Link/date derivation happens server-side inside Put — the FE just saves the doc.
        save(JSON.stringify(json))
      }, RICHTEXT_DEBOUNCE_MS)
    },
    [save],
  )

  const notice = conflicted ? (
    <p role="status" className="border-b border-border px-4 py-1 text-xs text-muted-foreground">
      Updated elsewhere; your latest edit wasn't saved.
    </p>
  ) : null

  return (
    <TiptapEditor
      doc={initial.doc}
      externalDoc={synced.doc}
      externalVersion={synced.version}
      onUpdate={handleUpdate}
      autoFocus={autoFocus}
      hideToolbar={hideToolbar}
      notice={notice}
    />
  )
}
