# perps-bots

## Local development using osmosis-local

Rough high level steps:

- Start localosmo

Using the top level justfile, run this:

``` shellsession
just run-localosmo
```

- Deploy contracts

Using the top level justfile, run this:

``` shellsession
just local-deploy
```

- Modify the perp-bot's [justfile](./justfile) environment variable. The only
  thing that needs to be changed there is `LEVANA_BOTS` environment
  variable based on previous output.

- Deposit liqudity using perps-qa tool:

``` shellsession
perps-qa deposit-liquidity --fund 5000
```

Make sure you set various environment variables appropritely before
running the command.

- Run the application via the justfile target `just run`.
