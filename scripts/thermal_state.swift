// Prints the current macOS thermal pressure level and exits.
// Confirmed on this machine (2026-09-05) to need zero elevated privilege,
// unlike `powermetrics --samplers thermal` which always requires root.
// Copied from the imagegen project (scripts/thermal_state.swift); used by scripts/cool_run.sh.
//
// Build: swiftc -O thermal_state.swift -o thermal_state (cool_run.sh does it when missing)
//
// Output: one of nominal | fair | serious | critical (stdout, exit 0),
// or "unknown" + non-zero exit if the API is ever unavailable.

import Foundation

let names = ["nominal", "fair", "serious", "critical"]
let state = ProcessInfo.processInfo.thermalState.rawValue

if state >= 0 && state < names.count {
    print(names[state])
    exit(0)
} else {
    print("unknown")
    exit(1)
}
