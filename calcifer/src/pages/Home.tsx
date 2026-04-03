import { TiptapEditorLoader } from '~/editors/tiptap/TiptapEditor'
import { LexicalEditor } from '~/editors/lexical/LexicalEditor'
import { Tabs, TabsList, TabsTrigger, TabsContent } from '~/components/ui/tabs'

export function Home() {
  return (
    <Tabs defaultValue="tiptap" className="flex h-screen flex-col">
      <div className="shrink-0 border-border px-2">
        <TabsList variant="line">
          <TabsTrigger value="tiptap">Tiptap</TabsTrigger>
          <TabsTrigger value="lexical">Lexical</TabsTrigger>
        </TabsList>
      </div>
      <TabsContent
        value="tiptap"
        forceMount
        className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden"
      >
        <TiptapEditorLoader />
      </TabsContent>
      <TabsContent
        value="lexical"
        forceMount
        className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden"
      >
        <LexicalEditor />
      </TabsContent>
    </Tabs>
  )
}
