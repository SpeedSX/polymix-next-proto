import { useMemo, useState } from 'react'
import type { KeyboardEvent } from 'react'
import { Modal, ScrollArea, Stack, Text, TextInput } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { fetchSearch, searchKeys } from './api'
import { SEARCH_ENTITIES } from './types'
import type { SearchEntity, SearchHit } from './types'
import { renderHighlight } from './utils'

const SEARCH_DEBOUNCE_MS = 250

const DETAIL_ROUTES: Record<SearchEntity, string> = {
  customers: '/customers/$id',
  orders: '/orders/$id',
  invoices: '/invoices/$id',
}

interface FlatHit {
  entity: SearchEntity
  hit: SearchHit
}

export interface OmniboxProps {
  opened: boolean
  onClose: () => void
}

export function Omnibox({ opened, onClose }: OmniboxProps) {
  const { t } = useTranslation('search')
  const navigate = useNavigate()
  const api = useApi()
  const [query, setQuery] = useState('')
  const [debouncedQuery] = useDebouncedValue(query, SEARCH_DEBOUNCE_MS)
  const [activeIndex, setActiveIndex] = useState(0)
  const trimmedQuery = debouncedQuery.trim()

  const { data, isFetching } = useQuery({
    queryKey: searchKeys.query(trimmedQuery),
    queryFn: () => fetchSearch(api, trimmedQuery),
    enabled: opened && trimmedQuery !== '',
  })

  const flatHits = useMemo<FlatHit[]>(
    () => SEARCH_ENTITIES.flatMap((entity) => (data?.[entity] ?? []).map((hit) => ({ entity, hit }))),
    [data],
  )
  // Clamped at render time rather than reset via effect, since activeIndex can
  // outlive a shrinking result set between the query changing and refetch settling.
  const clampedActiveIndex = flatHits.length === 0 ? 0 : Math.min(activeIndex, flatHits.length - 1)

  function close() {
    setQuery('')
    setActiveIndex(0)
    onClose()
  }

  function goTo(flat: FlatHit) {
    navigate({ to: DETAIL_ROUTES[flat.entity], params: { id: flat.hit.id } })
    close()
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setActiveIndex((index) => Math.min(index + 1, flatHits.length - 1))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setActiveIndex((index) => Math.max(index - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      const flat = flatHits[clampedActiveIndex]
      if (flat) {
        goTo(flat)
      }
    }
  }

  return (
    <Modal opened={opened} onClose={close} withCloseButton={false} padding={0} size="lg" trapFocus>
      <Stack gap={0}>
        <TextInput
          autoFocus
          size="lg"
          variant="unstyled"
          px="md"
          placeholder={t('placeholder')}
          value={query}
          onChange={(event) => {
            setQuery(event.currentTarget.value)
            setActiveIndex(0)
          }}
          onKeyDown={handleKeyDown}
        />
        <ScrollArea.Autosize mah={420}>
          <Stack gap="xs" p="sm" pt={0}>
            {isFetching && (
              <Text c="dimmed" size="sm" p="sm">
                {t('loading')}
              </Text>
            )}
            {!isFetching && trimmedQuery !== '' && flatHits.length === 0 && (
              <Text c="dimmed" size="sm" p="sm">
                {t('empty')}
              </Text>
            )}
            {SEARCH_ENTITIES.map((entity) => {
              const hits = data?.[entity] ?? []
              if (hits.length === 0) {
                return null
              }
              return (
                <Stack key={entity} gap={4}>
                  <Text size="xs" c="dimmed" fw={500} px="sm">
                    {t(`groups.${entity}`)}
                  </Text>
                  {hits.map((hit) => {
                    const flatIndex = flatHits.findIndex((flat) => flat.entity === entity && flat.hit.id === hit.id)
                    const active = flatIndex === clampedActiveIndex
                    return (
                      <Text
                        key={hit.id}
                        size="sm"
                        px="sm"
                        py={6}
                        style={{
                          cursor: 'pointer',
                          borderRadius: 4,
                          backgroundColor: active ? 'var(--mantine-color-blue-light)' : undefined,
                        }}
                        onMouseEnter={() => setActiveIndex(flatIndex)}
                        onClick={() => goTo({ entity, hit })}
                      >
                        {renderHighlight(hit.highlight)}
                      </Text>
                    )
                  })}
                </Stack>
              )
            })}
          </Stack>
        </ScrollArea.Autosize>
      </Stack>
    </Modal>
  )
}
