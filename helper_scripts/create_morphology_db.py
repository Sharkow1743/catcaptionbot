import sqlite3

import pymorphy3

morph = pymorphy3.MorphAnalyzer(lang="ru")
conn = sqlite3.connect("../morphology.db")
cursor = conn.cursor()

# 1. Re-create dictionary table with `lemma` and `word_rev` columns
cursor.execute("DROP TABLE IF EXISTS dictionary")
cursor.execute("""
    CREATE TABLE dictionary (
        word TEXT NOT NULL,
        lemma TEXT NOT NULL,
        tag TEXT NOT NULL,
        word_rev TEXT NOT NULL
    )
""")

print("Extracting dictionary words from local DAWG files...")

count = 0
batch = []

# 2. Store (word, lemma, tag, word_rev)
# word_rev is `word` reversed character-by-character — lets us index suffix
# lookups (e.g. "ends with ние") as a prefix search on the reversed string,
# which SQLite *can* use a B-tree index for (a plain `word LIKE '%ние'` cannot).
for parse in morph.iter_known_word_parses():
    word = parse.word
    batch.append((word, parse.normal_form, str(parse.tag), word[::-1]))
    count += 1

    if len(batch) >= 100000:
        cursor.executemany("INSERT INTO dictionary VALUES (?, ?, ?, ?)", batch)
        batch = []
        print(f"Processed {count} entries...")

if batch:
    cursor.executemany("INSERT INTO dictionary VALUES (?, ?, ?, ?)", batch)

print("Creating database indexes for instant lookup speed...")
cursor.execute("CREATE INDEX IF NOT EXISTS idx_word ON dictionary(word)")
cursor.execute("CREATE INDEX IF NOT EXISTS idx_lemma ON dictionary(lemma)")
cursor.execute("CREATE INDEX IF NOT EXISTS idx_word_rev ON dictionary(word_rev)")

conn.commit()
conn.close()

print(f"\nSuccess! Exported {count} word forms into morphology.db")
