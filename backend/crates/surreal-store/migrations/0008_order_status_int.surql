// M8: Store order.status as an integer code (see domain::order::OrderStatus).
// Backfills existing string values from earlier milestones.

UPDATE order
SET status =
  IF status = "draft" THEN 0
  ELSE IF status = "confirmed" THEN 1
  ELSE IF status = "in_production" THEN 2
  ELSE IF status = "completed" THEN 3
  ELSE IF status = "cancelled" THEN 4
  ELSE status
  END
// RETURN NONE: default UPDATE response echoes every mutated record; against
// the 200k-row demo tenant that's a 100MB+ payload which blows past the WS
// transport's message-size limit and resets the connection mid-migration.
RETURN NONE;

DEFINE INDEX IF NOT EXISTS order_status ON order FIELDS status;

