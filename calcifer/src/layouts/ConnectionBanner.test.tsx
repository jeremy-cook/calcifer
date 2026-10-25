// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { act, cleanup, render, screen } from '@testing-library/react'
import { replica } from '~/model/replica'
import { ConnectionBanner } from '~/layouts/ConnectionBanner'

beforeEach(() => {
  replica.reset()
})

afterEach(() => {
  cleanup()
})

describe('ConnectionBanner', () => {
  it('shows nothing while connecting or live', () => {
    render(<ConnectionBanner />)
    expect(screen.queryByRole('status')).toBeNull()

    act(() => replica.setConnection('live'))
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('shows the notice while retrying', () => {
    render(<ConnectionBanner />)
    act(() => replica.setConnection('retrying'))
    expect(screen.getByRole('status').textContent).toBe("Can't reach the server. Retrying…")
  })

  it('hides the notice once live again', () => {
    render(<ConnectionBanner />)
    act(() => replica.setConnection('retrying'))
    expect(screen.getByRole('status')).toBeTruthy()

    act(() => replica.setConnection('live'))
    expect(screen.queryByRole('status')).toBeNull()
  })
})
