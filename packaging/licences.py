"""Lists the licences of TuneCraft's dependencies (for ATTRIBUTION.md).
Usage: cargo metadata --format-version 1 | python3 packaging/licences.py"""
import collections
import json
import sys

data = json.load(sys.stdin)
used = {n["id"] for n in data["resolve"]["nodes"]}
counts = collections.Counter()
notable = []
for pkg in data["packages"]:
    if pkg["id"] not in used or pkg["name"] == "tunecraft":
        continue
    lic = pkg.get("license") or "NONE"
    counts[lic] += 1
    if any(word in lic for word in ("MPL", "OFL", "Unicode", "NONE", "GPL", "CC")):
        notable.append(f"{pkg['name']} {pkg['version']}: {lic}")
print(len(used) - 1, "packages")
for lic, n in counts.most_common():
    print(f"{n:4} {lic}")
print("notable:")
for line in sorted(notable):
    print("  " + line)
