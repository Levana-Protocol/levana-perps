CREATE TABLE "proposal_detail"(
    "id" SERIAL8  PRIMARY KEY UNIQUE,
    "title" TEXT NOT NULL,
    "address" TEXT NOT NULL UNIQUE,
    "chain" INTEGER NOT NULL,
    "environment" INTEGER NOT NULL,
    "url_id" SERIAL8 NOT NULL UNIQUE
);
