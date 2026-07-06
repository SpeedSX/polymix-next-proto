import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ClerkProvider } from '@clerk/react'
import { MantineProvider } from '@mantine/core'
import { RouterProvider } from '@tanstack/react-router'
import '@mantine/core/styles.css'
import './index.css'

import './lib/i18n'
import { router } from './app/routes'

const clerkPublishableKey = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ClerkProvider publishableKey={clerkPublishableKey} afterSignOutUrl="/">
      <MantineProvider>
        <RouterProvider router={router} />
      </MantineProvider>
    </ClerkProvider>
  </StrictMode>,
)
