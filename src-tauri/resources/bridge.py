"""Calibre API bridge. Invoked only with an application operation journal."""
import argparse
import copy
import fcntl
import hashlib
import json
import math
import pathlib
import shutil
import sqlite3
import xml.etree.ElementTree as ET

FIELDS = ("series1", "series2", "series3", "series4")
API_FIELDS = ("series", "#series2", "#series3", "#series4")
OPF_NS = "{http://www.idpf.org/2007/opf}"

def read_database(path):
    connection = sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    return connection

def series_state(connection, columns):
    books = {}
    for row in connection.execute("SELECT id,uuid,title,path,series_index FROM books"):
        books[row["id"]] = {
            "uuid": row["uuid"], "title": row["title"], "path": row["path"],
            "series": {field: {"name": None, "index": None} for field in FIELDS},
        }
        books[row["id"]]["series"]["series1"]["index"] = row["series_index"]
    for row in connection.execute(
        "SELECT l.book,s.name FROM books_series_link l JOIN series s ON s.id=l.series"
    ):
        books[row["book"]]["series"]["series1"]["name"] = row["name"]
    for field, column in columns.items():
        for row in connection.execute(
            f"SELECT l.book,v.value,l.extra FROM books_custom_column_{column}_link l "
            f"JOIN custom_column_{column} v ON v.id=l.value"
        ):
            books[row["book"]]["series"][field] = {
                "name": row["value"], "index": row["extra"],
            }
    return books

def unrelated_hashes(connection, ids, columns, comment_ids=(), bibliography_ids=()):
    """Series semantics are checked separately for every book, not just targets."""
    series_tables = {"series", "books_series_link"}
    for column in columns.values():
        series_tables.update({
            f"custom_column_{column}", f"books_custom_column_{column}_link"
        })
    if bibliography_ids:
        series_tables.update({"publishers", "books_publishers_link"})
    hashes = {}
    for (table,) in connection.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ):
        if table in series_tables or table == "metadata_dirtied":
            continue
        quoted = '"' + table.replace('"', '""') + '"'
        rows = []
        for raw in connection.execute("SELECT * FROM " + quoted):
            row = dict(raw)
            if table == "sqlite_sequence" and row.get("name") in series_tables:
                continue
            if table == "sqlite_sequence" and comment_ids and row.get("name") == "comments":
                continue
            if table == "comments" and row.get("book") in comment_ids:
                continue
            if table == "identifiers" and row.get("book") in bibliography_ids:
                continue
            if table == "sqlite_sequence" and bibliography_ids and row.get("name") == "identifiers":
                continue
            if table == "books" and row["id"] in ids:
                row.pop("series_index")
                row.pop("last_modified")
            if table == "preferences" and row.get("key") == "last_expired_trash_at":
                row.pop("val")
            rows.append(json.dumps(row, sort_keys=True, default=repr))
        hashes[table] = hashlib.sha256(json.dumps(sorted(rows)).encode()).hexdigest()
    return hashes

def category_payloads(connection, columns):
    result = {}
    for table, key in [("series", "name")] + [
        (f"custom_column_{column}", "value") for column in columns.values()
    ]:
        result[table] = {}
        for raw in connection.execute("SELECT * FROM " + table):
            row = dict(raw)
            name = row.pop(key)
            row.pop("id")
            result[table][name] = row
    return result

def write_fields(api, targets, checkpoint):
    for slot, field in enumerate(FIELDS):
        api_field = API_FIELDS[slot]
        updates = {book_id: values[field]["name"] for book_id, values in targets.items()
                   if api.field_for(api_field, book_id) != values[field]["name"]}
        if updates:
            api.set_field(api_field, updates, allow_case_change=False)
            checkpoint()
        index_field = "series_index" if field == "series1" else api_field + "_index"
        updates = {book_id: values[field]["index"] for book_id, values in targets.items()
                   if (field == "series1" or values[field]["name"] is not None)
                   and api.field_for(index_field, book_id) != values[field]["index"]}
        if updates:
            api.set_field(index_field, updates)
            checkpoint()

def verify_opf(path, values):
    tags = ET.parse(path).getroot().findall(".//" + OPF_NS + "meta")
    primary = {tag.get("name"): tag.get("content") for tag in tags}
    if primary.get("calibre:series") != values["series1"]["name"]:
        raise RuntimeError(f"Primary series OPF mismatch: {path}")
    if values["series1"]["name"] is not None:
        if float(primary["calibre:series_index"]) != values["series1"]["index"]:
            raise RuntimeError(f"Primary index OPF mismatch: {path}")
    for slot, field in enumerate(FIELDS[1:], 1):
        raw = primary.get("calibre:user_metadata:" + API_FIELDS[slot])
        data = json.loads(raw) if raw is not None else {}
        if data.get("#value#") != values[field]["name"]:
            raise RuntimeError(f"Custom series OPF mismatch: {field}")
        if values[field]["name"] is not None:
            if data.get("#extra#") != values[field]["index"]:
                raise RuntimeError(f"Custom series index OPF mismatch: {field}")


def atomic(path, data):
    temporary = path.with_suffix(".tmp")
    with temporary.open("w") as output:
        json.dump(data, output, indent=2, sort_keys=True)
        output.flush()
        import os
        os.fsync(output.fileno())
    temporary.replace(path)
    import os
    descriptor = os.open(str(path.parent), os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def mapped(values):
    if len(values) != 4:
        raise ValueError("All four hierarchy fields must be explicit")
    return dict(zip(FIELDS, values))


def safe_opf(library, book):
    directory = (library / book["path"]).resolve()
    if not directory.is_relative_to(library) or directory == library:
        raise ValueError("Book directory escapes its library")
    path = directory / "metadata.opf"
    if path.is_symlink():
        raise ValueError("Metadata OPF must not be a symbolic link")
    return path


def validate(operation, current):
    expected = copy.deepcopy(current)
    changes = operation["changes"]
    ids = {entry["id"] for entry in changes}
    if not changes or len(ids) != len(changes):
        raise ValueError("Empty changes or duplicate book IDs")
    for entry in changes:
        field = operation.get("metadata_field", "series")
        if field not in ("series", "comments", "bibliography", "repair"):
            raise ValueError("Unknown metadata operation field")
        if field == "comments":
            if entry["before"] != entry["after"] or entry.get("comments") is None:
                raise ValueError("Comments operations must preserve all hierarchy fields")
        elif field != "repair" and entry.get("comments") is not None:
            raise ValueError("Series operations cannot contain Comments edits")
        if field == "bibliography":
            if entry["before"] != entry["after"] or entry.get("bibliography") is None:
                raise ValueError("Bibliography operations must preserve hierarchy")
        elif field != "repair" and entry.get("bibliography") is not None:
            raise ValueError("Unexpected bibliography edit")
        book = current.get(entry["id"])
        if book is None or (book["uuid"], book["title"]) != (entry["uuid"], entry["title"]):
            raise ValueError(f"Stale book identity: {entry['id']}")
        if book["series"] != mapped(entry["before"]):
            raise ValueError(f"Stale before values: {entry['id']}")
        if not entry["evidence"].strip() or not entry["sources"]:
            raise ValueError("Reviewed provenance is required")
        for slot, value in enumerate(entry["after"]):
            name, index = value["name"], value["index"]
            if name is None:
                if index != (1.0 if slot == 0 else None):
                    raise ValueError("Invalid empty-series index")
            elif not isinstance(name, str) or not name.strip() or name != name.strip():
                raise ValueError("Invalid series name")
            elif isinstance(index, bool) or not isinstance(index, (float, int)) or not math.isfinite(index):
                raise ValueError("Invalid series index")
        expected[entry["id"]]["series"] = mapped(entry["after"])
    by_uuid = {book["uuid"]: (book_id, book) for book_id, book in current.items()}
    for protection in operation["protections"]:
        if protection["uuid"] not in by_uuid:
            raise ValueError("A protected book was removed")
        book_id, book = by_uuid[protection["uuid"]]
        for slot, locked in enumerate(protection["mask"]):
            if locked and (book["series"][FIELDS[slot]] != protection["values"][slot]
                           or expected[book_id]["series"][FIELDS[slot]] != protection["values"][slot]):
                raise ValueError(f"Protected field changed: {book_id}, slot {slot + 1}")
    return expected, ids


def comments_state(connection):
    return {row["book"]: row["text"] for row in connection.execute("SELECT book,text FROM comments")}


def validate_comments(operation, comments):
    expected = dict(comments)
    for entry in operation["changes"]:
        if entry.get("comments") is None:
            continue
        edit = entry["comments"]
        if set(edit) != {"before", "after"}:
            raise ValueError("Comments edit requires explicit before/after")
        if comments.get(entry["id"]) != edit["before"]:
            raise ValueError("Stale Comments value")
        if operation["kind"] not in ("undo", "recover"):
            if edit["before"] is not None and str(edit["before"]).strip():
                raise ValueError("Existing Comments are protected; overwrite refused")
            if not isinstance(edit["after"], str) or not edit["after"].strip():
                raise ValueError("Synopsis apply requires nonempty Comments")
        if edit["after"] is not None and (not isinstance(edit["after"], str) or len(edit["after"]) > 32000):
            raise ValueError("Invalid Comments target")
        if edit["after"] is None:
            expected.pop(entry["id"], None)
        else:
            expected[entry["id"]] = edit["after"]
    return expected


def verify_comments_opf(path, expected):
    node = ET.parse(path).getroot().find(".//{http://purl.org/dc/elements/1.1/}description")
    actual = "".join(node.itertext()) if node is not None else None
    if actual != expected:
        raise RuntimeError("Synopsis OPF description mismatch")


def bibliography_state(connection):
    values = {row["id"]: {"publisher": None, "identifiers": {}} for row in connection.execute("SELECT id FROM books")}
    for row in connection.execute("SELECT l.book,p.name FROM books_publishers_link l JOIN publishers p ON p.id=l.publisher"):
        values[row["book"]]["publisher"] = row["name"]
    for row in connection.execute("SELECT book,type,val FROM identifiers"):
        values[row["book"]]["identifiers"][row["type"]] = row["val"]
    return values


def validate_bibliography(operation, values):
    expected = copy.deepcopy(values)
    for entry in operation["changes"]:
        if entry.get("bibliography") is None:
            continue
        edit = entry["bibliography"]
        if set(edit) != {"before", "after"} or values[entry["id"]] != edit["before"]:
            raise ValueError("Stale bibliographic values")
        before, after = edit["before"], edit["after"]
        if set(after) != {"publisher", "identifiers"}:
            raise ValueError("Unsupported bibliographic fields")
        publisher = after["publisher"]
        if publisher is not None and (not isinstance(publisher, str) or not publisher.strip() or len(publisher) > 500):
            raise ValueError("Invalid publisher")
        if not isinstance(after["identifiers"], dict) or len(after["identifiers"]) > 100:
            raise ValueError("Invalid identifier map")
        for key, value in after["identifiers"].items():
            if not isinstance(key, str) or not isinstance(value, str) or not key or not value or len(key) > 100 or len(value) > 1000:
                raise ValueError("Invalid identifier")
        if operation["kind"] not in ("undo", "recover"):
            if before["publisher"] and before["publisher"].strip() and publisher != before["publisher"]:
                raise ValueError("Populated publisher is protected")
            for key, value in before["identifiers"].items():
                if value.strip() and after["identifiers"].get(key) != value:
                    raise ValueError("Populated identifiers are protected")
            if any(key not in after["identifiers"] for key in before["identifiers"]):
                raise ValueError("Cannot remove identifiers")
        expected[entry["id"]] = after
    return expected


def verify_bibliography_opf(path, expected):
    root = ET.parse(path).getroot()
    dc = "{http://purl.org/dc/elements/1.1/}"
    publisher = root.find(".//" + dc + "publisher")
    if (publisher.text if publisher is not None else None) != expected["publisher"]:
        raise RuntimeError("Publisher OPF mismatch")
    actual = {}
    for node in root.findall(".//" + dc + "identifier"):
        scheme = node.get(OPF_NS + "scheme", "").lower()
        if scheme and scheme not in ("uuid", "calibre"):
            actual[scheme] = node.text
    if actual != expected["identifiers"]:
        raise RuntimeError("Identifiers OPF mismatch")


def fingerprint(database, library, columns, ids, comment_ids=(), bibliography_ids=()):
    with read_database(database) as connection:
        books = series_state(connection, columns)
        modified = {
            str(book_id): connection.execute("SELECT last_modified FROM books WHERE id=?", (book_id,)).fetchone()[0]
            for book_id in ids
        }
        result = {
            "books": books, "unrelated": unrelated_hashes(connection, ids, columns, comment_ids, bibliography_ids),
            "categories": category_payloads(connection, columns), "modified": modified,
            "opf": {str(book_id): (
                hashlib.sha256(safe_opf(library, books[book_id]).read_bytes()).hexdigest()
                if safe_opf(library, books[book_id]).exists() else None
            ) for book_id in ids},
        }
        if comment_ids:
            result["comments"] = comments_state(connection)
        if bibliography_ids:
            result["bibliography"] = bibliography_state(connection)
            result["publishers"] = {row["name"]: {k: row[k] for k in row.keys() if k not in ("id", "name")}
                                    for row in connection.execute("SELECT * FROM publishers")}
        return result


def execute(journal):
    global API_FIELDS
    operation = json.loads(journal.read_text())
    repair_mode = operation.get("metadata_field") == "repair"
    comment_mode = operation.get("metadata_field", "series") == "comments" or (
        repair_mode and any(entry.get("comments") is not None for entry in operation["changes"]))
    bibliography_mode = operation.get("metadata_field") == "bibliography" or (
        repair_mode and any(entry.get("bibliography") is not None for entry in operation["changes"]))
    folder = journal.parent
    library = pathlib.Path(operation["library"]).resolve(strict=True)
    if folder.is_relative_to(library):
        raise ValueError("Journal must be outside the library")
    database = library / "metadata.db"
    labels = operation["columns"]
    if len(labels) != 3 or len(set(labels)) != 3:
        raise ValueError("Select three distinct custom series columns")
    API_FIELDS = ("series", *("#" + label for label in labels))
    columns = {}
    with read_database(database) as connection:
        for field, label in zip(FIELDS[1:], labels):
            row = connection.execute("SELECT id,datatype FROM custom_columns WHERE label=?", (label,)).fetchone()
            if row is None or row["datatype"] != "series" or int(row["id"]) <= 0:
                raise ValueError("Existing compatible series columns are required")
            columns[field] = int(row["id"])
    def phase(value):
        operation["phase"] = value
        atomic(journal, operation)
        print("CURATOR_PROGRESS:" + json.dumps({"phase": value, "completed": 0,
                                               "total": len(operation["changes"])}), flush=True)
    db = None
    try:
        # Calibre's installed DBCtx acquires singleinstance('db') in __init__.
        # It refuses a running Calibre writer; never bypass this or use a remote API.
        from calibre.db.cli.main import DBCtx, option_parser
        parser = option_parser()
        options, _ = parser.parse_args(["--with-library", str(library)])
        context = DBCtx(options, parser)
        db = context.db
        phase("locked")
        with read_database(database) as connection:
            current = series_state(connection, columns)
            expected, ids = validate(operation, current)
            comment_ids = {entry["id"] for entry in operation["changes"] if entry.get("comments") is not None}
            bibliography_ids = {entry["id"] for entry in operation["changes"] if entry.get("bibliography") is not None}
            expected_comments = validate_comments(operation, comments_state(connection)) if comment_mode else None
            expected_bibliography = validate_bibliography(operation, bibliography_state(connection)) if bibliography_mode else None
            for entry in operation["changes"]:
                authors = [row[0] for row in connection.execute(
                    "SELECT a.name FROM books_authors_link l JOIN authors a ON a.id=l.author "
                    "WHERE l.book=? ORDER BY l.id", (entry["id"],))]
                identifiers = dict(connection.execute(
                    "SELECT type,val FROM identifiers WHERE book=?", (entry["id"],)))
                if authors != entry["authors"] or identifiers != entry["identifiers"]:
                    raise ValueError("Identity metadata changed since the review")
        if operation["kind"] in ("undo", "recover"):
            import uuid
            parent_id = str(uuid.UUID(operation["parent"]))
            parent = folder.parent / parent_id
            guard = parent / "guard.json"
            if not guard.is_file():
                raise ValueError("No durable checkpoint exists; manual recovery review required")
            previous = json.loads((parent / "operation.json").read_text())
            if previous["library"] != str(library) or previous["columns"] != labels:
                raise ValueError("Recovery library/columns do not match")
            if previous.get("metadata_field", "series") != operation.get("metadata_field", "series"):
                raise ValueError("Recovery metadata field does not match")
            original_ids = {entry["id"] for entry in previous["changes"]}
            if ids != original_ids:
                raise ValueError("Recovery must account for every original target")
            if json.loads(guard.read_text()) != json.loads(json.dumps(
                    fingerprint(database, library, columns, ids, comment_ids, bibliography_ids))):
                raise ValueError("Intervening edits or an uncheckpointed write: guarded reversal refused")
            with read_database(parent / "backup" / "metadata.db") as connection:
                if unrelated_hashes(connection, ids, columns, comment_ids, bibliography_ids) != fingerprint(
                        database, library, columns, ids, comment_ids, bibliography_ids)["unrelated"]:
                    raise ValueError("Unrelated metadata changed since the original operation")
            originals = {entry["id"]: entry for entry in previous["changes"]}
            for entry in operation["changes"]:
                if entry.get("comments") is not None:
                    target = originals[entry["id"]]["comments"]["before"]
                    target = None if isinstance(target, str) and not target.strip() else target
                    if entry["comments"]["after"] != target:
                        raise ValueError("Reversal must restore the original Comments")
                if entry.get("bibliography") is not None:
                    if entry["bibliography"]["after"] != originals[entry["id"]]["bibliography"]["before"]:
                        raise ValueError("Reversal must restore original bibliography")
                if entry["after"] != originals[entry["id"]]["before"]:
                    raise ValueError("Reversal must restore the original hierarchy")
        backup = folder / "backup"
        backup.mkdir(exist_ok=False)
        original_opf = backup / "original-opf"
        original_opf.mkdir()
        with read_database(database) as connection:
            with sqlite3.connect(backup / "metadata.db") as destination:
                connection.backup(destination)
        for book_id in ids:
            path = safe_opf(library, current[book_id])
            if path.exists():
                shutil.copy2(path, original_opf / f"{book_id}.opf")
        operation["backup"] = str(backup)
        baseline = fingerprint(database, library, columns, ids, comment_ids, bibliography_ids)
        def checkpoint():
            atomic(folder / "guard.json", fingerprint(database, library, columns, ids, comment_ids, bibliography_ids))
        checkpoint()
        phase("backed_up")
        targets = {entry["id"]: mapped(entry["after"]) for entry in operation["changes"]}
        phase("writing")
        if repair_mode:
            write_fields(db.new_api, targets, checkpoint)
        if comment_mode:
            db.new_api.set_field("comments", {entry["id"]: entry["comments"]["after"] for entry in operation["changes"] if entry.get("comments") is not None})
            checkpoint()
        if bibliography_mode:
            for field in ("publisher", "identifiers"):
                db.new_api.set_field(field, {entry["id"]: entry["bibliography"]["after"][field] for entry in operation["changes"] if entry.get("bibliography") is not None})
                checkpoint()
        if not repair_mode and not comment_mode and not bibliography_mode:
            write_fields(db.new_api, targets, checkpoint)
        for completed, book_id in enumerate(sorted(ids), 1):
            db.new_api.dump_metadata(book_ids={book_id})
            checkpoint()
            verify_opf(safe_opf(library, current[book_id]), targets[book_id])
            if book_id in comment_ids:
                verify_comments_opf(safe_opf(library, current[book_id]), expected_comments.get(book_id))
            if book_id in bibliography_ids:
                verify_bibliography_opf(safe_opf(library, current[book_id]), expected_bibliography[book_id])
            print("CURATOR_PROGRESS:" + json.dumps({"phase": "opf", "completed": completed,
                                                   "total": len(ids)}), flush=True)
        phase("verifying")
        actual = fingerprint(database, library, columns, ids, comment_ids, bibliography_ids)
        if actual["books"] != expected:
            raise RuntimeError("Whole-library hierarchy or identity changed unexpectedly")
        if actual["unrelated"] != baseline["unrelated"]:
            raise RuntimeError("Unrelated metadata changed")
        if comment_mode and actual["comments"] != expected_comments:
            raise RuntimeError("Target/non-target Comments verification failed")
        if bibliography_mode:
            if actual["bibliography"] != expected_bibliography:
                raise RuntimeError("Whole-library bibliography verification failed")
            if not repair_mode and actual["categories"] != baseline["categories"]:
                raise RuntimeError("Series categories changed during bibliography fill")
            for name, payload in baseline["publishers"].items():
                if name in actual["publishers"] and actual["publishers"][name] != payload:
                    raise RuntimeError("Existing publisher category payload changed")
        if comment_mode and not repair_mode and actual["categories"] != baseline["categories"]:
            raise RuntimeError("Series category metadata changed during synopsis apply")
        for table, names in baseline["categories"].items():
            for name, payload in names.items():
                value = actual["categories"][table].get(name)
                if value is not None and value != payload:
                    raise RuntimeError("Existing category payload changed")
                if value is None and any(v for k, v in payload.items() if k != "sort"):
                    raise RuntimeError("Nonempty category payload was removed")
        with read_database(database) as connection:
            if connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise RuntimeError("Database integrity check failed")
        operation["verification"] = {
            "integrity": "ok", "all_book_series_checked": len(current),
            "unrelated_metadata_unchanged": True, "protected_fields_preserved": True,
            "opf_verified": len(ids),
            "all_book_comments_checked": len(current) if comment_mode else 0,
            "all_book_bibliography_checked": len(current) if bibliography_mode else 0,
            "metadata_field": operation.get("metadata_field", "series"),
        }
        checkpoint()
        phase("verified")
    except BaseException as error:
        operation["error"] = str(error)
        operation["phase"] = "failed" if operation.get("backup") else "rejected"
        atomic(journal, operation)
        raise
    finally:
        if db is not None:
            db.close()
    return operation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("journal", type=pathlib.Path)
    args = parser.parse_args()
    journal = args.journal.resolve(strict=True)
    # Also serialize different Curator windows/processes using this workspace.
    with (journal.parent.parent.parent / "writer.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        result = execute(journal)
        print("CURATOR_RESULT:" + json.dumps(result), flush=True)


if __name__ == "__main__":
    main()
