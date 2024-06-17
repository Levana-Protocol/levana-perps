-- Add down migration script here
ALTER TABLE "position_detail"
DROP COLUMN "pnl_percentage",
DROP COLUMN "wallet";

ALTER TABLE "position_detail"
ALTER COLUMN "pnl_usd" SET NOT NULL;

ALTER TABLE "position_detail"
RENAME "pnl_usd" TO "pnl";
