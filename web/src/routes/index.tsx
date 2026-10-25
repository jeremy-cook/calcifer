import { createFileRoute } from '@tanstack/react-router'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'

export const Route = createFileRoute('/')({
  component: TiptapEditor,
})
