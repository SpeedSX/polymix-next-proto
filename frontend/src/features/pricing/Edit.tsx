import { Alert, Button, Loader } from '@mantine/core'
import { IconTrash } from '@tabler/icons-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Navigate, useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { deleteEntity, fetchOne, pricingKeys, updateEntity } from './api'
import { ENTITY_REGISTRY, isPricingEntity } from './registry'

export function PricingEdit() {
  const { t } = useTranslation('pricing')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const { entity, id } = useParams({ strict: false })

  const enabled = isPricingEntity(entity) && typeof id === 'string'
  const { data, isLoading, isError } = useQuery({
    queryKey: enabled ? pricingKeys.detail(entity, id) : ['pricing', 'disabled'],
    queryFn: () => fetchOne(api, entity as never, id as string),
    enabled,
  })

  const deleteMutation = useMutation({
    mutationFn: () => deleteEntity(api, entity as never, id as string),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: pricingKeys.all })
      void navigate({ to: '/settings/catalog' })
    },
  })

  if (!isPricingEntity(entity) || typeof id !== 'string') {
    return <Navigate to="/settings/catalog" />
  }

  if (isError) {
    return <Alert color="red">{t('form.unexpectedError')}</Alert>
  }
  if (isLoading || !data) {
    return <Loader />
  }

  const config = ENTITY_REGISTRY[entity]
  const backToList = () => navigate({ to: '/settings/catalog' })

  return config.renderEdit(data, {
    breadcrumb: [t('list.title'), t(config.singularKey)],
    title: String(data.name ?? data.id ?? t(config.singularKey)),
    onSubmit: (doc) => updateEntity(api, entity, id, doc),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: pricingKeys.all })
      void backToList()
    },
    onCancel: backToList,
    headerActions: (
      <Button
        variant="subtle"
        color="red"
        leftSection={<IconTrash size={16} />}
        loading={deleteMutation.isPending}
        onClick={() => {
          if (window.confirm(t('form.confirmDelete'))) {
            deleteMutation.mutate()
          }
        }}
      >
        {t('form.delete')}
      </Button>
    ),
  })
}
