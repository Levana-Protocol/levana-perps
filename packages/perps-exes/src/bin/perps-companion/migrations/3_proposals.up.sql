CREATE TABLE "proposal_detail"(
    "id" SERIAL8  PRIMARY KEY UNIQUE,
    "title" VARCHAR(200) NOT NULL,
    "address" VARCHAR(70) NOT NULL UNIQUE,
    "chain" INTEGER NOT NULL,
    "market_id" VARCHAR NOT NULL,
    "environment" INTEGER NOT NULL
    "url_id" SERIAL8 NOT NULL UNIQUE,
);
