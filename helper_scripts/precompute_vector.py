import argparse
import base64
import os

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer


class LocalOnnxEmbedder:
    """Same embedding logic as build_db.py's embedder, kept standalone here so this
    script has no dependency on the build script."""

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

        self.tokenizer = Tokenizer.from_file(tokenizer_path)
        self.session = ort.InferenceSession(model_path)

    def embed(self, texts: list[str]) -> np.ndarray:
        encodings = [self.tokenizer.encode(t) for t in texts]
        max_len = max(len(e.ids) for e in encodings)
        b_size = len(texts)

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
        last_hidden_state = outputs[0]  # [batch, seq_len, hidden_dim]

        mask_expanded = np.expand_dims(attention_mask, -1)
        sum_embeddings = np.sum(last_hidden_state * mask_expanded, axis=1)
        sum_mask = np.clip(mask_expanded.sum(axis=1), a_min=1e-9, a_max=None)
        batch_embs = sum_embeddings / sum_mask

        norms = np.linalg.norm(batch_embs, axis=1, keepdims=True)
        batch_embs = batch_embs / np.maximum(norms, 1e-12)

        return batch_embs.astype(np.float32)


def encode_f32_vec_base64(vec: np.ndarray) -> str:
    """Matches Rust's `encode_embedding`: base64 over raw little-endian f32 bytes —
    the same layout used for the `vec_words.embedding` BLOB column."""
    little_endian = vec.astype("<f4")  # ensure little-endian regardless of host arch
    return base64.b64encode(little_endian.tobytes()).decode("ascii")


def encode_f32_vec_base64(vec: np.ndarray) -> str:
    little_endian = vec.astype("<f4")
    return base64.b64encode(little_endian.tobytes()).decode("ascii")


def encode_q8_vec_base64(vec: np.ndarray) -> str:
    """Matches Rust's `encode_embedding_q8`: 4-byte LE f32 scale + one int8 per dim."""
    max_abs = float(np.max(np.abs(vec))) if vec.size else 0.0
    scale = max_abs / 127.0 if max_abs > 0.0 else 1.0
    quantized = np.clip(np.round(vec / scale), -127, 127).astype(np.int8)
    header = np.array([scale], dtype="<f4").tobytes()
    return base64.b64encode(header + quantized.tobytes()).decode("ascii")


def main():
    parser = argparse.ArgumentParser(
        description="Precompute a word/phrase embedding and print it as a base64 "
        "string usable with WordSpec::similar_to_str(...) in Rust."
    )
    parser.add_argument("text", nargs="+", help="Word or phrase(s) to embed.")
    parser.add_argument(
        "-m",
        "--model",
        default="../robert-tiny2",
        help="Path to local ONNX model directory (default: ../robert-tiny2).",
    )
    parser.add_argument(
        "--rust",
        action="store_true",
        help="Also print a ready-to-paste WordSpec snippet for each phrase.",
    )
    parser.add_argument(
        "-q",
        "--quantize",
        action="store_true",
        help="Emit the shorter int8-quantized encoding (WordSpec::similar_to_q8_str) "
        "instead of full-precision f32.",
    )
    args = parser.parse_args()

    if not os.path.exists(args.model):
        print(f"Error: Model directory '{args.model}' not found!")
        raise SystemExit(1)

    embedder = LocalOnnxEmbedder(args.model)
    texts = [t.strip().lower() for t in args.text]
    vectors = embedder.embed(texts)

    for text, vec in zip(texts, vectors):
        if args.quantize:
            encoded = encode_q8_vec_base64(vec)
            method = "similar_to_q8_str"
        else:
            encoded = encode_f32_vec_base64(vec)
            method = "similar_to_str"

        print(f"# {text}")
        print(encoded)
        if args.rust:
            print(
                f'WordSpec::new().{method}("{encoded}") // "{text}", dim={vec.shape[0]}'
            )
        print()


if __name__ == "__main__":
    main()
