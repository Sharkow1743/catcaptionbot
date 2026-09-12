import argparse
import glob
import math
import os
import sqlite3
from collections import defaultdict
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


def get_pos_weight_and_combined_pos(pos_str: str, rank: int = 1) -> tuple[float, POS]:
    """
    Maps pymorphy3 POS tags to POS enum and assigns weights.
    If a word has an UNKNOWN POS but is used enough (rank <= 25000),
    it gets a normal weight (1.0) like standard content words.
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
        case "UNKNOWN" | None:
            # If used enough (popular rank), promote to normal content word weight
            if rank <= 25000:
                return 1.0, POS.UNKNOWN
            return 0.2, POS.UNKNOWN
        case _:
            if rank <= 25000:
                return 1.0, POS.UNKNOWN
            return 0.2, POS.UNKNOWN


def get_rank_weight(rank_index: int) -> float:
    log_rank = math.log(max(1, rank_index))

    log_peak_main = math.log(2500)
    sigma_main = 1.4 if log_rank <= log_peak_main else 1.7
    diff_main = log_rank - log_peak_main
    main_curve = math.exp(-(diff_main**2) / (2 * (sigma_main**2)))

    base_weight = 0.15
    main_amplitude = 1.15

    return base_weight + (main_amplitude * main_curve)


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
        return 2.3
    return None


def is_valid_word(word: str) -> bool:
    """Ensures input word is purely Cyrillic and longer than 1 character."""
    if not word or len(word) <= 1:
        return False
    valid_chars = set("абвгдеёжзийклмнопрстуфхцчшщъыьэюя")
    return all(c in valid_chars for c in word)


def parse_leipzig_file(file_path: str) -> dict[str, int]:
    """Parses tab-separated Leipzig frequency files, splitting hyphenated words into separate parts."""
    counts = {}
    if not os.path.exists(file_path):
        return counts

    with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
        for line in f:
            parts = line.strip().split("\t")
            if len(parts) >= 3:
                raw_word = parts[1].strip().lower()
                try:
                    c = int(parts[2].strip())
                except ValueError:
                    continue

                # Divide words with '-' into separate words
                for sub_w in raw_word.split("-"):
                    if is_valid_word(sub_w):
                        counts[sub_w] = counts.get(sub_w, 0) + c
    return counts


def parse_words_txt(file_path: str) -> list[str]:
    """Parses line-by-line ranked words file, splitting hyphenated words into separate parts."""
    words = []
    if not os.path.exists(file_path):
        return words

    with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
        for line in f:
            raw_word = line.strip().lower()
            # Divide words with '-' into separate words
            for sub_w in raw_word.split("-"):
                if is_valid_word(sub_w):
                    words.append(sub_w)
    return words


def build_unified_word_list(words_dir: str, target_size: int = 50000) -> list[str]:
    """
    Combines words from different source files using weighted popularity scores:
      - words.txt (Primary Source, highest weight)
      - web-public (High weight)
      - news-2023 (Medium weight)
      - news-2024 (Very low impact)
      - wikipedia (Supplemental weight if present)
    """
    source_weights = {
        "words_txt": 10.0,      # Highest priority
        "web_public": 2.5,     # Web public corpus
        "wikipedia": 1.5,      # Wikipedia corpus
        "news_2023": 0.8,      # Moderate impact
        "news_2024": 0.05,     # Very low impact
    }

    words_txt_path = os.path.join(words_dir, "words.txt")
    web_public_files = glob.glob(os.path.join(words_dir, "*web-public*"))
    wiki_files = glob.glob(os.path.join(words_dir, "*wikipedia*"))
    news_2023_files = glob.glob(os.path.join(words_dir, "*news_2023*"))
    news_2024_files = glob.glob(os.path.join(words_dir, "*news_2024*"))

    combined_scores: dict[str, float] = defaultdict(float)

    # 1. Primary source: words.txt
    if os.path.exists(words_txt_path):
        print(f"Reading primary source: {words_txt_path}")
        raw_words = parse_words_txt(words_txt_path)
        for rank, w in enumerate(raw_words, start=1):
            rank_score = 1.0 / (rank ** 0.5)
            combined_scores[w] += rank_score * source_weights["words_txt"]
    else:
        print(f"Warning: Primary file '{words_txt_path}' not found!")

    # 2. Leipzig frequency corpora
    def add_freq_corpus(files: list[str], source_weight: float, label: str):
        for fpath in files:
            print(f"Reading corpus ({label}): {os.path.basename(fpath)}")
            counts = parse_leipzig_file(fpath)
            if not counts:
                continue
            max_c = max(counts.values())
            for w, c in counts.items():
                freq_score = math.log1p(c) / math.log1p(max_c)
                combined_scores[w] += freq_score * source_weight

    add_freq_corpus(web_public_files, source_weights["web_public"], "web-public")
    add_freq_corpus(wiki_files, source_weights["wikipedia"], "wikipedia")
    add_freq_corpus(news_2023_files, source_weights["news_2023"], "news-2023")
    add_freq_corpus(news_2024_files, source_weights["news_2024"], "news-2024 (low impact)")

    # 3. Sort by blended popularity score
    sorted_words = sorted(combined_scores.items(), key=lambda item: item[1], reverse=True)
    selected_words = [w for w, score in sorted_words[:target_size]]

    print(f"\nUnified candidate pool: {len(combined_scores)} unique split words.")
    print(f"Selected top {len(selected_words)} words (target: {target_size}).")
    return selected_words


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
        print(f"Notice: Could not load cache from existing DB ({e}). Will recompute embeddings.")
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
            raise FileNotFoundError(f"Could not find 'tokenizer.json' in '{model_dir}'.")

        print(f"Loading ONNX model from: {model_path}")
        print(f"Loading Tokenizer from: {tokenizer_path}")

        self.tokenizer = Tokenizer.from_file(tokenizer_path)
        self.session = ort.InferenceSession(model_path)

        sample_emb = self.embed(["тест"])
        self.vector_dim = sample_emb.shape[1]
        print(f"Model initialized successfully. Detected Vector Dimension: {self.vector_dim}")

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
            last_hidden_state = outputs[0]

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
        description="Build or update 50,000 word database with embeddings, splitting hyphenated words."
    )
    parser.add_argument(
        "-f",
        "--force",
        action="store_true",
        help="Force recomputation of all embeddings from scratch.",
    )
    parser.add_argument(
        "-w",
        "--words-dir",
        default="../words",
        help="Path to directory containing corpora files and words.txt (default: ../words).",
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
    parser.add_argument(
        "-s",
        "--size",
        type=int,
        default=50000,
        help="Target number of words (default: 50000).",
    )
    args = parser.parse_args()

    words_dir = args.words_dir
    db_file = args.db
    model_dir = args.model
    target_size = args.size

    if not os.path.exists(words_dir):
        print(f"Error: Words directory '{words_dir}' not found!")
        exit(1)

    if not os.path.exists(model_dir):
        print(f"Error: Model directory '{model_dir}' not found!")
        exit(1)

    # 1. Load cached embeddings if database exists
    cached_embeddings = {}
    if not args.force:
        cached_embeddings = load_cached_embeddings(db_file)
    else:
        print("Force flag specified (-f/--force). Recomputing all embeddings from scratch.")

    # 2. Build prioritized word list of surface forms (with hyphens split)
    selected_words = build_unified_word_list(words_dir, target_size=target_size)

    # 3. Only inspect POS tag of surface word (no lemmatization)
    morph = pymorphy3.MorphAnalyzer(lang="ru")
    print("\nAnalyzing POS on surface word forms and computing weights...")

    words_buffer = []
    for rank, word in enumerate(selected_words, start=1):
        parses = morph.parse(word)
        pos_str = str(parses[0].tag.POS) if (parses and parses[0].tag.POS) else "UNKNOWN"

        final_weight = get_custom_weight(word)
        if final_weight is None:
            pos_weight, combined_pos = get_pos_weight_and_combined_pos(pos_str, rank=rank)
            rank_weight = get_rank_weight(rank)
            len_weight = get_len_weight(len(word))
            final_weight = pos_weight * rank_weight * len_weight
        else:
            _, combined_pos = get_pos_weight_and_combined_pos(pos_str, rank=rank)

        words_buffer.append((rank, word, combined_pos.value, final_weight, rank))

    # 4. Compute missing embeddings
    missing_words = [item[1] for item in words_buffer if item[1] not in cached_embeddings]

    if missing_words:
        print(f"\nFound {len(missing_words)} words missing embeddings. Loading ONNX model from '{model_dir}'...")
        embedder = LocalOnnxEmbedder(model_dir=model_dir)
        vector_dim = embedder.vector_dim

        print(f"Embedding {len(missing_words)} words in batches...")
        embeddings = embedder.embed(missing_words, batch_size=256)

        for w, emb in zip(missing_words, embeddings):
            cached_embeddings[w] = emb.tobytes()
        print("New embeddings computed successfully.")
    else:
        print("\nAll words already have cached embeddings! Detecting vector dimension from cache...")
        first_bytes = next(iter(cached_embeddings.values()))
        vector_dim = len(first_bytes) // 4

    # 5. Save to SQLite Database
    print(f"\nWriting updated database to '{db_file}' (Vector Dim: {vector_dim})...")
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
    cursor.execute(f"CREATE VIRTUAL TABLE vec_words USING vec0(embedding float[{vector_dim}]);")

    DB_COMMIT_BATCH = 2000
    total_count = len(words_buffer)

    for i in range(0, total_count, DB_COMMIT_BATCH):
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
        print(f"Processed {min(i + DB_COMMIT_BATCH, total_count)} / {total_count} words...")

    conn.close()
    print(f"\nSuccess! Updated '{db_file}' with {total_count} words.")


if __name__ == "__main__":
    main()
