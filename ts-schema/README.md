### Usage 

From root dir:

`just typescript-schema`

you then get a gazillion typescript definition files in `/schema/typescript` (this is *not* checked into the repo).
These should be copied over to frontend (or any typescript project) and re-exported or consumed however

### Why
Typescript definitions auto-generated from our `msg` crate allow us to automatically keep the frontend in sync with contract changes, or at least surface bugs at compiletime instead of runtime.

### But why like this
The JSON schema tooling is an official part of Cosmos, and it's nice to have JSON as a byproduct.

Generating Typescript from JSON relies on tooling that has also been battletested in the wild.

While there are some tools which allow generating Typescript directly from Rust, they aren't production ready yet and butcher comments (at least last time we evaluated it)

So, going to typescript through JSON is a bit convoluted, but at the end of the day it's standing on the shoulders of proven/official tech


### But why like *this*

See comments in [build-schema.mjs](build-schema.mjs)

### Tips and Tricks

Want typechecked guarantees of Query/Response pairs?

First create these Typescript helpers:

```typescript
export type UnionKeys<T> = T extends T ? keyof T : never
export type UnionValue<T, K extends UnionKeys<T>> = T extends { [key in K]: T[K] }
  ? { [key in K]: T[K] }
  : never
```

Then import whatever types you need from the generated files, e.g.:
```typescript
import type {QueryMsg as MarketQueryMsg} from "./path/to/schema/typescript/market_query";
import type {PositionsResp} from "./path/to/schema/typescript/market_response_to_positions"
```

Lastly, create message helpers like this:

```typescript
// "positions" here is the key in MarketQueryMsg and is typechecked to exist
// but it's up to you to associate the correct response here
export type MarketQueryMsgPositions = UnionValue<MarketQueryMsg, "positions">
export type MarketQueryPositions = (msg: MarketQueryMsgPositions) => Promise<PositionsResp>;
```

Now you have Typescript function definitions which guarantee the correct query/response pair 

For example, everything here is typechecked:

```typescript
const foo:MarketQueryPositions = async (props) => {
    props.positions.position_ids
    return {
        positions: []
    }
}
```

Alternative syntax:

```typescript
async function bar(props:Parameters<MarketQueryPositions>[0]):Promise<ReturnType<MarketQueryPositions>> {
    props.positions.position_ids
    return {
        positions: []
    }
}
```