#!/usr/bin/env python3
from pathlib import Path

path = Path("tools/apply_runtime_convergence_stage7.py")
text = path.read_text(encoding="utf-8")
old = '''replace(
    decision,
    """validity-window, fail-closed relay and destination replay state; advertisement
expiry/network rechecks; bounded selection input; channel-scoped authenticated
search-provider provenance; and wrong-purpose rejection. Focused
""",
    """validity-window, fail-closed relay and destination replay state; advertisement
expiry/network rechecks; bounded selection input; permanent connection poisoning
on ambiguous bearer/channel failure; authenticated CH1 on every socket in a full
onion chain; sealed channel-scoped search-provider provenance; and wrong-purpose
rejection. Focused
""",
)
'''
new = '''replace(
    decision,
    """onion-v2 domain separation, clock-skew-bounded validity, fail-closed relay and
 destination replay state; advertisement
expiry/network rechecks; bounded selection input; channel-scoped authenticated
search-provider provenance; and wrong-purpose rejection. Focused
""",
    """onion-v2 domain separation, clock-skew-bounded validity, fail-closed relay and
 destination replay state; advertisement
expiry/network rechecks; bounded selection input; permanent connection poisoning
on ambiguous bearer/channel failure; authenticated CH1 on every socket in a full
onion chain; sealed channel-scoped search-provider provenance; and wrong-purpose
rejection. Focused
""",
)
'''
if text.count(old) != 1:
    raise SystemExit("stage 7 decision replacement block mismatch")
path.write_text(text.replace(old, new), encoding="utf-8")
print("stage 7 decision replacement aligned")
