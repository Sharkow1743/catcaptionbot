import argparse
import math
import os
import sqlite3
from enum import Enum

import numpy as np
import onnxruntime as ort
import pymorphy3
import sqlite_vec
from tokenizers import Tokenizer


class POS(Enum):
    NOUN = "NOUN"
    ADJ = "ADJ"
    VERB = "VERB"
    PRT = "PRT"
    GRND = "GRND"
    ADV = "ADV"
    COMP = "COMP"
    PRED = "PRED"
    NPRO = "NPRO"
    NUMR = "NUMR"
    MISC = "MISC"
    UNKNOWN = "UNKNOWN"


def get_pos_weight_and_combined_pos(pos_str: str) -> tuple[float, POS]:
    """
    Maps pymorphy3 POS tags to POS enum and assigns weights optimized
    for hilarious and punchy phrase generation.
    """
    match pos_str:
        case "NOUN":
            return 1.3, POS.NOUN
        case "ADJF" | "ADJS":
            return 1.3, POS.ADJ
        case "VERB" | "INFN":
            return 1.0, POS.VERB
        case "PRTF" | "PRTS":
            return 1.0, POS.PRT
        case "ADVB":
            return 1.1, POS.ADV
        case "GRND":
            return 1.0, POS.GRND
        case "PREP" | "CONJ" | "PRCL" | "INTJ":
            return 0.3, POS.MISC
        case "COMP":
            return 0.8, POS.COMP
        case "PRED":
            return 0.7, POS.PRED
        case "NPRO":
            return 0.0, POS.NPRO
        case "NUMR":
            return 0.0, POS.NUMR
        case "UNKNOWN":
            return 0.0, POS.UNKNOWN
        case _:
            return 0.2, POS.UNKNOWN


def get_rank_weight(rank_index: int) -> float:
    log_rank = math.log(max(1, rank_index))

    log_peak_main = math.log(2500)
    sigma_main = 1.4 if log_rank <= log_peak_main else 1.7
    diff_main = log_rank - log_peak_main
    main_curve = math.exp(-(diff_main**2) / (2 * (sigma_main**2)))

    log_peak_bump = math.log(10738)
    sigma_bump = 0.35
    diff_bump = log_rank - log_peak_bump
    bump_curve = math.exp(-(diff_bump**2) / (2 * (sigma_bump**2)))

    base_weight = 0.15
    main_amplitude = 1.15
    bump_amplitude = 0.25

    return base_weight + (main_amplitude * main_curve) + (bump_amplitude * bump_curve)


def get_len_weight(length: int) -> float:
    if length <= 2:
        return 0.2
    elif length <= 4:
        return 0.7
    elif length <= 10:
        return 1.3
    elif length <= 14:
        return 1.0
    else:
        return 0.5


def get_custom_weight(word: str) -> float | None:
    if word in ["се", "кот", "сборка", "обновить"]:
        return 2.2
    return None


def is_valid_word(word: str) -> bool:
    """Ensures input word is alphabetic or hyphenated Cyrillic."""
    if not word or len(word) <= 1:
        return False
    return all(c.isalpha() or c == "-" for c in word)


def load_cached_embeddings(db_file: str) -> dict[str, bytes]:
    """Attempts to read existing embeddings from an existing SQLite database."""
    if not os.path.exists(db_file):
        return {}

    print(f"Found existing database at '{db_file}'. Reading cached embeddings...")
    cached = {}
    try:
        conn = sqlite3.connect(db_file)
        conn.enable_load_extension(True)
        sqlite_vec.load(conn)
        cursor = conn.cursor()

        cursor.execute("""
            SELECT w.word, v.embedding
            FROM words w
            JOIN vec_words v ON w.id = v.rowid
        """)
        for word, emb_bytes in cursor.fetchall():
            if emb_bytes:
                cached[word] = bytes(emb_bytes)

        conn.close()
        print(f"Loaded {len(cached)} cached word embeddings.")
    except Exception as e:
        print(
            f"Notice: Could not load cache from existing DB ({e}). Will recompute embeddings."
        )
        cached = {}

    return cached


class LocalOnnxEmbedder:
    """Lightweight Local ONNX Embedder (zero PyTorch, zero fastembed download)"""

    def __init__(self, model_dir: str):
        model_path = os.path.join(model_dir, "model.onnx")
        if not os.path.exists(model_path):
            alt_path = os.path.join(model_dir, "model.onnix")
            if os.path.exists(alt_path):
                model_path = alt_path
            else:
                raise FileNotFoundError(
                    f"Could not find model file in '{model_dir}'. Expected 'model.onnx' or 'model.onnix'."
                )

        tokenizer_path = os.path.join(model_dir, "tokenizer.json")
        if not os.path.exists(tokenizer_path):
            raise FileNotFoundError(
                f"Could not find 'tokenizer.json' in '{model_dir}'."
            )

        print(f"Loading ONNX model from: {model_path}")
        print(f"Loading Tokenizer from: {tokenizer_path}")

        self.tokenizer = Tokenizer.from_file(tokenizer_path)
        self.session = ort.InferenceSession(model_path)

        # Detect model vector output dimension automatically
        sample_emb = self.embed(["тест"])
        self.vector_dim = sample_emb.shape[1]
        print(
            f"Model initialized successfully. Detected Vector Dimension: {self.vector_dim}"
        )

    def embed(self, texts: list[str], batch_size: int = 256) -> np.ndarray:
        all_embeddings = []

        for i in range(0, len(texts), batch_size):
            batch_texts = texts[i : i + batch_size]
            encodings = [self.tokenizer.encode(t) for t in batch_texts]
            max_len = max(len(e.ids) for e in encodings)

            b_size = len(batch_texts)
            input_ids = np.zeros((b_size, max_len), dtype=np.int64)
            attention_mask = np.zeros((b_size, max_len), dtype=np.int64)
            token_type_ids = np.zeros((b_size, max_len), dtype=np.int64)

            for idx, e in enumerate(encodings):
                l = len(e.ids)
                input_ids[idx, :l] = e.ids
                attention_mask[idx, :l] = e.attention_mask
                token_type_ids[idx, :l] = e.type_ids

            inputs = {
                "input_ids": input_ids,
                "attention_mask": attention_mask,
                "token_type_ids": token_type_ids,
            }

            outputs = self.session.run(None, inputs)
            last_hidden_state = outputs[0]  # Shape: [batch_size, seq_len, hidden_dim]

            # Mean pooling weighted by attention mask
            mask_expanded = np.expand_dims(attention_mask, -1)
            sum_embeddings = np.sum(last_hidden_state * mask_expanded, axis=1)
            sum_mask = np.clip(mask_expanded.sum(axis=1), a_min=1e-9, a_max=None)
            batch_embs = sum_embeddings / sum_mask

            # L2 Normalization
            norms = np.linalg.norm(batch_embs, axis=1, keepdims=True)
            batch_embs = batch_embs / np.maximum(norms, 1e-12)

            all_embeddings.append(batch_embs.astype(np.float32))

        return np.vstack(all_embeddings)


def main():
    parser = argparse.ArgumentParser(
        description="Build or update word database with embeddings using local ONNX model."
    )
    parser.add_argument(
        "-f",
        "--force",
        action="store_true",
        help="Force recomputation of all embeddings from scratch.",
    )
    parser.add_argument(
        "-i",
        "--input",
        default="../words.txt",
        help="Path to input text file containing words (default: ../words.txt).",
    )
    parser.add_argument(
        "-d",
        "--db",
        default="../words.db",
        help="Path to SQLite database file (default: ../words.db).",
    )
    parser.add_argument(
        "-m",
        "--model",
        default="../robert-tiny2",
        help="Path to local ONNX model directory (default: ../robert-tiny2).",
    )
    args = parser.parse_args()

    input_file = args.input
    db_file = args.db
    model_dir = args.model

    if not os.path.exists(input_file):
        print(f"Error: Input file '{input_file}' not found!")
        exit(1)

    if not os.path.exists(model_dir):
        print(f"Error: Model directory '{model_dir}' not found!")
        exit(1)

    # 1. Load cached embeddings if database exists and force option is not set
    cached_embeddings = {}
    if not args.force:
        cached_embeddings = load_cached_embeddings(db_file)
    else:
        print(
            "Force flag specified (-f/--force). Recomputing all embeddings from scratch."
        )

    # 2. Read and parse words with pymorphy3
    morph = pymorphy3.MorphAnalyzer(lang="ru")
    print(f"Reading and parsing words from '{input_file}'...")

    words_buffer = []
    seen_words = set()
    count = 0

    with open(input_file, "r", encoding="utf-8") as f:
        for line in f:
            word = line.strip().lower()
            if not is_valid_word(word) or word in seen_words:
                continue

            seen_words.add(word)
            count += 1

            parses = morph.parse(word)
            pos_str = (
                str(parses[0].tag.POS) if (parses and parses[0].tag.POS) else "UNKNOWN"
            )

            final_weight = get_custom_weight(word)
            if not final_weight:
                pos_weight, combined_pos = get_pos_weight_and_combined_pos(pos_str)
                rank_weight = get_rank_weight(count)
                len_weight = get_len_weight(len(word))

                final_weight = pos_weight * rank_weight * len_weight

            words_buffer.append((count, word, combined_pos.value, final_weight, count))

    # 3. Compute missing embeddings using Local ONNX Embedder
    missing_words = [
        item[1] for item in words_buffer if item[1] not in cached_embeddings
    ]

    if missing_words:
        print(
            f"Found {len(missing_words)} words missing embeddings. Loading ONNX model from '{model_dir}'..."
        )
        embedder = LocalOnnxEmbedder(model_dir=model_dir)
        vector_dim = embedder.vector_dim

        print(f"Embedding {len(missing_words)} words in batches...")
        embeddings = embedder.embed(missing_words, batch_size=256)

        for w, emb in zip(missing_words, embeddings):
            cached_embeddings[w] = emb.tobytes()
        print("New embeddings computed successfully.")
    else:
        print(
            "All words already have cached embeddings! Detecting vector dimension from cache..."
        )
        # Sample one cached embedding to determine vector dimension
        first_bytes = next(iter(cached_embeddings.values()))
        vector_dim = len(first_bytes) // 4

    # 4. Save/Update SQLite Database
    print(f"Writing updated database to '{db_file}' (Vector Dim: {vector_dim})...")
    conn = sqlite3.connect(db_file)
    conn.enable_load_extension(True)
    sqlite_vec.load(conn)
    cursor = conn.cursor()

    cursor.execute("DROP TABLE IF EXISTS vec_words")
    cursor.execute("DROP TABLE IF EXISTS words")

    cursor.execute("""
        CREATE TABLE words (
            id INTEGER PRIMARY KEY,
            word TEXT NOT NULL UNIQUE,
            pos TEXT NOT NULL DEFAULT 'UNKNOWN',
            weight FLOAT NOT NULL DEFAULT 1.0,
            rank INTEGER NOT NULL
        )
    """)
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_pos ON words(pos)")
    cursor.execute(
        f"CREATE VIRTUAL TABLE vec_words USING vec0(embedding float[{vector_dim}]);"
    )

    DB_COMMIT_BATCH = 2000

    for i in range(0, count, DB_COMMIT_BATCH):
        chunk_meta = words_buffer[i : i + DB_COMMIT_BATCH]

        words_db_rows = []
        vec_db_rows = []

        for item in chunk_meta:
            word_id, word, pos_val, weight, rank = item
            emb_bytes = cached_embeddings.get(word)
            if emb_bytes is None:
                continue

            words_db_rows.append((word_id, word, pos_val, weight, rank))
            vec_db_rows.append((word_id, emb_bytes))

        cursor.executemany(
            "INSERT INTO words (id, word, pos, weight, rank) VALUES (?, ?, ?, ?, ?)",
            words_db_rows,
        )
        cursor.executemany(
            "INSERT INTO vec_words (rowid, embedding) VALUES (?, ?)",
            vec_db_rows,
        )

        conn.commit()
        print(f"Processed {min(i + DB_COMMIT_BATCH, count)} / {count} words...")

    conn.close()
    print(f"\nSuccess! Updated '{db_file}' with {count} words.")


if __name__ == "__main__":
    main()
