"""Start one disposable browser; this short-lived launcher then exits."""
import os
import subprocess
from pathlib import Path

browser = os.environ["HEADLESS_SHELL"]
profile = os.environ["HEADLESS_REAPER_PATH"]
if not profile.startswith("/tmp/reaper-demo."):
    raise SystemExit("Expected a dedicated /tmp/reaper-demo.* profile")
process = subprocess.Popen(
    [browser, "--headless", "--no-sandbox", "--disable-gpu",
     "--remote-debugging-port=0", "--user-data-dir=" + profile, "about:blank"],
    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    start_new_session=True,
)
Path(profile, "demo-browser.pid").write_text(str(process.pid))
print("Started a disposable browser; its launcher now exits.")
