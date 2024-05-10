import contextlib
import difflib
import os
import subprocess

# Retrieve all subfolders of the current folder to run tests in.
subfolders = [f.name for f in os.scandir(".") if f.is_dir()]

# Those markers are emitted by scrutinizer to indicate where the lint output starts and ends.
LINTS_START_MARKER = "--LINTSSTART--"
LINTS_END_MARKER = "--LINTSEND--"

n_tests = 0
n_passed = 0
n_failed = 0

print("\033[95m-- Starting scrutinizer test runner...\033[0m", flush=True)

for subfolder in subfolders:
    with contextlib.chdir(subfolder):
        # Skip if the folder is not a Cargo crate.
        if not os.path.isfile("Cargo.toml"):
            continue

        n_tests += 1
        print(f"Testing crate '{subfolder}' ... ", end="", flush=True)

        # Run dylint.
        ret = subprocess.run(["cargo", "+nightly-2023-04-12",
                              "dylint",
                              "-q",
                              "--all",
                              "--",
                              "-Z", "build-std",
                              "--target", "aarch64-apple-darwin"], env=dict(os.environ) | {
            "RUSTFLAGS": "-Z always-encode-mir -Z nll-facts"}, capture_output=True)

        # Parse output between the markers.
        received_stderr = ret.stderr.decode().split("\n")

        try:
            start_pos = received_stderr.index(LINTS_START_MARKER)
            end_pos = received_stderr.index(LINTS_END_MARKER)
        except ValueError:
            print("\033[93mrunning `cargo dylint` failed.\033[0m", flush=True)
            print("\033[93mChild process output:", flush=True)
            print(ret.stderr.decode() + "\033[0m")
            continue

        received_stderr = received_stderr[start_pos+1:end_pos]

        # This is necessary to avoid a corner case when splitting an empty string yields [""]
        if len(received_stderr) == 0:
            received_stderr = [""]

        # Read correct test output.
        try:
            with open(f"{subfolder}.stderr", "r") as desired_stderr_file:
                desired_stderr = desired_stderr_file.read().split("\n")
        except OSError:
            desired_stderr = [""]

        # Check for correctness.
        if desired_stderr == received_stderr:
            n_passed += 1
            print("\033[92mOK\033[0m", flush=True)
        else:
            n_failed += 1
            print(f"\033[91mFAIL\033[0m, diff will be written to {
                  subfolder}.diff.stderr", flush=True)

            # Calculate a diff between two files.
            diff = difflib.unified_diff(
                desired_stderr, received_stderr, fromfile="desired.stderr", tofile="received.stderr")
            printable_diff = os.linesep.join([line.strip() for line in diff])

            # Write received stderr to file.
            with open(f"{subfolder}.received.stderr", "w") as received_stderr_file:
                received_stderr_file.write(os.linesep.join(received_stderr))

            # Write diff to file.
            with open(f"{subfolder}.diff.stderr", "w") as diff_stderr_file:
                diff_stderr_file.write(printable_diff)

print(f"\033[95m-- Ran {n_tests} tests, {
      n_passed} passed, {n_failed} failed.\033[0m")
