"""Full snapshot tests operate exclusively on temporary fictional libraries."""
import importlib.util
import json
import pathlib
import sqlite3
import tempfile
import unittest
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("curator_bridge", ROOT / "src-tauri/resources/bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class FullBackupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.library = self.root / "library"
        self.library.mkdir()
        with sqlite3.connect(self.library / "metadata.db") as db:
            db.execute("CREATE TABLE fixture(value TEXT)")
            db.execute("INSERT INTO fixture VALUES('original')")
        directory = self.library / "Author" / "Book"
        directory.mkdir(parents=True)
        (directory / "book.epub").write_bytes(b"Fictional ebook contents")
        (directory / "metadata.opf").write_text("<metadata>fixture</metadata>")
        (self.library / "empty").mkdir()
        (self.library / "notes.txt").write_text("Auxiliary library file")
        self.backup = self.root / "backup"

    def test_complete_independent_verified_snapshot(self):
        result = bridge.full_library_backup(self.library, self.backup)
        snapshot = self.backup / "library"
        self.assertTrue(result["verified"])
        self.assertEqual(result["coverage"], "full-library")
        for name, entry in result["files"].items():
            self.assertEqual(bridge.file_hash(snapshot / name), entry["sha256"])
            self.assertNotEqual((snapshot / name).stat().st_ino, (self.library / name).stat().st_ino)
        self.assertTrue((snapshot / "empty").is_dir())
        with sqlite3.connect(snapshot / "metadata.db") as db:
            self.assertEqual(db.execute("SELECT value FROM fixture").fetchone()[0], "original")
        self.assertEqual(json.loads((self.backup / "manifest.json").read_text())["files"], result["files"])
        bridge.verify_full_backup(self.backup)
        (self.library / "Author/Book/book.epub").write_bytes(b"changed after backup")
        self.assertEqual((snapshot / "Author/Book/book.epub").read_bytes(), b"Fictional ebook contents")
        (snapshot / "notes.txt").write_text("corrupt backup")
        with self.assertRaisesRegex(ValueError, "backup file hash changed"):
            bridge.verify_full_backup(self.backup)

    def test_insufficient_space_refuses_before_copy(self):
        with patch.object(bridge.shutil, "disk_usage") as usage:
            usage.return_value.free = 0
            with self.assertRaisesRegex(ValueError, "Insufficient backup space"):
                bridge.full_library_backup(self.library, self.backup)
        self.assertFalse(self.backup.exists())

    def test_copy_failure_never_certifies_a_backup(self):
        before = bridge.file_hash(self.library / "metadata.db")
        with patch.object(bridge.shutil, "copy2", side_effect=OSError("fixture copy failure")):
            with self.assertRaisesRegex(OSError, "fixture copy failure"):
                bridge.full_library_backup(self.library, self.backup)
        self.assertFalse((self.backup / "manifest.json").exists())
        self.assertEqual(bridge.file_hash(self.library / "metadata.db"), before)

    def test_committed_wal_pages_are_in_the_consistent_database_backup(self):
        writer = sqlite3.connect(self.library / "metadata.db")
        self.addCleanup(writer.close)
        writer.execute("PRAGMA journal_mode=WAL")
        writer.execute("INSERT INTO fixture VALUES('committed WAL row')")
        writer.commit()
        bridge.full_library_backup(self.library, self.backup)
        with sqlite3.connect(self.backup / "library/metadata.db") as db:
            self.assertEqual(db.execute("SELECT count(*) FROM fixture").fetchone()[0], 2)
        self.assertFalse((self.backup / "library/metadata.db-wal").exists())

    def test_wal_only_mutation_during_copy_refuses_before_writes(self):
        writer = sqlite3.connect(self.library / "metadata.db")
        self.addCleanup(writer.close)
        writer.execute("PRAGMA journal_mode=WAL")
        original = bridge.durable_copy
        def concurrent_write(source, destination):
            original(source, destination)
            if source.name == "notes.txt":
                writer.execute("INSERT INTO fixture VALUES('concurrent')")
                writer.commit()
        with patch.object(bridge, "durable_copy", side_effect=concurrent_write):
            with self.assertRaisesRegex(ValueError, "database changed|inventory changed"):
                bridge.full_library_backup(self.library, self.backup)
        self.assertFalse((self.backup / "manifest.json").exists())

    def test_symlinks_and_concurrent_changes_refused(self):
        link = self.library / "linked"
        link.symlink_to(self.library / "notes.txt")
        with self.assertRaisesRegex(ValueError, "symbolic links"):
            bridge.full_library_backup(self.library, self.backup)
        link.unlink()
        original = bridge.durable_copy
        def changing_copy(source, destination):
            original(source, destination)
            if source.name == "notes.txt":
                source.write_text("concurrent mutation")
        with patch.object(bridge, "durable_copy", side_effect=changing_copy):
            with self.assertRaisesRegex(ValueError, "changed during backup"):
                bridge.full_library_backup(self.library, self.backup)
        self.assertFalse((self.backup / "manifest.json").exists())


if __name__ == "__main__":
    unittest.main()
