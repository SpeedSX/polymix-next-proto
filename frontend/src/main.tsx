import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ClerkProvider } from '@clerk/react'
import {
  Accordion,
  Button,
  Fieldset,
  InputWrapper,
  MantineProvider,
  Table,
  createTheme,
} from '@mantine/core'
import type { CSSVariablesResolver, MantineColorsTuple } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import '@mantine/core/styles.css'
import './index.css'

import './lib/i18n'
import { AuthProvider, isClerkMode } from './lib/auth'
import { LiveUpdatesProvider } from './lib/ws'
import { router } from './app/routes'

const queryClient = new QueryClient()

// "Industry" design-system steel accent (docs/design), OKLCH ramp 100–900
// widened to Mantine's 10 steps; index 6 (#597ea3) is the base accent.
const steel: MantineColorsTuple = [
  '#eef6ff',
  '#d6ebff',
  '#b5d9fd',
  '#94bce3',
  '#749dc4',
  '#678eb4',
  '#597ea3',
  '#416180',
  '#2c455d',
  '#1d2d3d',
]

const theme = createTheme({
  primaryColor: 'steel',
  primaryShade: 6,
  colors: { steel },
  defaultRadius: 0,
  fontFamily: 'Roboto, system-ui, sans-serif',
  headings: {
    fontFamily: '"Roboto", system-ui, sans-serif',
    fontWeight: '500',
  },
  components: {
    Table: Table.extend({
      defaultProps: { highlightOnHoverColor: '#d6ebff' },
      styles: {
        table: { width: '100%' },
        th: {
          textTransform: 'uppercase',
          fontSize: '11px',
          letterSpacing: '0.08em',
          fontWeight: 500,
          color: 'var(--mantine-color-gray-6)',
          paddingTop: '14px',
          background: 'var(--mantine-color-white)',
        },
      },
    }),
    Button: Button.extend({
      styles: {
        root: {
          fontFamily: 'var(--mantine-font-family-headings)',
          fontWeight: 500
        },
      },
    }),
    Fieldset: Fieldset.extend({
      styles: {
        root: {
          backgroundColor: 'transparent',
          borderColor: 'var(--mantine-color-gray-3)',
        },
        legend: {
          fontFamily: 'var(--mantine-font-family-headings)',
          fontWeight: 500,
          fontSize: '13px',
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
          color: 'var(--mantine-color-steel-7)',
          marginBottom: '4px',
        },
      },
    }),
    Accordion: Accordion.extend({
      styles: {
        control: {
          fontFamily: 'var(--mantine-font-family-headings)',
          fontWeight: 500,
        },
      },
    }),
    InputWrapper: InputWrapper.extend({
      styles: {
        label: {
          fontWeight: 500,
          fontSize: '13px',
          marginBottom: '4px',
        },
      },
    }),
  },
})

const cssVariablesResolver: CSSVariablesResolver = () => ({
  variables: {},
  light: { '--mantine-color-body': '#f2f2f3' },
  dark: {},
})

const app = (
  <MantineProvider theme={theme} forceColorScheme="light" cssVariablesResolver={cssVariablesResolver}>
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
