import { createFileRoute } from '@tanstack/react-router'
import { useEntityStore } from '~/model/store'

export const Route = createFileRoute('/tag/$id')({
  component: function TagRoute() {
    const { id } = Route.useParams()
    const tag = useEntityStore((s) => s.entities[id])

    if (!tag || tag.structureType !== 'Tag') {
      return (
        <div className="flex flex-col gap-2 px-16 py-10">
          <h1 className="text-2xl font-semibold">Tag not found</h1>
          <p className="text-sm text-muted-foreground">This tag doesn't exist or was deleted.</p>
        </div>
      )
    }

    return (
      <div className="flex flex-col gap-6 px-16 py-10">
        <h1 className="text-2xl font-semibold">#{tag.title}</h1>
        <p className="text-sm text-muted-foreground">No content yet.</p>
      </div>
    )
  },
})
