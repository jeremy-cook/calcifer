import { type Editor } from '@tiptap/react'
import { ImageInsertPopover as SharedImageInsertPopover } from '~/editors/shared/ImageInsertPopover'

interface Props {
  editor: Editor
}

export function ImageInsertPopover({ editor }: Props) {
  function onInsert(src: string, altText: string) {
    editor.chain().focus().setImage({ src, alt: altText }).run()
  }

  return <SharedImageInsertPopover onInsert={onInsert} />
}
