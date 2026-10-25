import { BubbleMenu } from '@tiptap/react/menus'
import { useEditorState, type Editor } from '@tiptap/react'
import { NodeSelection } from '@tiptap/pm/state'
import { Toggle } from '~/components/ui/toggle'
import { TextAlignLeftIcon, TextAlignCenterIcon, TextAlignRightIcon } from '@phosphor-icons/react'
import type { ImageAlignment } from '~/lib/tiptap-extension-resize-image'

const ALIGNMENTS: { value: ImageAlignment; icon: React.ReactNode; label: string }[] = [
  { value: 'left', icon: <TextAlignLeftIcon />, label: 'Align left' },
  { value: 'center', icon: <TextAlignCenterIcon />, label: 'Align center' },
  { value: 'right', icon: <TextAlignRightIcon />, label: 'Align right' },
]

interface Props {
  editor: Editor
}

export function ImageActionMenu({ editor }: Props) {
  const { alignment } = useEditorState({
    editor,
    selector: (ctx) => ({
      alignment: (ctx.editor.getAttributes('image').alignment as ImageAlignment) ?? 'left',
    }),
  })

  return (
    <BubbleMenu
      editor={editor}
      shouldShow={({ state }) => {
        const { selection } = state
        return selection instanceof NodeSelection && selection.node.type.name === 'image'
      }}
      options={{ placement: 'top' }}
    >
      <div className="flex items-center gap-0.5 rounded-md border border-border bg-background p-1 shadow-md">
        {ALIGNMENTS.map(({ value, icon, label }) => (
          <Toggle
            key={value}
            size="sm"
            pressed={alignment === value}
            onPressedChange={() => editor.chain().focus().setImageAlignment(value).run()}
            aria-label={label}
          >
            {icon}
          </Toggle>
        ))}
      </div>
    </BubbleMenu>
  )
}
