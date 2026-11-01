// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { act, cleanup, render, screen } from '@testing-library/react'
import { replica } from '~/model/replica'
import { ConnectionPill } from '~/layouts/ConnectionPill'

beforeEach(() => {
  replica.reset()
})

afterEach(() => {
  cleanup()
})

describe('ConnectionPill', () => {
  it('shows nothing while connecting or live', () => {
    render(<ConnectionPill />)
    expect(screen.queryByRole('status')).toBeNull()

    act(() => replica.setConnection('live'))
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('shows the pill while retrying', () => {
    render(<ConnectionPill />)
    act(() => replica.setConnection('retrying'))
    expect(screen.getByRole('status').textContent).toBe('Offline · retrying')
  })

  it('hides the pill once live again', () => {
    render(<ConnectionPill />)
    act(() => replica.setConnection('retrying'))
    expect(screen.getByRole('status')).toBeTruthy()

    act(() => replica.setConnection('live'))
    expect(screen.queryByRole('status')).toBeNull()
  })
})
