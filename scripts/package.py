"""Build the Tauri application and a Debian package without file watchers."""
import gzip
import json
import pathlib
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def main():
    run("pnpm", "install", "--frozen-lockfile")
    run("pnpm", "build")
    run("cargo", "build", "--locked", "--release", "--workspace",
        "--features", "calibre-curator/custom-protocol")
    release = ROOT / "target" / "release"
    resource = release / "resources"
    resource.mkdir(exist_ok=True)
    shutil.copy2(ROOT / "src-tauri/resources/bridge.py", resource / "bridge.py")
    config = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())
    architecture = subprocess.check_output(["dpkg", "--print-architecture"], text=True).strip()
    output = release / "bundle" / "deb"
    output.mkdir(parents=True, exist_ok=True)
    package = output / f"calibre-curator_{config['version']}_{architecture}.deb"
    with tempfile.TemporaryDirectory(prefix="deb-stage-", dir=ROOT / "target") as temporary:
        stage = pathlib.Path(temporary)
        files = {
            release / "calibre-curator": "usr/bin/calibre-curator",
            release / "curator": "usr/bin/calibre-curator-audit",
            resource / "bridge.py": "usr/lib/calibre-curator/resources/bridge.py",
            ROOT / "src-tauri/icons/128x128.png": "usr/share/icons/hicolor/128x128/apps/calibre-curator.png",
            ROOT / "LICENSE": "usr/share/doc/calibre-curator/copyright",
        }
        for source, destination in files.items():
            destination = stage / destination
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        desktop = stage / "usr/share/applications/calibre-curator.desktop"
        desktop.parent.mkdir(parents=True, exist_ok=True)
        desktop.write_text(
            "[Desktop Entry]\nType=Application\nName=Calibre Curator\n"
            "Comment=Local source-backed Calibre metadata curation\n"
            "Exec=calibre-curator\nIcon=calibre-curator\nTerminal=false\nCategories=Office;\n")
        (stage / "usr/share/doc/calibre-curator/README.gz").write_bytes(
            gzip.compress((ROOT / "README.md").read_bytes(), mtime=0))
        control = stage / "DEBIAN/control"
        control.parent.mkdir()
        size = sum(p.stat().st_size for p in stage.rglob("*") if p.is_file()) // 1024
        control.write_text(
            f"Package: calibre-curator\nVersion: {config['version']}\n"
            f"Architecture: {architecture}\nInstalled-Size: {size}\n"
            "Maintainer: Calibre Curator contributors <noreply@example.invalid>\n"
            "Section: utils\nPriority: optional\n"
            "Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libglib2.0-0, libc6 (>= 2.35), libgcc-s1, libstdc++6\n"
            "Recommends: calibre\n"
            "Description: Local source-backed Calibre universe and subseries curation\n"
            " Review hierarchy diffs, preserve protected numbering, and apply with\n"
            " Calibre API backups, semantic verification and guarded recovery.\n")
        run("dpkg-deb", "--build", "--root-owner-group", str(stage), str(package))
    print(f"Executable: {release / 'calibre-curator'}")
    print(f"Debian package: {package}")


if __name__ == "__main__":
    main()
