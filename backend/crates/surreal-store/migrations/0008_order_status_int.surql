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
  END;

DEFINE INDEX IF NOT EXISTS order_status ON order FIELDS status;

