import type { ReactNode } from 'react'

import { FormatForm } from './FormatForm'
import { MachineForm } from './MachineForm'
import { MaterialForm } from './MaterialForm'
import { OperationForm } from './OperationForm'
import { PolicyForm } from './PolicyForm'
import type { EntityFormProps } from './PricingForm'
import {
  emptyFormatFormValues,
  emptyMachineFormValues,
  emptyMaterialFormValues,
  emptyOperationFormValues,
  emptyPolicyFormValues,
  fromFormatDoc,
  fromMachineDoc,
  fromMaterialDoc,
  fromOperationDoc,
  fromPolicyDoc,
} from './types'
import type { CatalogDoc, PricingEntitySegment } from './types'

interface EntityConfig {
  /** i18n key under `pricing:entitySingular.*` for form-screen titles. */
  singularKey: string
  renderNew: (props: EntityFormProps) => ReactNode
  renderEdit: (doc: CatalogDoc, props: EntityFormProps) => ReactNode
}

export const ENTITY_REGISTRY: Record<PricingEntitySegment, EntityConfig> = {
  formats: {
    singularKey: 'entitySingular.formats',
    renderNew: (props) => <FormatForm initialValues={emptyFormatFormValues} {...props} />,
    renderEdit: (doc, props) => <FormatForm initialValues={fromFormatDoc(doc)} {...props} />,
  },
  materials: {
    singularKey: 'entitySingular.materials',
    renderNew: (props) => <MaterialForm initialValues={emptyMaterialFormValues} {...props} />,
    renderEdit: (doc, props) => <MaterialForm initialValues={fromMaterialDoc(doc)} {...props} />,
  },
  machines: {
    singularKey: 'entitySingular.machines',
    renderNew: (props) => <MachineForm initialValues={emptyMachineFormValues} {...props} />,
    renderEdit: (doc, props) => <MachineForm initialValues={fromMachineDoc(doc)} {...props} />,
  },
  operations: {
    singularKey: 'entitySingular.operations',
    renderNew: (props) => <OperationForm initialValues={emptyOperationFormValues} {...props} />,
    renderEdit: (doc, props) => <OperationForm initialValues={fromOperationDoc(doc)} {...props} />,
  },
  policies: {
    singularKey: 'entitySingular.policies',
    renderNew: (props) => <PolicyForm initialValues={emptyPolicyFormValues()} {...props} />,
    renderEdit: (doc, props) => <PolicyForm initialValues={fromPolicyDoc(doc)} {...props} />,
  },
}

export function isPricingEntity(value: string | undefined): value is PricingEntitySegment {
  return value !== undefined && value in ENTITY_REGISTRY
}
