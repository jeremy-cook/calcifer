import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Timestamp } from '@bufbuild/protobuf/wkt'
import type { JSONContent } from '@tiptap/core'
import type { RichTextRef } from 'calcifer/v1/entities_pb'
import { LoadingText } from '~/components/loading/LoadingText'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'
import { cn } from '~/lib/utils'
import { isNewerRichText, richTextRef, usePutRichText, useRichText, type RichTextState } from '~/model/richtext'

export const RICHTEXT_DEBOUNCE_MS = 300

export interface EntityRichTextFieldProps {
  entityId: string
  // A rich-text property the entity's structure declares; the doc is addressed
  // by (entityId, propertyId), not by a stored property value.
  propertyId: string
  autoFocus?: boolean
  hideToolbar?: boolean
  // Extra classes for the editor's frame.
  className?: string
}

export function EntityRichTextField({
  entityId,
  propertyId,
  autoFocus,
  hideToolbar,
  className,
}: EntityRichTextFieldProps) {
  const propertyRef = useMemo(() => richTextRef(entityId, propertyId), [entityId, propertyId])
  const { data, isError } = useRichText(propertyRef)

  // The editor needs the saved doc and its version before it mounts (the key
  // remounts cleanly per entity/property). Once it has data, a failed background
  // refetch keeps that data, so the open editor stays mounted.
  if (!data && isError) return <p className="p-4 text-sm text-muted-foreground">Couldn't load this text.</p>
  if (!data) {
    return (
      <div className={cn('flex h-full w-full flex-col border border-border', className)}>
        <LoadingText className="px-16 py-10" />
      </div>
    )
  }

  return (
    <LiveRichTextEditor
      key={`${entityId}:${propertyId}`}
      propertyRef={propertyRef}
      initial={data}
      autoFocus={autoFocus}
      hideToolbar={hideToolbar}
      className={className}
    />
  )
}

interface LiveRichTextEditorProps {
  propertyRef: RichTextRef
  initial: RichTextState
  autoFocus?: boolean
  hideToolbar?: boolean
  className?: string
}

interface SyncedVersion {
  // The server version `doc` is based on; moves up when our own save lands.
  updatedAt: RichTextState['updatedAt']
  // The doc to push into the editor, and to start a fresh editor from; bumping
  // `version` pushes it into one that is open.
  doc: string
  version: number
  // The server version this `version` was loaded at, which a save of content
  // set from it is built on. Unlike `updatedAt` it doesn't move.
  loadedAt: RichTextState['updatedAt']
}

// What the editor last emitted, for flushing a pending save on unmount.
interface EditorDoc {
  json: JSONContent
  appliedVersion: number | undefined
}

// Keeps an open editor in step with the server: debounced conditional saves,
// outside changes (e.g. an agent's append) loaded while there are no local
// edits in play, and a reload plus notice when a save loses a conflict.
function LiveRichTextEditor({ propertyRef, initial, autoFocus, hideToolbar, className }: LiveRichTextEditorProps) {
  const { data = initial } = useRichText(propertyRef)
  const [synced, setSynced] = useState<SyncedVersion>({
    updatedAt: initial.updatedAt,
    doc: initial.doc,
    version: 0,
    loadedAt: initial.updatedAt,
  })
  // `loadedAt` of every version pushed into the editor, so a save is built on
  // the version the editor says its content was set from, not the newest one.
  const loadedAtRef = useRef(new Map<number, Timestamp | undefined>())
  useEffect(() => {
    loadedAtRef.current.set(synced.version, synced.loadedAt)
  }, [synced.version, synced.loadedAt])
  // True while a debounced save is waiting to fire.
  const [dirty, setDirty] = useState(false)
  const [conflicted, setConflicted] = useState(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const latestDocRef = useRef<EditorDoc | null>(null)

  const handleSaved = useCallback((saved: RichTextState) => {
    // Our own save: the editor already shows it, so only the base and the doc a
    // fresh editor would start from move, not the version.
    setSynced((s) =>
      isNewerRichText(saved.updatedAt, s.updatedAt) ? { ...s, updatedAt: saved.updatedAt, doc: saved.doc } : s,
    )
  }, [])

  const handleConflict = useCallback((latest: RichTextState) => {
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = null
    setDirty(false)
    setConflicted(true)
    setSynced((s) => ({
      updatedAt: latest.updatedAt,
      doc: latest.doc,
      version: s.version + 1,
      loadedAt: latest.updatedAt,
    }))
  }, [])

  const { save, adopt, busy, unsaved } = usePutRichText(propertyRef, {
    initialBase: initial.updatedAt,
    onSaved: handleSaved,
    onConflict: handleConflict,
  })

  // Load a newer cached version (from Watch) only when no local save is
  // pending or in flight; otherwise the next save's conflict check covers it.
  if (!dirty && !busy && isNewerRichText(data.updatedAt, synced.updatedAt)) {
    setSynced({ updatedAt: data.updatedAt, doc: data.doc, version: synced.version + 1, loadedAt: data.updatedAt })
  }

  // The next save expects the version now in the editor.
  useEffect(() => {
    adopt(synced.updatedAt)
  }, [adopt, synced.updatedAt])

  // The doc goes out built on the version the editor's content was set from.
  const saveEditorDoc = useCallback(
    ({ json, appliedVersion }: EditorDoc) => {
      const base = appliedVersion === undefined ? undefined : loadedAtRef.current.get(appliedVersion)
      save(JSON.stringify(json), base)
    },
    [save],
  )

  // Leaving with a debounced save pending (e.g. navigating to another day)
  // sends it now, so the text isn't lost and a pending prune can wait for it.
  useEffect(() => {
    return () => {
      if (!timerRef.current || !latestDocRef.current) return
      clearTimeout(timerRef.current)
      timerRef.current = null
      saveEditorDoc(latestDocRef.current)
    }
  }, [saveEditorDoc])

  const handleUpdate = useCallback(
    (json: JSONContent, appliedVersion: number | undefined) => {
      const edited = { json, appliedVersion }
      latestDocRef.current = edited
      setConflicted(false)
      setDirty(true)
      if (timerRef.current) clearTimeout(timerRef.current)
      timerRef.current = setTimeout(() => {
        timerRef.current = null
        setDirty(false)
        // Link/date derivation happens server-side inside Put — the FE just saves the doc.
        saveEditorDoc(edited)
      }, RICHTEXT_DEBOUNCE_MS)
    },
    [saveEditorDoc],
  )

  // A save being retried comes first; a conflict ends the retrying and takes its place.
  const noticeText = () => {
    if (unsaved) return 'Not saved yet. Retrying…'
    if (conflicted) return "Updated elsewhere; your latest edit wasn't saved."
    return null
  }
  const message = noticeText()
  const notice = message ? (
    <p role="status" className="border-b border-border px-4 py-1 text-xs text-muted-foreground">
      {message}
    </p>
  ) : null

  return (
    <TiptapEditor
      doc={synced.doc}
      externalDoc={synced.doc}
      externalVersion={synced.version}
      onUpdate={handleUpdate}
      autoFocus={autoFocus}
      hideToolbar={hideToolbar}
      notice={notice}
      className={className}
    />
  )
}
