import { createRootRoute, createRoute, createRouter, Navigate } from '@tanstack/react-router'

import { CustomerDetail } from '../features/customers/Detail'
import { CustomerList } from '../features/customers/List'
import { CustomerNew } from '../features/customers/New'
import { InvoiceDetail } from '../features/invoices/Detail'
import { InvoiceList } from '../features/invoices/List'
import { OrderDetail } from '../features/orders/Detail'
import { OrderList } from '../features/orders/List'
import { OrderNew } from '../features/orders/New'
import { PricingEdit } from '../features/pricing/Edit'
import { PricingNew } from '../features/pricing/New'
import { SettingsCatalog } from '../features/settings/Catalog'
import { SettingsLayout } from '../features/settings/SettingsLayout'
import { SettingsUsersRoles } from '../features/settings/UsersRoles'
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

const ordersListRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/orders',
  component: OrderList,
})

const ordersNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/orders/new',
  component: OrderNew,
})

const orderDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/orders/$id',
  component: OrderDetail,
})

const invoicesListRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/invoices',
  component: InvoiceList,
})

const invoiceDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/invoices/$id',
  component: InvoiceDetail,
})

const settingsLayoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: SettingsLayout,
})

const settingsIndexRoute = createRoute({
  getParentRoute: () => settingsLayoutRoute,
  path: '/',
  component: () => <Navigate to="/settings/catalog" />,
})

const settingsCatalogRoute = createRoute({
  getParentRoute: () => settingsLayoutRoute,
  path: '/catalog',
  component: SettingsCatalog,
})

const settingsCatalogNewRoute = createRoute({
  getParentRoute: () => settingsLayoutRoute,
  path: '/catalog/$entity/new',
  component: PricingNew,
})

const settingsCatalogEditRoute = createRoute({
  getParentRoute: () => settingsLayoutRoute,
  path: '/catalog/$entity/$id',
  component: PricingEdit,
})

const settingsUsersRolesRoute = createRoute({
  getParentRoute: () => settingsLayoutRoute,
  path: '/users-roles',
  component: SettingsUsersRoles,
})

const routeTree = rootRoute.addChildren([
  indexRoute,
  customersListRoute,
  customersNewRoute,
  customerDetailRoute,
  ordersListRoute,
  ordersNewRoute,
  orderDetailRoute,
  invoicesListRoute,
  invoiceDetailRoute,
  settingsLayoutRoute.addChildren([
    settingsIndexRoute,
    settingsCatalogRoute,
    settingsCatalogNewRoute,
    settingsCatalogEditRoute,
    settingsUsersRolesRoute,
  ]),
])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
