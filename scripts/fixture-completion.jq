length >= 3 and
any(.[]; .name == "affected" and .conclusion == "success") and
([.[] | select(.name | startswith("run"))] | length >= 3) and
any(.[]; .name == "status" and .conclusion == "success") and
all(.[] | select(.name | startswith("run")); .conclusion == "success")
