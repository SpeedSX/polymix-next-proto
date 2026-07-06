-- Customer entity (M1). Validation lives in the domain layer, not schema
-- constraints, so the table stays SCHEMALESS like the rest of the prototype.
DEFINE TABLE IF NOT EXISTS customer SCHEMALESS;
