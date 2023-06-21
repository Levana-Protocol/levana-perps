-- Add up migration script here
CREATE TABLE "market"(
    "id" SERIAL8 PRIMARY KEY UNIQUE,
    "address" VARCHAR(70) NOT NULL UNIQUE,
    "chain" INTEGER NOT NULL,
    "market_id" VARCHAR NOT NULL,
    "environment" INTEGER NOT NULL
);

CREATE TABLE "position_detail"(
    "id" SERIAL8  PRIMARY KEY UNIQUE,
    "market" INT8 NOT NULL REFERENCES market(id),
    "position_id" BIGINT NOT NULL,
    "url_id" SERIAL8 NOT NULL UNIQUE,
    "pnl" VARCHAR NOT NULL,
    "pnl_type" INTEGER NOT NULL,
    "direction" INTEGER NOT NULL,
    "entry_price" VARCHAR NOT NULL,
    "exit_price" VARCHAR NOT NULL,
    "leverage" VARCHAR NOT NULL
);

-- The urls need to start with 1000
ALTER SEQUENCE public."position_detail_url_id_seq" RESTART WITH 1000;
