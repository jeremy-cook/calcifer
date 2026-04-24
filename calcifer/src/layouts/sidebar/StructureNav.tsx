import { Link } from '@tanstack/react-router'
import { STRUCTURE_LIST } from '~/model/structures'

const ROW_CLASS =
  'flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground hover:bg-muted [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground'

export function StructureNav() {
  return (
    <div className="flex flex-col gap-0.5">
      {STRUCTURE_LIST.map((s) => {
        const IconComponent = s.icon
        return (
          <Link
            key={s.type}
            to="/s/$structureType"
            params={{ structureType: s.type }}
            className={ROW_CLASS}
            activeProps={{ className: 'bg-muted font-medium' }}
          >
            <IconComponent />
            <span className="truncate">{s.plural}</span>
          </Link>
        )
      })}
    </div>
  )
}
