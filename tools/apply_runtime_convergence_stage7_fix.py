#!/usr/bin/env python3
from pathlib import Path

path = Path("crates/mini-transport-security/tests/authenticated_onion_chain_tcp.rs")
text = path.read_text(encoding="utf-8")
old = """    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();
    let entry_root_kel = entry.identity.root.kel();
    let entry_device_kel = entry.identity.device.kel();
    let rendezvous_root_kel = rendezvous.identity.root.kel();
    let rendezvous_device_kel = rendezvous.identity.device.kel();
    let delivery_root_kel = delivery.identity.root.kel();
    let delivery_device_kel = delivery.identity.device.kel();
    let destination_root_kel = destination.identity.root.kel();
    let destination_device_kel = destination.identity.device.kel();
"""
new = """    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();
    let entry_root_for_client = entry.identity.root.kel();
    let entry_device_for_client = entry.identity.device.kel();
    let entry_root_for_rendezvous = entry.identity.root.kel();
    let entry_device_for_rendezvous = entry.identity.device.kel();
    let rendezvous_root_for_entry = rendezvous.identity.root.kel();
    let rendezvous_device_for_entry = rendezvous.identity.device.kel();
    let rendezvous_root_for_delivery = rendezvous.identity.root.kel();
    let rendezvous_device_for_delivery = rendezvous.identity.device.kel();
    let delivery_root_for_rendezvous = delivery.identity.root.kel();
    let delivery_device_for_rendezvous = delivery.identity.device.kel();
    let delivery_root_for_destination = delivery.identity.root.kel();
    let delivery_device_for_destination = delivery.identity.device.kel();
    let destination_root_kel = destination.identity.root.kel();
    let destination_device_kel = destination.identity.device.kel();
"""
if text.count(old) != 1:
    raise SystemExit("declaration block mismatch")
text = text.replace(old, new)
changes = [
    ("&delivery_root_kel,\n            &delivery_device_kel,", "&delivery_root_for_destination,\n            &delivery_device_for_destination,"),
    ("&rendezvous_root_kel,\n            &rendezvous_device_kel,", "&rendezvous_root_for_delivery,\n            &rendezvous_device_for_delivery,"),
    ("&entry_root_kel,\n            &entry_device_kel,", "&entry_root_for_rendezvous,\n            &entry_device_for_rendezvous,"),
    ("&delivery_root_kel,\n            &delivery_device_kel,", "&delivery_root_for_rendezvous,\n            &delivery_device_for_rendezvous,"),
    ("&rendezvous_root_kel,\n            &rendezvous_device_kel,", "&rendezvous_root_for_entry,\n            &rendezvous_device_for_entry,"),
    ("&entry.identity.root.kel(),\n        &entry.identity.device.kel(),", "&entry_root_for_client,\n        &entry_device_for_client,"),
]
for source, target in changes:
    if source not in text:
        raise SystemExit("threaded KEL use mismatch")
    text = text.replace(source, target, 1)
path.write_text(text, encoding="utf-8")
print("stage 7 threaded KEL fix applied")
