import sqlite3

import pymorphy3

morph = pymorphy3.MorphAnalyzer(lang="ru")
conn = sqlite3.connect("../morphology.db")
cursor = conn.cursor()

# 1. Re-create dictionary table with `lemma` column
cursor.execute("DROP TABLE IF EXISTS dictionary")
cursor.execute("""
    CREATE TABLE dictionary (
        word TEXT NOT NULL,
        lemma TEXT NOT NULL,
        tag TEXT NOT NULL
    )
""")

print("Extracting dictionary words from local DAWG files...")

count = 0
batch = []

# 2. Store (word, lemma, tag)
for parse in morph.iter_known_word_parses():
    batch.append((parse.word, parse.normal_form, str(parse.tag)))
    count += 1

    if len(batch) >= 100000:
        cursor.executemany("INSERT INTO dictionary VALUES (?, ?, ?)", batch)
        batch = []
        print(f"Processed {count} entries...")

if batch:
    cursor.executemany("INSERT INTO dictionary VALUES (?, ?, ?)", batch)

print("Creating database indexes for instant lookup speed...")
cursor.execute("CREATE INDEX IF NOT EXISTS idx_word ON dictionary(word)")
cursor.execute("CREATE INDEX IF NOT EXISTS idx_lemma ON dictionary(lemma)")

conn.commit()
conn.close()

print(f"\nSuccess! Exported {count} word forms into morphology.db")
