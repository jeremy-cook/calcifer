import { type Editor } from '@tiptap/react'
import { ImageInsertPopover as SharedImageInsertPopover } from '~/editors/shared/ImageInsertPopover'

interface Props {
  editor: Editor
}

export function ImageInsertPopover({ editor }: Props) {
  return (
    <SharedImageInsertPopover
      onInsert={(src) => editor.chain().focus().setImage({ src }).run()}
    />
  )
}
