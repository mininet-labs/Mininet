# mini-messaging

Encrypted private-message semantics over `ObjectEnvelopeV2` and `mini-store`.

Implemented now: signed encrypted text/system messages, replies, attachment
links, delivery/read receipts, opaque-route persistence, deterministic reads,
and per-item rejection of undecryptable/malformed envelopes.

Not implemented here: authenticated prekeys, pairwise session establishment,
forward secrecy, post-compromise security, multi-device fanout, relay mailbox
delivery, push notifications, spam controls, group-key rotation, or call
signalling. Supplying a `ConversationSecret` means the caller has already
established it safely; raw keys must not be copied through an unauthenticated
channel in a production client.
