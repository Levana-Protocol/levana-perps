GET http://localhost:3000
HTTP 200

POST http://localhost:3000/pnl-url
Content-Type: application/json
Accept: application/json
{
  "address": "neutron1awkajw4ltv0uw3jhqg5gx2lwkwl0nyg4krq54dg8x5w7qawcymyqenzcnk",
  "chain": "neutron-1",
  "position_id": "1",
  "pnl_type": "usd"
}
HTTP *
[Asserts]
status == 200
jsonpath "$.url" contains "/pnl/"

GET http://localhost:3000/export-history/osmosis-1/osmo1ssw6x553kzqher0earlkwlxasfm2stnl3ms3ma2zz4tnajxyyaaqlucd45/osmo12g96ahplpf78558cv5pyunus2m66guykt96lvc
HTTP *
[Asserts]
status == 200
