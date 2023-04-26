# Findings so far

* House always wins does nothing to help this system so far, because the protection never kicks in. We would need to simulate a different kind of attack where the attacker first raised then lowered the price (or vice versa), or something like that. Worth doing still IMO.
* Due to the nature of the simulation, TWAP protection tops out after a certain size. We can make it more helpful by extending the time it takes to complete an attack, but that's not really relevant. Importantly though, TWAP definitely reduces successful attacks.
* Initial review indicates that artificial slippage on its own is _not_ sufficient to stop an attacker. That's because the attacker is able to receive back his slippage funds. However, if we reduce the amount of funds the attacker receives back, it becomes a much more powerful deterrent.
* Due to fee calculations, it appears that higher leverage positions often do not result in a successful attacks (needs a bit more investigation). This means that to make significant profits, an attacker would have to put more of their funds at risk via collateral, which is a good feature of the protocol.
* Unidirectional slippage (trader only pays, never receives) seems to reduce even more attack cases
