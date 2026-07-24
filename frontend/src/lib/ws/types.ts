export type ChangeEntity = 'customer' | 'order' | 'invoice' | 'quote'
export type ChangeAction = 'create' | 'update' | 'delete'

export interface ChangeFrame {
  type: 'change'
  entity: ChangeEntity
  action: ChangeAction
  id: string
  data: unknown
}

export interface ResyncFrame {
  type: 'resync'
}

export interface PingFrame {
  type: 'ping'
}

export type ServerFrame = ChangeFrame | ResyncFrame | PingFrame
