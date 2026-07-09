-- Exchange rate snapshots (M4): rows are inserted by
-- `exchange_rate::seed_default_rates` at tenant provisioning, not by this
-- migration — it only establishes the table.
DEFINE TABLE IF NOT EXISTS exchange_rate SCHEMALESS;
