#!/usr/bin/env python3
"""
Build sentences.db from the Tatoeba rus_sentences.tsv export.

Expected input format (tab-separated, no header), as downloaded from
https://tatoeba.org/en/downloads for the Russian sentences dump:

    id<TAB>lang<TAB>text

Usage:
    python build_quotes_db.py
    python build_quotes_db.py --input ../rus_sentences.tsv --output ../sentences.db

Produces sentences.db with:
    quotes(id INTEGER PRIMARY KEY, text TEXT NOT NULL)
    quotes_fts (FTS5 index over quotes.text, content-linked to quotes)
"""

import argparse
import csv
import sqlite3
import sys
from pathlib import Path

SCHEMA = """
CREATE TABLE IF NOT EXISTS quotes (
    id   INTEGER PRIMARY KEY,
    text TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS quotes_fts USING fts5(
    text,
    content='quotes',
    content_rowid='id',
    tokenize='unicode61'
);

CREATE TRIGGER IF NOT EXISTS quotes_ai AFTER INSERT ON quotes BEGIN
    INSERT INTO quotes_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER IF NOT EXISTS quotes_ad AFTER DELETE ON quotes BEGIN
    INSERT INTO quotes_fts(quotes_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER IF NOT EXISTS quotes_au AFTER UPDATE ON quotes BEGIN
    INSERT INTO quotes_fts(quotes_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO quotes_fts(rowid, text) VALUES (new.id, new.text);
END;
"""


def build(input_path: Path, output_path: Path, batch_size: int = 5000) -> None:
    if not input_path.exists():
        sys.exit(f"input file not found: {input_path}")

    if output_path.exists():
        output_path.unlink()

    con = sqlite3.connect(output_path)
    con.executescript(SCHEMA)

    inserted = 0
    skipped = 0
    batch = []

    with input_path.open(encoding="utf-8", newline="") as f:
        reader = csv.reader(f, delimiter="\t")
        for row in reader:
            if len(row) < 3:
                skipped += 1
                continue

            sid_raw, lang, text = row[0], row[1], row[2]
            text = text.strip()

            if lang.strip().lower() not in ("rus", "ru"):
                skipped += 1
                continue
            if not text:
                skipped += 1
                continue

            try:
                sid = int(sid_raw)
            except ValueError:
                skipped += 1
                continue

            batch.append((sid, text))
            if len(batch) >= batch_size:
                con.executemany(
                    "INSERT OR IGNORE INTO quotes (id, text) VALUES (?, ?)", batch
                )
                inserted += len(batch)
                batch.clear()
                print(f"  ...{inserted} inserted", end="\r", file=sys.stderr)

    if batch:
        con.executemany("INSERT OR IGNORE INTO quotes (id, text) VALUES (?, ?)", batch)
        inserted += len(batch)

    con.commit()

    total = con.execute("SELECT COUNT(*) FROM quotes").fetchone()[0]
    fts_total = con.execute("SELECT COUNT(*) FROM quotes_fts").fetchone()[0]

    print(f"\ndone: {input_path} -> {output_path}")
    print(f"  rows read/inserted (post-filter): {inserted}, skipped: {skipped}")
    print(f"  quotes table rows: {total}")
    print(f"  quotes_fts rows:   {fts_total}")

    con.execute("PRAGMA optimize")
    con.execute("VACUUM")
    con.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "rus_sentences.tsv",
        help="Path to rus_sentences.tsv (default: ../rus_sentences.tsv relative to this script)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "sentences.db",
        help="Path to write sentences.db (default: ../sentences.db relative to this script)",
    )
    args = parser.parse_args()
    build(args.input, args.output)


if __name__ == "__main__":
    main()
