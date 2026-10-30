"""Install the repository build's application-menu entry for the current user."""
import os
import pathlib
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    binary = ROOT / "target/release/calibre-curator"
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit("Build the application with pnpm package first.")
    data = pathlib.Path(os.environ.get("XDG_DATA_HOME", str(pathlib.Path.home() / ".local/share")))
    launcher = data / "applications/calibre-curator.desktop"
    launcher.parent.mkdir(parents=True, exist_ok=True)
    quoted = str(binary).replace("\\", "\\\\").replace('"', '\\"').replace("$", "\\$").replace("`", "\\`")
    launcher.write_text(
        "[Desktop Entry]\nVersion=1.0\nType=Application\nName=Calibre Curator\n"
        "Comment=Local source-backed Calibre universe and subseries curation\n"
        f'Exec="{quoted}"\nTryExec={binary}\n'
        f"Icon={ROOT / 'src-tauri/icons/128x128.png'}\nTerminal=false\n"
        "Categories=Office;\nKeywords=Calibre;Books;Series;Metadata;Chronology;\nStartupNotify=true\n")
    if shutil.which("desktop-file-validate"):
        subprocess.run(["desktop-file-validate", str(launcher)], check=True)
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(launcher.parent)], check=True)
    print(f"Application-menu entry: {launcher}")


if __name__ == "__main__":
    main()
