// Prints the current macOS thermal pressure level and exits.
// Confirmed on this machine (2026-09-05) to need zero elevated privilege,
// unlike `powermetrics --samplers thermal` which always requires root.
// Copied from the imagegen project (scripts/thermal_state.swift); used by scripts/cool_run.sh.
//
// Build: swiftc -O thermal_state.swift -o thermal_state (cool_run.sh does it when missing)
//
// Output: one of nominal | fair | serious | critical (stdout, exit 0),
// or "unknown" + non-zero exit if the API is ever unavailable.
// Options: -h | --help prints the usage (exit 0); any other argument is an error (exit 2).

import Foundation

let usage = """
Usage: thermal_state [-h | --help]

Print the current macOS thermal pressure level and exit: one of
nominal | fair | serious | critical on stdout with status 0, or "unknown" with
status 1 if the API is unavailable. Needs no privileges and reads no files.

Options:
  -h, --help    print this help and exit
"""

for arg in CommandLine.arguments.dropFirst() {
    if arg == "-h" || arg == "--help" {
        print(usage)
        exit(0)
    }
    FileHandle.standardError.write("thermal_state: unknown option: \(arg) (see --help)\n".data(using: .utf8)!)
    exit(2)
}

let names = ["nominal", "fair", "serious", "critical"]
let state = ProcessInfo.processInfo.thermalState.rawValue

if state >= 0 && state < names.count {
    print(names[state])
    exit(0)
} else {
    print("unknown")
    exit(1)
}
