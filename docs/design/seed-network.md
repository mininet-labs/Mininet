# Owner-controlled seed network

The target is a shared content network: nodes and servers retain explicitly
selected signed history for an owner-chosen number of days, serve it to peers,
and collect verifiable delivery evidence and settlement claims. Original copies,
local retention, network availability and payment finality are separate facts.

This branch connects a bounded persistent seed pool to authenticated encrypted
transport. Pool expiry stops new reads even before physical cleanup. Revocation
and pruning affect only the pool, never the owner's original object store.
Private/encrypted objects are excluded from this public seed path. Owners opt
in with storage limits; no device joins or spends merely by opening Mininet.

Service evidence must bind the actual received bytes and both delegated devices
using mini-storage receipts. A signed payment promise is pending until canonical
settlement; local counters, replica requests and elapsed retention are not proof
of payment or continuous independent availability. No new monetary issuance,
governance authority or paid organic ranking is introduced.

Implementation and usage details will be recorded here as each slice lands.
The initial draft checkpoint records direction, not completed functionality.
