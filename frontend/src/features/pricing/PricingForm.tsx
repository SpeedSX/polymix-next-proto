import type { ReactNode } from 'react'
import { Alert, Box, Button, Stack } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { PageHeader } from '../../components/PageHeader'
import type { CatalogDoc } from './types'

/** Props every entity form takes; each form adds its own `initialValues`. */
export interface EntityFormProps {
  onSubmit: (doc: CatalogDoc) => Promise<CatalogDoc>
  onSuccess: (doc: CatalogDoc) => void
  onCancel: () => void
  breadcrumb: string[]
  title: ReactNode
  /** Extra header controls (e.g. a Delete button on the edit screen). */
  headerActions?: ReactNode
}

export interface PricingFormShellProps {
  breadcrumb: string[]
  title: ReactNode
  submitting: boolean
  formError: string | null
  onCancel: () => void
  onSubmit: (event?: React.FormEvent<HTMLFormElement>) => void
  headerActions?: ReactNode
  children: ReactNode
}

export function PricingFormShell({
  breadcrumb,
  title,
  submitting,
  formError,
  onCancel,
  onSubmit,
  headerActions,
  children,
}: PricingFormShellProps) {
  const { t } = useTranslation('pricing')
  return (
    <form onSubmit={onSubmit}>
      <Stack gap={0}>
        <PageHeader
          sticky
          breadcrumb={breadcrumb}
          title={title}
          actions={
            <>
              {headerActions}
              <Button variant="subtle" onClick={onCancel} disabled={submitting}>
                {t('form.cancel')}
              </Button>
              <Button type="submit" loading={submitting}>
                {t('form.save')}
              </Button>
            </>
          }
        />
        {formError && (
          <Box pt="md">
            <Alert color="red">{formError}</Alert>
          </Box>
        )}
        <Box maw={760} pt="md">
          {children}
        </Box>
      </Stack>
    </form>
  )
}
