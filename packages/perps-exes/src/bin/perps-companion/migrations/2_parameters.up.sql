-- Add up migration script here
ALTER TABLE "position_detail"
ALTER COLUMN "pnl" DROP NOT NULL;

ALTER TABLE "position_detail"
RENAME "pnl" TO "pnl_usd";

ALTER TABLE "position_detail"
ADD COLUMN "pnl_percentage" VARCHAR,
ADD COLUMN "wallet" VARCHAR;
