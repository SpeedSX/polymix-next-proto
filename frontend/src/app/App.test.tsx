import { render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { RouterProvider } from '@tanstack/react-router'
import { describe, expect, it } from 'vitest'

import '../lib/i18n'
import { router } from './routes'

describe('app shell', () => {
  it('renders the app title', async () => {
    render(
      <MantineProvider>
        <RouterProvider router={router} />
      </MantineProvider>,
    )

    expect(await screen.findAllByText('PolyMix Next')).not.toHaveLength(0)
  })
})
