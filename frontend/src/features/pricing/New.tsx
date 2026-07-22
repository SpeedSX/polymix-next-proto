import { Navigate, useNavigate, useParams } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { createEntity, pricingKeys } from './api'
import { ENTITY_REGISTRY, isPricingEntity } from './registry'

export function PricingNew() {
  const { t } = useTranslation('pricing')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const { entity } = useParams({ strict: false })

  if (!isPricingEntity(entity)) {
    return <Navigate to="/settings/catalog" />
  }

  const config = ENTITY_REGISTRY[entity]
  const backToList = () => navigate({ to: '/settings/catalog' })

  return config.renderNew({
    breadcrumb: [t('list.title'), t(config.singularKey)],
    title: t('create.title', { entity: t(config.singularKey) }),
    onSubmit: (doc) => createEntity(api, entity, doc),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: pricingKeys.all })
      void backToList()
    },
    onCancel: backToList,
  })
}
