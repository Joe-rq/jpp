"""编码器端口：本地 bge-m3（1024 维，归一化）。召回用的精确算法底座，与判断器无关。"""
from __future__ import annotations

import hashlib
import os
import sqlite3
import threading

import numpy as np

os.environ.setdefault("OMP_NUM_THREADS", "2")


class EncPort:
    def __init__(self, cache_path: str, model: str = "BAAI/bge-m3", device: str = "mps"):
        import torch
        torch.set_num_threads(2)
        from sentence_transformers import SentenceTransformer
        self.m = SentenceTransformer(model, device=device)
        self.dim = self.m.get_sentence_embedding_dimension()
        self.db = sqlite3.connect(cache_path, check_same_thread=False)
        self.db.execute("create table if not exists enc(k text primary key, v blob)")
        self.lock = threading.Lock()
        self.n_encoded = 0

    def encode(self, texts: list[str]) -> np.ndarray:
        keys = [hashlib.sha256(t.encode()).hexdigest() for t in texts]
        out = [None] * len(texts)
        miss = []
        with self.lock:
            for i, k in enumerate(keys):
                r = self.db.execute("select v from enc where k=?", (k,)).fetchone()
                if r:
                    out[i] = np.frombuffer(r[0], dtype=np.float32)
                else:
                    miss.append(i)
            if miss:
                vs = self.m.encode([texts[i] for i in miss], batch_size=64, normalize_embeddings=True,
                                   show_progress_bar=False).astype(np.float32)
                self.n_encoded += len(miss)
                for i, v in zip(miss, vs):
                    out[i] = v
                    self.db.execute("insert or replace into enc values(?,?)", (keys[i], v.tobytes()))
                self.db.commit()
        return np.stack(out) if out else np.zeros((0, self.dim), np.float32)
