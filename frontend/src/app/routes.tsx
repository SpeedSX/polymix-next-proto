import { createRootRoute, createRoute, createRouter, Navigate } from '@tanstack/react-router'

import { CustomerDetail } from '../features/customers/Detail'
import { CustomerList } from '../features/customers/List'
import { CustomerNew } from '../features/customers/New'
import { AppShell } from './AppShell'

const rootRoute = createRootRoute({
  component: AppShell,
})

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: () => <Navigate to="/customers" />,
})

const customersListRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/customers',
  component: CustomerList,
})

const customersNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/customers/new',
  component: CustomerNew,
})

const customerDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/customers/$id',
  component: CustomerDetail,
})

const routeTree = rootRoute.addChildren([
  indexRoute,
  customersListRoute,
  customersNewRoute,
  customerDetailRoute,
])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
