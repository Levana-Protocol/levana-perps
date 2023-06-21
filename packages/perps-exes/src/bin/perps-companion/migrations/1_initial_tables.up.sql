-- Add up migration script here
CREATE TABLE "address"("id" SERIAL8  PRIMARY KEY UNIQUE,"address" VARCHAR(70) NOT NULL);
ALTER TABLE "address" ADD CONSTRAINT "unique_address" UNIQUE("address");

CREATE TABLE "position_detail"("id" SERIAL8  PRIMARY KEY UNIQUE,"contract_address" INT8 NOT NULL,"chain" INTEGER NOT NULL,"position_id" BIGINT NOT NULL,"url_id" SERIAL NOT NULL,"pnl_type" VARCHAR(10) NOT NULL);
ALTER TABLE "position_detail" ADD CONSTRAINT "unique_position" UNIQUE("contract_address","chain","position_id","pnl_type");
ALTER TABLE "position_detail" ADD CONSTRAINT "unique_url_id" UNIQUE("url_id");
ALTER TABLE "position_detail" ADD CONSTRAINT "position_detail_contract_address_fkey" FOREIGN KEY("contract_address") REFERENCES "address"("id") ON DELETE RESTRICT  ON UPDATE RESTRICT;

-- The urls need to start with 1000
ALTER SEQUENCE public."position_detail_url_id_seq" RESTART WITH 1000;
