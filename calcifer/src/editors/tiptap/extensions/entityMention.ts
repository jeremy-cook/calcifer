import { Mention } from '@tiptap/extension-mention'
import { ReactNodeViewRenderer } from '@tiptap/react'
import { MentionNodeView } from './MentionNodeView'

export const EntityMention = Mention.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      structureType: {
        default: null,
        parseHTML: (el) => el.getAttribute('data-structure-type'),
        renderHTML: (attrs) =>
          attrs.structureType ? { 'data-structure-type': attrs.structureType } : {},
      },
      char: {
        default: '@',
        parseHTML: (el) => el.getAttribute('data-char'),
        renderHTML: (attrs) => (attrs.char ? { 'data-char': attrs.char } : {}),
      },
    }
  },
  addNodeView() {
    return ReactNodeViewRenderer(MentionNodeView)
  },
})

export const HashtagMention = EntityMention.extend({ name: 'hashtag' })
