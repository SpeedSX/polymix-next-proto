import { useCallback, useState } from 'react'
import type { ReactNode } from 'react'
import { Alert, Button, Paper, Stack, TextInput, Title } from '@mantine/core'
import { useForm } from '@mantine/form'
import { useTranslation } from 'react-i18next'

import { AuthContext } from './context'

interface DevSession {
  token: string
  orgId: string
}

interface DevSignInValues {
  userId: string
  orgId: string
}

async function requestDevToken(userId: string, orgId: string): Promise<string> {
  const apiUrl = import.meta.env.VITE_API_URL ?? ''
  const response = await fetch(`${apiUrl}/dev/token`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ user_id: userId, org_id: orgId }),
  })
  if (!response.ok) {
    throw new Error(`dev token request failed: ${response.status}`)
  }
  const body = (await response.json()) as { token: string }
  return body.token
}

function DevSignInForm({
  onSignIn,
}: {
  onSignIn: (userId: string, orgId: string) => Promise<void>
}) {
  const { t } = useTranslation()
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const form = useForm<DevSignInValues>({
    initialValues: { userId: 'user_dev1', orgId: 'org_dev1' },
  })

  const handleSubmit = form.onSubmit(async (values) => {
    setError(null)
    setSubmitting(true)
    try {
      await onSignIn(values.userId, values.orgId)
    } catch {
      setError(t('auth.devSignInFailed'))
    } finally {
      setSubmitting(false)
    }
  })

  return (
    <Stack align="center" justify="center" mih="100vh">
      <Paper withBorder shadow="sm" p="xl" w={360}>
        <Stack>
          <Title order={3}>{t('auth.devSignInTitle')}</Title>
          {error && <Alert color="red">{error}</Alert>}
          <form onSubmit={handleSubmit}>
            <Stack>
              <TextInput label={t('auth.userId')} required {...form.getInputProps('userId')} />
              <TextInput label={t('auth.orgId')} required {...form.getInputProps('orgId')} />
              <Button type="submit" loading={submitting}>
                {t('auth.signIn')}
              </Button>
            </Stack>
          </form>
        </Stack>
      </Paper>
    </Stack>
  )
}

export function DevAuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<DevSession | null>(null)

  const signIn = useCallback(async (userId: string, orgId: string) => {
    const token = await requestDevToken(userId, orgId)
    setSession({ token, orgId })
  }, [])

  if (!session) {
    return <DevSignInForm onSignIn={signIn} />
  }

  return (
    <AuthContext.Provider
      value={{
        mode: 'dev',
        orgId: session.orgId,
        getToken: async () => session.token,
        signOut: () => setSession(null),
      }}
    >
      {children}
    </AuthContext.Provider>
  )
}
