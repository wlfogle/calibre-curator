"""Create only the disposable library named by CURATOR_TEST_LIBRARY."""
import os
from calibre.db.legacy import LibraryDatabase
from calibre.ebooks.metadata.book.base import Metadata

db = LibraryDatabase(os.environ["CURATOR_TEST_LIBRARY"])
for label in ("series2", "series3", "series4"):
    db.new_api.create_custom_column(label, label, "series", False)
db.close()
db = LibraryDatabase(os.environ["CURATOR_TEST_LIBRARY"])
api = db.new_api
ids, _ = api.add_books([(Metadata(title, ["Test Author"]), {}) for title in
                       ("Promote me", "Leave me", "Protected Foundation", "Ungrouped")])
ids = sorted(ids)
api.set_field("series", {ids[0]: "Original Cycle", ids[1]: "Original Cycle",
                         ids[2]: "Greater Foundation Universe"})
api.set_field("series_index", {ids[0]: 2.0, ids[1]: 3.0, ids[2]: 9.0})
api.set_field("#series2", {ids[0]: "Original Branch"})
api.set_field("#series2_index", {ids[0]: 4.0})
api.set_field("#series4", {ids[0]: "Retained Detail"})
api.set_field("#series4_index", {ids[0]: 8.0})
api.set_field("comments", {book_id: "<p>Keep synopsis.</p>" for book_id in ids})
api.dump_metadata(book_ids=ids)
db.close()
