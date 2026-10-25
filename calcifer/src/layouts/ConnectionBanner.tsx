import { ServerUnreachableNotice } from '~/components/ServerUnreachableNotice'
import { useConnectionState } from '~/model/store'

// Shown while the Watch stream is down after a failed attempt. `connecting` shows
// nothing, so a normal load doesn't flash it.
export function ConnectionBanner() {
  const connection = useConnectionState()
  if (connection !== 'retrying') return null
  return <ServerUnreachableNotice />
}
