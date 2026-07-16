import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ClerkProvider } from '@clerk/react'
import { MantineProvider, createTheme } from '@mantine/core'
import type { MantineColorsTuple } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import '@mantine/core/styles.css'
import './index.css'

import './lib/i18n'
import { AuthProvider, isClerkMode } from './lib/auth'
import { LiveUpdatesProvider } from './lib/ws'
import { router } from './app/routes'

const queryClient = new QueryClient()

const myColor: MantineColorsTuple = [
  '#ecf4ff',
  '#dce4f5',
  '#b9c7e2',
  '#94a8d0',
  '#748dc0',
  '#5f7cb7',
  '#5474b4',
  '#44639f',
  '#3a5890',
  '#2c4b80'
];

const theme = createTheme({
  colors: {
    myColor,
  },
  primaryColor: 'myColor',
});

const app = (
  <MantineProvider theme={theme}>
    <QueryClientProvider client={queryClient}>
      <AuthProvider>
        <LiveUpdatesProvider>
          <RouterProvider router={router} />
        </LiveUpdatesProvider>
      </AuthProvider>
    </QueryClientProvider>
  </MantineProvider>
)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isClerkMode() ? (
      <ClerkProvider publishableKey={import.meta.env.VITE_CLERK_PUBLISHABLE_KEY} afterSignOutUrl="/">
        {app}
      </ClerkProvider>
    ) : (
      app
    )}
  </StrictMode>,
)
