"""Actual release-window smoke test; needs X11, xdotool, ImageMagick, tesseract."""
import csv
import hashlib
import io
import json
import os
import pathlib
import re
import sqlite3
import statistics
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent


def wait(predicate, seconds=15):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        result = predicate()
        if result:
            return result
        time.sleep(0.3)
    raise RuntimeError("Native window condition timed out")


def main():
    folder = pathlib.Path(tempfile.mkdtemp(prefix="calibre-curator-window-"))
    library = folder / "library"
    library.mkdir()
    env = dict(os.environ, CURATOR_TEST_LIBRARY=str(library),
               CURATOR_LIBRARY=str(library), CURATOR_WORKSPACE=str(folder / "workspace"))
    subprocess.run(["/usr/bin/calibre-debug", "-e", str(ROOT / "tests/fixture.py")],
                   env=env, check=True, capture_output=True)
    before = hashlib.sha256((library / "metadata.db").read_bytes()).hexdigest()
    log = (folder / "native.log").open("w")
    process = subprocess.Popen([str(ROOT / "target/release/calibre-curator")], env=env,
                               stdout=log, stderr=log)
    lock = None
    try:
        def window_id():
            result = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid),
                                     "--name", "Calibre Curator"], capture_output=True, text=True)
            return result.stdout.splitlines()[0] if result.returncode == 0 else None
        window = wait(window_id)
        subprocess.run(["xdotool", "windowactivate", "--sync", window], check=True)

        def words():
            path = folder / "current.png"
            subprocess.run(["import", "-window", window, str(path)], check=True)
            output = subprocess.check_output(["tesseract", str(path), "stdout", "tsv"],
                                             stderr=subprocess.DEVNULL, text=True)
            contrast = folder / "ocr.png"
            subprocess.run(["convert", str(path), "-colorspace", "Gray", "-threshold", "65%", "-negate", str(contrast)], check=True)
            extra = subprocess.check_output(["tesseract", str(contrast), "stdout", "--psm", "11", "tsv"],
                                            stderr=subprocess.DEVNULL, text=True)
            return [row for row in csv.DictReader(io.StringIO(output), delimiter="\t")
                    if row["level"] == "5" and row["text"].strip()] + [
                    row for row in csv.DictReader(io.StringIO(extra), delimiter="\t")
                    if row["level"] == "5" and row["text"].strip()]

        def clean(value):
            return re.sub("[^a-z0-9]", "", value.lower())

        def locate(text, last=False):
            tokens = [clean(word) for word in text.split() if clean(word)]
            found = [word for word in words() if clean(word["text"])]
            target = "".join(tokens)
            matches = []
            for i in range(len(found)):
                for count in range(1, len(tokens) + 4):
                    group = found[i:i + count]
                    combined = "".join(clean(word["text"]) for word in group)
                    if combined == target:
                        left = min(int(w["left"]) for w in group)
                        right = max(int(w["left"]) + int(w["width"]) for w in group)
                        baseline = int(statistics.median(int(w["top"]) + int(w["height"]) / 2 for w in group))
                        matches.append(((left + right) // 2, baseline))
                        break
                    if not target.startswith(combined):
                        break
            return (matches[-1] if last else matches[0]) if matches else None

        def click(text, scroll=False, last=False):
            point = None
            for _ in range(12 if scroll else 1):
                point = locate(text,last)
                if point:
                    break
                subprocess.run(["xdotool", "mousemove", "--window", window, "650", "550",
                                 "click", "--repeat", "3", "--delay", "100", "5"], check=True)
            if not point:
                raise RuntimeError(f"Native control not visible: {text}")
            print(f"Native click: {text} at {point}", flush=True)
            subprocess.run(["xdotool", "mousemove", "--sync", "--window", window, str(point[0]), str(point[1])], check=True)
            subprocess.run(["xdotool", "click", "1"], check=True)
            time.sleep(0.4)

        def edit(label, value):
            click(label, scroll=True)
            subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+a",
                             "type", "--clearmodifiers", value], check=True)

        wait(lambda: locate("4 books"))
        subprocess.run(["import", "-window", window, str(folder / "audit.png")], check=True)
        click("View")
        wait(lambda: locate("Create reviewed reference"))
        click("Create reviewed reference")
        edit("Series: overarching universe", "Test Universe")
        edit("Independent number", "1")
        edit("Evidence / explicit user decision", "Known disposable fixture: explicit whole-book position.")
        click("reviewed this evidence and approve this rule", scroll=True)
        click("Save reference", scroll=True)

        def saved():
            with sqlite3.connect(folder / "workspace/state.sqlite3") as db:
                data = json.loads(db.execute("SELECT value FROM state WHERE key='app'").fetchone()[0])
            return next((r for r in data["references"] if r["title"] == "Promote me"
                         and r["approved"] and r["hierarchy"][0] == {"name": "Test Universe", "index": 1}), None)
        wait(saved)
        click("Select eligible on this page")
        click("Review selected")
        wait(lambda: locate("Test Universe"))
        subprocess.run(["import", "-window", window, str(folder / "review.png")], check=True)
        lock = subprocess.Popen(["/usr/bin/calibre-debug", "-c",
            "from calibre.utils.lock import singleinstance; import time; held=singleinstance('db'); "
            "print('LOCK_STATE_READY',flush=True); time.sleep(60) if held else None"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        for line in lock.stdout:
            if line.strip() == "LOCK_STATE_READY":
                break
        click("Apply checked changes", scroll=True)
        click("Confirm",last=True)
        wait(lambda: locate("Another calibre program"))
        click("History")
        wait(lambda: locate("rejected"))
        subprocess.run(["import", "-window", window, str(folder / "lock-refused.png")], check=True)
        if hashlib.sha256((library / "metadata.db").read_bytes()).hexdigest() != before:
            raise RuntimeError("Lock-refused native test changed the disposable database")
        print(json.dumps({"status": "passed", "folder": str(folder),
                          "checks": ["native audit", "reference edit/approval", "field review",
                                     "production writer-lock refusal", "history", "database unchanged"]}))
    finally:
        if lock is not None and lock.poll() is None:
            lock.terminate()
            lock.wait(timeout=10)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        log.close()
        print(f"Native test files: {folder}")


if __name__ == "__main__":
    main()
