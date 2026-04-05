import { useEffect } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import {
  $insertNodes,
  $isRootOrShadowRoot,
  $createParagraphNode,
  COMMAND_PRIORITY_EDITOR,
  COMMAND_PRIORITY_LOW,
  PASTE_COMMAND,
  createCommand,
  type LexicalCommand,
} from 'lexical'
import { $wrapNodeInElement, mergeRegister } from '@lexical/utils'
import { $createImageNode, ImageNode } from '../nodes/ImageNode'
import { readFileAsDataURL } from '~/lib/utils'

export const INSERT_IMAGE_COMMAND: LexicalCommand<{ src: string; altText?: string }> =
  createCommand('INSERT_IMAGE_COMMAND')

export function ImagesPlugin() {
  const [editor] = useLexicalComposerContext()

  useEffect(() => {
    if (!editor.hasNodes([ImageNode])) {
      throw new Error('ImagesPlugin: ImageNode not registered on editor')
    }

    return mergeRegister(
      editor.registerCommand(
        INSERT_IMAGE_COMMAND,
        (payload) => {
          const imageNode = $createImageNode({ src: payload.src, altText: payload.altText ?? '' })
          $insertNodes([imageNode])
          if ($isRootOrShadowRoot(imageNode.getParentOrThrow())) {
            $wrapNodeInElement(imageNode, $createParagraphNode).selectEnd()
          }
          return true
        },
        COMMAND_PRIORITY_EDITOR,
      ),
      editor.registerCommand(
        PASTE_COMMAND,
        (payload) => {
          const clipboardEvent = payload as ClipboardEvent
          const items = Array.from(clipboardEvent.clipboardData?.items ?? [])
          const imageItem = items.find((item) => item.type.startsWith('image/'))
          if (imageItem) {
            clipboardEvent.preventDefault()
            const file = imageItem.getAsFile()
            if (file) {
              readFileAsDataURL(file).then((src) => {
                editor.dispatchCommand(INSERT_IMAGE_COMMAND, { src })
              })
            }
            return true
          }
          return false
        },
        COMMAND_PRIORITY_LOW,
      ),
    )
  }, [editor])

  return null
}
