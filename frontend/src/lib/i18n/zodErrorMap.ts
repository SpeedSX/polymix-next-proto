import i18n from 'i18next'
import { z } from 'zod'

/**
 * Maps an exact-length string constraint to the same `common:validation.*`
 * code the backend attaches to the equivalent field (see `money.rs`'s
 * `length(equal = 3, code = "invalid_currency_code")` and `customer.rs`'s
 * `length(equal = 2, code = "invalid_country_code")`) so client and server
 * report identical text for the same rule.
 */
function exactStringLengthCode(length: number): string {
  if (length === 2) return 'invalid_country_code'
  if (length === 3) return 'invalid_currency_code'
  return 'out_of_range'
}

function resolveValidationCode(issue: z.ZodIssueOptionalMessage): string | null {
  switch (issue.code) {
    case z.ZodIssueCode.invalid_type:
      if (issue.received === 'undefined') return 'required'
      if (issue.expected === 'integer' || issue.expected === 'number') return 'out_of_range'
      return null
    case z.ZodIssueCode.too_small:
      if (issue.type === 'array') return 'min_line_items'
      if (issue.type === 'string') return issue.exact ? exactStringLengthCode(Number(issue.minimum)) : 'required'
      if (issue.type === 'number') return issue.minimum === 1 ? 'positive_quantity' : 'out_of_range'
      return null
    case z.ZodIssueCode.too_big:
      if (issue.type === 'string' && issue.exact) return exactStringLengthCode(Number(issue.maximum))
      if (issue.type === 'number') return 'out_of_range'
      return null
    case z.ZodIssueCode.invalid_string:
      return issue.validation === 'email' ? 'invalid_email' : 'invalid_amount'
    case z.ZodIssueCode.invalid_union: {
      // Fields like `country`/`email` are `z.union([z.literal(''), <real check>])` so an
      // empty string can bypass the check; the useful message lives on the non-empty branch.
      const specific = issue.unionErrors
        .flatMap((unionError) => unionError.issues)
        .find((subIssue) => subIssue.code !== z.ZodIssueCode.invalid_literal)
      return specific ? resolveValidationCode(specific) : null
    }
    default:
      return null
  }
}

/**
 * Routes every Zod validation message through `common:validation.<code>`,
 * reusing the same codes the backend's `validation_failed` responses use for
 * the matching field rule, so client-side and server-side errors read
 * identically instead of Zod's untranslated English defaults.
 */
export const zodErrorMap: z.ZodErrorMap = (issue, ctx) => {
  const code = resolveValidationCode(issue)
  return { message: code ? i18n.t(`common:validation.${code}`) : ctx.defaultError }
}
