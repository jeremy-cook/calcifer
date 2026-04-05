import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { ImageInsertPopover as SharedImageInsertPopover } from '~/editors/shared/ImageInsertPopover'
import { INSERT_IMAGE_COMMAND } from '../plugins/ImagesPlugin'

export function ImageInsertPopover() {
  const [editor] = useLexicalComposerContext()
  return (
    <SharedImageInsertPopover
      onInsert={(src) => editor.dispatchCommand(INSERT_IMAGE_COMMAND, { src })}
    />
  )
}
