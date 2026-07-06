import type { ReactNode } from 'react'
import { OrganizationSwitcher, SignIn, useAuth as useClerkAuth, useOrganization } from '@clerk/react'
import { Center, Stack, Text } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { AuthContext } from './context'

export function ClerkAuthProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const { isSignedIn, getToken, signOut } = useClerkAuth()
  const { organization, isLoaded: isOrgLoaded } = useOrganization()

  if (!isSignedIn) {
    return (
      <Center mih="100vh">
        <SignIn />
      </Center>
    )
  }

  if (!isOrgLoaded || !organization) {
    return (
      <Center mih="100vh">
        <Stack align="center">
          <Text>{t('auth.selectOrganization')}</Text>
          <OrganizationSwitcher hidePersonal />
        </Stack>
      </Center>
    )
  }

  return (
    <AuthContext.Provider
      value={{
        mode: 'clerk',
        orgId: organization.id,
        getToken: () => getToken(),
        signOut: () => void signOut(),
      }}
    >
      {children}
    </AuthContext.Provider>
  )
}
