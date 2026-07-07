import { createRootRoute, createRoute, createRouter, Navigate } from '@tanstack/react-router'

import { CustomerDetail } from '../features/customers/Detail'
import { CustomerList } from '../features/customers/List'
import { CustomerNew } from '../features/customers/New'
import { InvoiceDetail } from '../features/invoices/Detail'
import { InvoiceList } from '../features/invoices/List'
import { OrderDetail } from '../features/orders/Detail'
import { OrderList } from '../features/orders/List'
import { OrderNew } from '../features/orders/New'
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
])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
