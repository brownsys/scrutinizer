import contextlib
import difflib
import os
import subprocess

subfolders = [f.name for f in os.scandir(".") if f.is_dir()]
LINTS_START_MARKER = "--LINTSSTART--"
LINTS_END_MARKER = "--LINTSEND--"

for subfolder in subfolders:
    with contextlib.chdir(subfolder):
        print(f"testing crate '{subfolder}' ... ", end="", flush=True)
        ret = subprocess.run(["cargo", "+nightly-2023-04-12",
                              "dylint",
                              "-q",
                              "--all",
                              "--",
                              "-Z", "build-std",
                              "--target", "aarch64-apple-darwin"], env=dict(os.environ) | {
            "RUSTFLAGS": "-Z always-encode-mir -Z nll-facts"}, capture_output=True)

        received_stderr = ret.stderr.decode().split("\n")
        start_pos = received_stderr.index(LINTS_START_MARKER)
        end_pos = received_stderr.index(LINTS_END_MARKER)
        received_stderr = received_stderr[start_pos+1:end_pos]

        # This is necessary to avoid a corner case when splitting an empty string yields [""]
        if len(received_stderr) == 0:
            received_stderr = [""] 

        with open(f"{subfolder}.stderr", "r") as desired_stderr_file:
            desired_stderr = desired_stderr_file.read().split("\n")

        if desired_stderr == received_stderr:
            print("\033[92mOK\033[0m", flush=True)
        else:
            print(f"\033[91mFAIL\033[0m, diff will be written to {
                  subfolder}.diff.stderr", flush=True)

            diff = difflib.unified_diff(
                desired_stderr, received_stderr, fromfile="desired.stderr", tofile="received.stderr")
            printable_diff = os.linesep.join([line.strip() for line in diff])

            with open(f"{subfolder}.received.stderr", "w") as received_stderr_file:
                received_stderr_file.write(os.linesep.join(received_stderr))

            with open(f"{subfolder}.diff.stderr", "w") as diff_stderr_file:
                diff_stderr_file.write(printable_diff)
