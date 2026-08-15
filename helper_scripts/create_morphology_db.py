import sqlite3

import pymorphy3

morph = pymorphy3.MorphAnalyzer(lang="ru")
conn = sqlite3.connect("../morphology.db")
cursor = conn.cursor()

cursor.execute("DROP TABLE IF EXISTS dictionary")
cursor.execute("CREATE TABLE dictionary (word TEXT, tag TEXT)")

print("Extracting dictionary words from local DAWG files...")

# Option A: Iterate through all known word parses in the dictionary
count = 0
for parse in morph.iter_known_word_parses():
    cursor.execute("INSERT INTO dictionary VALUES (?, ?)", (parse.word, str(parse.tag)))
    count += 1
    if count % 100000 == 0:
        print(f"Processed {count} entries...")

conn.commit()
conn.close()

print(f"\nSuccess! Successfully exported {count} word forms into morphology.db")
